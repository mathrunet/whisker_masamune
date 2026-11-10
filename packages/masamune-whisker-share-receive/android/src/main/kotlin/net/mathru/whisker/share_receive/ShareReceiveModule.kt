// Receives ACTION_SEND / ACTION_SEND_MULTIPLE intents (the CNG plugin adds the intent filters).
//
// The launch intent is read by `getInitialShare`; later intents arrive through the activity's
// onNewIntent listener and are sent as the `share` event. Shared streams (content:// or file://)
// are copied into `<cacheDir>/masamune_whisker_share_receive/` off the main thread, and the
// absolute paths are returned. Each intent is marked as consumed so it is delivered once, and
// intents relaunched from the recents screen are ignored.

package net.mathru.whisker.share_receive

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.Parcelable
import android.provider.OpenableColumns
import android.util.Log
import android.webkit.MimeTypeMap
import androidx.activity.ComponentActivity
import androidx.core.util.Consumer
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.RuntimeAttachedListener
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise
import java.io.File
import java.util.concurrent.Executors

@WhiskerModule
class ShareReceiveModule : Module() {
    private val main = Handler(Looper.getMainLooper())
    private val io = Executors.newSingleThreadExecutor()

    /** Activity whose onNewIntent is observed, while `share` has subscribers. */
    private var intentActivity: ComponentActivity? = null
    private val newIntentListener = Consumer<Intent> { intent -> deliver(intent) }
    private val attachListener = RuntimeAttachedListener { main.post { attachIntentListener() } }

    override fun definition() = ModuleDefinition {
        Name("ShareReceive")
        Events("share")

        OnStartObserving("share") { appContext.addOnRuntimeAttachedListener(attachListener) }
        OnStopObserving("share") {
            appContext.removeOnRuntimeAttachedListener(attachListener)
            main.post {
                intentActivity?.removeOnNewIntentListener(newIntentListener)
                intentActivity = null
            }
        }

        // getInitialShare() -> { text, imagePaths } | null — waits for the host activity, then consumes
        // its launch intent.
        AsyncFunction("getInitialShare") { _, promise ->
            var listener: RuntimeAttachedListener? = null
            val pending = RuntimeAttachedListener {
                main.post {
                    val current = listener ?: return@post
                    listener = null
                    appContext.removeOnRuntimeAttachedListener(current)
                    readInitial(promise)
                }
            }
            listener = pending
            appContext.addOnRuntimeAttachedListener(pending)
        }
    }

    private fun attachIntentListener() {
        val activity = appContext.currentActivity as? ComponentActivity ?: return
        if (activity === intentActivity) return
        intentActivity?.removeOnNewIntentListener(newIntentListener)
        activity.addOnNewIntentListener(newIntentListener)
        intentActivity = activity
    }

    private fun readInitial(promise: WhiskerPromise) {
        val activity = appContext.currentActivity
        val intent = activity?.intent
        if (activity == null || intent == null || !claim(intent)) {
            Bridge.resolve(promise, null)
            return
        }
        val context = activity.applicationContext
        io.execute {
            try {
                Bridge.resolve(promise, read(context, intent))
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }
    }

    private fun deliver(intent: Intent) {
        if (!claim(intent)) return
        val context = intentActivity?.applicationContext ?: return
        io.execute {
            val payload = try {
                read(context, intent)
            } catch (error: Throwable) {
                Log.w(TAG, "failed to read the shared content: ${error.javaClass.simpleName}")
                null
            } ?: return@execute
            main.post { sendEvent("share", Bridge.event(payload)) }
        }
    }

    /** Marks a share intent as consumed. Returns false when it is not one, or was handled. */
    private fun claim(intent: Intent): Boolean {
        if (intent.action != Intent.ACTION_SEND && intent.action != Intent.ACTION_SEND_MULTIPLE) return false
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return false
        if (intent.getBooleanExtra(CONSUMED, false)) return false
        intent.putExtra(CONSUMED, true)
        return true
    }

    private fun read(context: Context, intent: Intent): Map<String, Any?>? {
        val text = (intent.getCharSequenceExtra(Intent.EXTRA_TEXT) ?: intent.getStringExtra(Intent.EXTRA_TEXT))
            ?.toString()
            ?.takeIf { it.isNotEmpty() }
        val directory = File(context.cacheDir, DIRECTORY).apply { mkdirs() }
        cleanUp(directory)
        val stamp = System.currentTimeMillis()
        val paths = streams(intent).mapIndexedNotNull { index, uri ->
            try {
                copy(context, uri, directory, "${stamp}_$index")
            } catch (error: Throwable) {
                Log.w(TAG, "failed to copy a shared stream: ${error.javaClass.simpleName}")
                null
            }
        }
        if (text == null && paths.isEmpty()) return null
        return mapOf("text" to text, "imagePaths" to paths)
    }

    private fun streams(intent: Intent): List<Uri> {
        val uris = mutableListOf<Uri>()
        if (intent.action == Intent.ACTION_SEND_MULTIPLE) {
            uris += parcelableList(intent)
        } else {
            parcelable(intent)?.let { uris += it }
        }
        if (uris.isEmpty()) {
            val clip = intent.clipData
            if (clip != null && intent.type?.startsWith("image/") == true) {
                for (i in 0 until clip.itemCount) clip.getItemAt(i).uri?.let { uris += it }
            }
        }
        return uris.distinct()
    }

    private fun parcelable(intent: Intent): Uri? =
        if (Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra<Parcelable>(Intent.EXTRA_STREAM) as? Uri
        }

    private fun parcelableList(intent: Intent): List<Uri> =
        if (Build.VERSION.SDK_INT >= 33) {
            intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        } else {
            @Suppress("DEPRECATION")
            intent.getParcelableArrayListExtra<Parcelable>(Intent.EXTRA_STREAM).orEmpty().filterIsInstance<Uri>()
        }

    /** Copies a shared stream into `directory`, keeping its extension. Returns the absolute path. */
    private fun copy(context: Context, uri: Uri, directory: File, prefix: String): String? {
        val resolver = context.contentResolver
        var name: String? = null
        if (uri.scheme == "content") {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
                if (cursor.moveToFirst() && !cursor.isNull(0)) name = cursor.getString(0)
            }
        } else {
            name = uri.lastPathSegment
        }
        var ext = name?.substringAfterLast('.', "")?.lowercase().orEmpty()
        if (ext.isEmpty() || ext.length > 8 || !ext.all { it.isLetterOrDigit() }) {
            val mime = resolver.getType(uri)
            ext = mime?.let { MimeTypeMap.getSingleton().getExtensionFromMimeType(it) }.orEmpty()
        }
        val target = File(directory, if (ext.isEmpty()) prefix else "$prefix.$ext")
        val input = resolver.openInputStream(uri) ?: return null
        input.use { source -> target.outputStream().use { source.copyTo(it) } }
        return target.absolutePath
    }

    /** Deletes copies older than a day; the app is expected to have used them by then. */
    private fun cleanUp(directory: File) {
        val limit = System.currentTimeMillis() - 24 * 60 * 60 * 1000L
        directory.listFiles()?.forEach { if (it.lastModified() < limit) it.delete() }
    }

    private companion object {
        const val TAG = "MasamuneShareReceive"
        const val DIRECTORY = "masamune_whisker_share_receive"
        const val CONSUMED = "net.mathru.whisker.share_receive.CONSUMED"
    }
}
