// Streaming speech recognition with partial results, preferring the on-device recognizer.
//
// A session listens either to the microphone (the recognizer service records it) or, on Android 13+,
// to PCM16 mono audio that the app feeds through `feed` (RecognizerIntent.EXTRA_AUDIO_SOURCE). The
// external audio mode lets an app that already records the microphone (for example a WebView with
// getUserMedia) run the recognizer without a second capture competing for the input.
//
// Every result is sent to Rust as a "speech" event ({ id, text } with the whole text so far, or
// { id, error }). The app must declare `android.permission.RECORD_AUDIO`; the microphone mode does
// not request it and fails with "permission_denied" when it is missing.

package net.mathru.whisker.speech_recognition

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.os.Build
import android.os.Bundle
import android.os.ParcelFileDescriptor
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.util.Base64
import androidx.core.content.ContextCompat
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import java.io.OutputStream
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

@WhiskerModule
class SpeechRecognitionModule : Module() {
    private var session: Session? = null

    override fun definition() = ModuleDefinition {
        Name("SpeechRecognition")
        Events("speech")

        // availability() -> { available, onDevice, externalAudio }.
        Function("availability") { _ ->
            val context = appContext.currentActivity
            val available = context != null && SpeechRecognizer.isRecognitionAvailable(context)
            val onDevice = context != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
                SpeechRecognizer.isOnDeviceRecognitionAvailable(context)
            Bridge.ok(
                mapOf(
                    "available" to (available || onDevice),
                    "onDevice" to onDevice,
                    "externalAudio" to (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU),
                ),
            )
        }

        // start({ id, language, preferOnDevice, externalAudio, sampleRate }) -> true. A running session
        // is stopped first.
        AsyncFunction("start") { args, promise ->
            val json = Bridge.args(args)
            val options = Options(
                id = json.optString("id"),
                language = json.optString("language").ifBlank { null },
                preferOnDevice = json.optBoolean("preferOnDevice", true),
                externalAudio = json.optBoolean("externalAudio", false),
                sampleRate = json.optInt("sampleRate", 16000),
            )
            Bridge.onMain {
                val context = appContext.currentActivity
                if (context == null) {
                    Bridge.reject(promise, "no activity")
                    return@onMain
                }
                if (options.externalAudio && Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
                    Bridge.reject(promise, "unsupported: external audio needs Android 13")
                    return@onMain
                }
                if (!options.externalAudio &&
                    ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) !=
                    PackageManager.PERMISSION_GRANTED
                ) {
                    Bridge.reject(promise, "permission_denied")
                    return@onMain
                }
                session?.stop()
                session = null
                val next = Session(context, options) { payload ->
                    Bridge.onMain {
                        if (session?.id == options.id) sendEvent("speech", Bridge.event(payload))
                    }
                }
                try {
                    next.start()
                    session = next
                    Bridge.resolve(promise, true)
                } catch (error: Throwable) {
                    next.stop()
                    Bridge.reject(promise, "start_failed: ${error.message ?: error.javaClass.simpleName}")
                }
            }
        }

        // feed({ id, data: base64 PCM16 }) — audio of an external audio session.
        Function("feed") { args ->
            val json = Bridge.args(args)
            val id = json.optString("id")
            val data = json.optString("data")
            val current = session
            if (current != null && current.id == id && data.isNotEmpty()) {
                current.feed(Base64.decode(data, Base64.NO_WRAP))
            }
            Bridge.ok(true)
        }

        // stop({ id }) — ends the session without further events.
        Function("stop") { args ->
            val id = Bridge.args(args).optString("id")
            Bridge.onMain {
                val current = session
                if (current != null && current.id == id) {
                    session = null
                    current.stop()
                }
            }
            Bridge.ok(true)
        }
    }
}

private data class Options(
    val id: String,
    val language: String?,
    val preferOnDevice: Boolean,
    val externalAudio: Boolean,
    val sampleRate: Int,
)

