// Microphone capture as 16 kHz (configurable) mono PCM16 little-endian chunks.
//
// AudioRecord (VOICE_RECOGNITION) is read on a background thread and every chunk is sent to Rust as an
// "audio" event ({ id, data: base64 }). The app must declare `android.permission.RECORD_AUDIO` in its
// manifest (for example with the masamune-whisker-ai-debugger CNG plugin); the runtime permission is
// requested here.

package net.mathru.whisker.audio_capture

import android.Manifest
import android.annotation.SuppressLint
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.util.Base64
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise
import java.io.ByteArrayOutputStream
import java.util.concurrent.atomic.AtomicInteger

@WhiskerModule
class AudioCaptureModule : Module() {
    private val counter = AtomicInteger(0)
    private var capture: Capture? = null

    override fun definition() = ModuleDefinition {
        Name("AudioCapture")
        Events("audio")

        // start({ id, sampleRate, chunkMs }) -> true. Rejects with "permission_denied" when the user
        // denies the microphone. A running capture is stopped first.
        AsyncFunction("start") { args, promise ->
            val json = Bridge.args(args)
            val id = json.optString("id")
            val sampleRate = json.optInt("sampleRate", 16000)
            val chunkMs = json.optInt("chunkMs", 100)
            Bridge.onMain {
                val activity = appContext.currentActivity as? ComponentActivity
                if (activity == null) {
                    Bridge.reject(promise, "no activity")
                    return@onMain
                }
                if (ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
                    PackageManager.PERMISSION_GRANTED
                ) {
                    begin(id, sampleRate, chunkMs, promise)
                    return@onMain
                }
                var launcher: ActivityResultLauncher<String>? = null
                launcher = activity.activityResultRegistry.register(
                    "masamune_whisker_audio_capture_${counter.incrementAndGet()}",
                    ActivityResultContracts.RequestPermission(),
                ) { granted ->
                    launcher?.unregister()
                    if (granted) begin(id, sampleRate, chunkMs, promise) else Bridge.reject(promise, "permission_denied")
                }
                launcher.launch(Manifest.permission.RECORD_AUDIO)
            }
        }

        // stop({ id }) -> { data: base64 of the audio after the last chunk } | null (not running).
        AsyncFunction("stop") { args, promise ->
            val id = Bridge.args(args).optString("id")
            Bridge.onMain {
                val rest = finish(id)
                Bridge.resolve(promise, rest?.let { mapOf("data" to Base64.encodeToString(it, Base64.NO_WRAP)) })
            }
        }

        // cancel({ id }) — stops without returning the remaining audio.
        Function("cancel") { args ->
            val id = Bridge.args(args).optString("id")
            Bridge.onMain { finish(id) }
            Bridge.ok(true)
        }
    }

    private fun begin(id: String, sampleRate: Int, chunkMs: Int, promise: WhiskerPromise) {
        capture?.stop()
        capture = null
        val chunkBytes = maxOf(2, sampleRate * 2 * chunkMs / 1000)
        val next = Capture(id, sampleRate, chunkBytes, { data ->
            Bridge.onMain {
                if (capture?.id == id) {
                    sendEvent("audio", Bridge.event(mapOf("id" to id, "data" to Base64.encodeToString(data, Base64.NO_WRAP))))
                }
            }
        }, { message ->
            Bridge.onMain {
                if (capture?.id == id) {
                    capture?.stop()
                    capture = null
                    sendEvent("audio", Bridge.event(mapOf("id" to id, "error" to message)))
                }
            }
        })
        try {
            next.start()
            capture = next
            Bridge.resolve(promise, true)
        } catch (error: Throwable) {
            next.stop()
            Bridge.reject(promise, error)
        }
    }

    /** Stops the capture [id] and returns its remaining audio, or null when it is not running. */
    private fun finish(id: String): ByteArray? {
        val current = capture ?: return null
        if (current.id != id) return null
        capture = null
        return current.stop()
    }
}

/** One capture: an AudioRecord read on its own thread. */
private class Capture(
    val id: String,
    private val sampleRate: Int,
    private val chunkBytes: Int,
    private val emit: (ByteArray) -> Unit,
    private val fail: (String) -> Unit,
) {
    private var record: AudioRecord? = null
    private var thread: Thread? = null
    private val pending = ByteArrayOutputStream()

    @Volatile
    private var running = false

    @SuppressLint("MissingPermission")
    fun start() {
        val minSize = AudioRecord.getMinBufferSize(sampleRate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (minSize <= 0) throw IllegalStateException("start_failed: the microphone input format is not supported")
        val recorder = AudioRecord(
            MediaRecorder.AudioSource.VOICE_RECOGNITION,
            sampleRate,
            AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
            maxOf(minSize, chunkBytes) * 2,
        )
        if (recorder.state != AudioRecord.STATE_INITIALIZED) {
            recorder.release()
            throw IllegalStateException("start_failed: the microphone could not be initialized")
        }
        recorder.startRecording()
        record = recorder
        running = true
        thread = Thread({ read(recorder) }, "masamune_whisker_audio_capture").apply { start() }
    }

    private fun read(recorder: AudioRecord) {
        // Small reads keep the stop latency low; full chunks are emitted from `pending`.
        val buffer = ByteArray(maxOf(2, chunkBytes / 5) and 1.inv())
        while (running) {
            val count = recorder.read(buffer, 0, buffer.size)
            if (count < 0) {
                if (running) fail("read_failed: $count")
                break
            }
            if (count == 0) continue
            val chunks = mutableListOf<ByteArray>()
            synchronized(pending) {
                pending.write(buffer, 0, count)
                if (pending.size() >= chunkBytes) {
                    val all = pending.toByteArray()
                    var offset = 0
                    while (all.size - offset >= chunkBytes) {
                        chunks.add(all.copyOfRange(offset, offset + chunkBytes))
                        offset += chunkBytes
                    }
                    pending.reset()
                    pending.write(all, offset, all.size - offset)
                }
            }
            chunks.forEach(emit)
        }
    }

    /** Stops recording and returns the samples that did not fill a chunk yet. */
    fun stop(): ByteArray {
        running = false
        val recorder = record
        record = null
        try {
            recorder?.stop()
        } catch (_: IllegalStateException) {
        }
        thread?.join(500)
        thread = null
        recorder?.release()
        return synchronized(pending) {
            val rest = pending.toByteArray()
            pending.reset()
            rest
        }
    }
}
