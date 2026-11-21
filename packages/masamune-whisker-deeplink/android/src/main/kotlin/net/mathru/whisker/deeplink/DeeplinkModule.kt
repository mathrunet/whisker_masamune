// Receives ACTION_VIEW intents (custom schemes and App Links; the CNG plugin adds the intent
// filters and sets launchMode="singleTask").
//
// The launch intent is read by `getInitialLink`; later intents arrive through the activity's
// onNewIntent listener and are sent as the `link` event. Each intent is marked as consumed so it
// is delivered once, and intents relaunched from the recents screen are ignored.

package net.mathru.whisker.deeplink

import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.core.util.Consumer
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.RuntimeAttachedListener
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerValue

@WhiskerModule
class DeeplinkModule : Module() {
    private val main = Handler(Looper.getMainLooper())

    /** Activity whose onNewIntent is observed, while `link` has subscribers. */
    private var intentActivity: ComponentActivity? = null
    private val newIntentListener = Consumer<Intent> { intent -> deliver(intent) }
    private val attachListener = RuntimeAttachedListener { main.post { attachIntentListener() } }

    override fun definition() = ModuleDefinition {
        Name("Deeplink")
        Events("link")

        OnStartObserving("link") { appContext.addOnRuntimeAttachedListener(attachListener) }
        OnStopObserving("link") {
            appContext.removeOnRuntimeAttachedListener(attachListener)
            main.post {
                intentActivity?.removeOnNewIntentListener(newIntentListener)
                intentActivity = null
            }
        }

        // getInitialLink() -> string | null — waits for the host activity, then consumes its launch intent.
        AsyncFunction("getInitialLink") { _, promise ->
            var listener: RuntimeAttachedListener? = null
            val pending = RuntimeAttachedListener {
                main.post {
                    val current = listener ?: return@post
                    listener = null
                    appContext.removeOnRuntimeAttachedListener(current)
                    val intent = appContext.currentActivity?.intent
                    Bridge.resolve(promise, intent?.let { claim(it) })
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

    private fun deliver(intent: Intent) {
        val link = claim(intent) ?: return
        sendEvent("link", WhiskerValue.Str(JSONObject.quote(link)))
    }

    /** Marks a VIEW intent as consumed and returns its URL. Null when it is not one, or was handled. */
    private fun claim(intent: Intent): String? {
        if (intent.action != Intent.ACTION_VIEW) return null
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return null
        if (intent.getBooleanExtra(CONSUMED, false)) return null
        val link = intent.dataString?.takeIf { it.isNotEmpty() } ?: return null
        intent.putExtra(CONSUMED, true)
        return link
    }

    private companion object {
        const val CONSUMED = "net.mathru.whisker.deeplink.CONSUMED"
    }
}