/** One recognition session. Created and driven on the main thread. */
private class Session(
    private val context: Context,
    private val options: Options,
    private val emit: (Map<String, Any?>) -> Unit,
) {
    val id = options.id

    private var recognizer: SpeechRecognizer? = null
    private var onDevice = false
    private var readSide: ParcelFileDescriptor? = null
    private var output: OutputStream? = null
    private var writer: ExecutorService? = null

    /** Japanese, Chinese and Korean text is joined without spaces. */
    private val separator =
        if (options.language?.lowercase()?.let { it.startsWith("ja") || it.startsWith("zh") || it.startsWith("ko") } == true) "" else " "

    /** Text of the finished segments. */
    private var committed = ""
    private var lastText = ""
    private var heard = false
    private var running = false

    fun start() {
        running = true
        if (options.externalAudio) {
            val pipe = ParcelFileDescriptor.createPipe()
            readSide = pipe[0]
            output = ParcelFileDescriptor.AutoCloseOutputStream(pipe[1])
            writer = Executors.newSingleThreadExecutor()
        }
        val useOnDevice = options.preferOnDevice && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
            SpeechRecognizer.isOnDeviceRecognitionAvailable(context)
        listen(useOnDevice)
    }

    private fun listen(useOnDevice: Boolean) {
        recognizer?.destroy()
        onDevice = useOnDevice
        val next = if (useOnDevice && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
        } else {
            SpeechRecognizer.createSpeechRecognizer(context)
        }
        next.setRecognitionListener(listener)
        recognizer = next
        next.startListening(intent())
    }

    private fun intent() = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
        putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
        putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
        putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
        putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
        options.language?.let { putExtra(RecognizerIntent.EXTRA_LANGUAGE, it) }
        if (options.externalAudio && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE, readSide)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_CHANNEL_COUNT, 1)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_ENCODING, AudioFormat.ENCODING_PCM_16BIT)
            putExtra(RecognizerIntent.EXTRA_AUDIO_SOURCE_SAMPLING_RATE, options.sampleRate)
            // Keep one session until the audio source closes instead of ending at the first pause.
            putExtra(RecognizerIntent.EXTRA_SEGMENTED_SESSION, RecognizerIntent.EXTRA_AUDIO_SOURCE)
        }
    }

    fun feed(data: ByteArray) {
        val stream = output ?: return
        writer?.execute {
            try {
                stream.write(data)
            } catch (_: Throwable) {
                // The recognizer closed its side; later audio has nowhere to go.
            }
        }
    }

    fun stop() {
        running = false
        recognizer?.let {
            it.setRecognitionListener(null)
            it.cancel()
            it.destroy()
        }
        recognizer = null
        val stream = output
        output = null
        writer?.execute { runCatching { stream?.close() } }
        writer?.shutdown()
        writer = null
        runCatching { readSide?.close() }
        readSide = null
    }

    private fun publish(partial: String) {
        val text = join(committed, partial, separator)
        if (text == lastText) return
        lastText = text
        emit(mapOf("id" to id, "text" to text))
    }

    private fun firstOf(results: Bundle?): String =
        results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty()

    private fun finishSegment(results: Bundle?) {
        val text = firstOf(results)
        if (text.isNotEmpty()) {
            committed = join(committed, text, separator)
            publish("")
        }
    }

    private val listener = object : RecognitionListener {
        override fun onReadyForSpeech(params: Bundle?) {}
        override fun onBeginningOfSpeech() {
            heard = true
        }
        override fun onRmsChanged(rmsdB: Float) {}
        override fun onBufferReceived(buffer: ByteArray?) {}
        override fun onEndOfSpeech() {}
        override fun onEvent(eventType: Int, params: Bundle?) {}

        override fun onPartialResults(partialResults: Bundle?) {
            if (!running) return
            val text = firstOf(partialResults)
            if (text.isNotEmpty()) publish(text)
        }

        override fun onSegmentResults(segmentResults: Bundle) {
            if (running) finishSegment(segmentResults)
        }

        override fun onEndOfSegmentedSession() {}

        override fun onResults(results: Bundle?) {
            if (!running) return
            finishSegment(results)
            // The microphone mode ends at the first pause: listen again to keep going.
            if (!options.externalAudio) listen(onDevice)
        }

        override fun onError(error: Int) {
            if (!running) return
            // The on-device model may lack the language: fall back to the default recognizer once.
            if (onDevice && !heard && isLanguageError(error)) {
                listen(false)
                return
            }
            if (!options.externalAudio &&
                (error == SpeechRecognizer.ERROR_NO_MATCH || error == SpeechRecognizer.ERROR_SPEECH_TIMEOUT)
            ) {
                listen(onDevice)
                return
            }
            running = false
            emit(mapOf("id" to id, "error" to errorName(error)))
        }
    }
}

private fun join(head: String, tail: String, separator: String): String = when {
    head.isEmpty() -> tail
    tail.isEmpty() -> head
    else -> "$head$separator$tail"
}

private fun isLanguageError(error: Int): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
        (error == SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED || error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE)

private fun errorName(error: Int): String = when (error) {
    SpeechRecognizer.ERROR_AUDIO -> "audio"
    SpeechRecognizer.ERROR_CLIENT -> "client"
    SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "permission_denied"
    SpeechRecognizer.ERROR_NETWORK, SpeechRecognizer.ERROR_NETWORK_TIMEOUT -> "network"
    SpeechRecognizer.ERROR_NO_MATCH -> "no_match"
    SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "busy"
    SpeechRecognizer.ERROR_SERVER -> "server"
    SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> "speech_timeout"
    else -> "error_$error"
}
