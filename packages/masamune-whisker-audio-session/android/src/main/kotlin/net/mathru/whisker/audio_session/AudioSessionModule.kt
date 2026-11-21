// Audio session module (Android). The state lives in AudioSessions; this class exposes it to
// Rust and reports the launches from the media notification as `notificationTap`.

package net.mathru.whisker.audio_session

import android.content.Intent
import android.os.Handler
import android.os.Looper
import androidx.activity.ComponentActivity
import androidx.core.util.Consumer
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.RuntimeAttachedListener
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class AudioSessionModule : Module() {
    private val main = Handler(Looper.getMainLooper())
    private var intentActivity: ComponentActivity? = null
    private val newIntentListener = Consumer<Intent> { intent -> deliverTap(intent) }
    private val attachListener = RuntimeAttachedListener { main.post { attachTapListener() } }

    init {
        AudioSessions.attach(this)
    }

    override fun definition() = ModuleDefinition {
        Name("AudioSession")
        Events("interruption", "routeChange", "command", "notificationTap")

        OnStartObserving("notificationTap") { appContext.addOnRuntimeAttachedListener(attachListener) }
        OnStopObserving("notificationTap") {
            appContext.removeOnRuntimeAttachedListener(attachListener)
            main.post {
                intentActivity?.removeOnNewIntentListener(newIntentListener)
                intentActivity = null
            }
        }

        // configure({ mixWithOthers, duckOthers, channelId, channelName, channelDescription, smallIcon })
        Function("configure") { args ->
            call { context ->
                AudioSessions.configure(context, Bridge.args(args))
                true
            }
        }

        // setActive({ active }) -> whether the app holds audio focus
        Function("setActive") { args ->
            call { context -> AudioSessions.setActive(context, Bridge.args(args).optBoolean("active")) }
        }

        // setNowPlaying({ id, title, artist, album, artworkPath, durationMs })
        Function("setNowPlaying") { args ->
            call { context ->
                AudioSessions.setNowPlaying(context, Bridge.args(args))
                true
            }
        }

        Function("clearNowPlaying") { _ ->
            call { context ->
                AudioSessions.clear(context)
                true
            }
        }

        // setPlaybackState({ playing, state, positionMs, speed })
        Function("setPlaybackState") { args ->
            call { context ->
                AudioSessions.setPlaybackState(context, Bridge.args(args))
                true
            }
        }

        // setControls({ previous, next, seek, custom: [{ id, label, androidIcon, active }] })
        Function("setControls") { args ->
            call { context ->
                AudioSessions.setControls(context, Bridge.args(args))
                true
            }
        }
    }

    internal fun emit(event: String, payload: Map<String, Any?>) {
        sendEvent(event, Bridge.event(payload))
    }

    private inline fun call(block: (android.content.Context) -> Any?) = try {
        val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
        Bridge.ok(block(context))
    } catch (error: Throwable) {
        Bridge.err(error)
    }

    private fun attachTapListener() {
        val activity = appContext.currentActivity as? ComponentActivity ?: return
        if (activity === intentActivity) return
        intentActivity?.removeOnNewIntentListener(newIntentListener)
        activity.addOnNewIntentListener(newIntentListener)
        intentActivity = activity
        // The tap that launched the app.
        activity.intent?.let { deliverTap(it) }
    }

    private fun deliverTap(intent: Intent) {
        if (!intent.getBooleanExtra(AudioSessions.EXTRA_NOTIFICATION_TAP, false)) return
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return
        intent.removeExtra(AudioSessions.EXTRA_NOTIFICATION_TAP)
        emit("notificationTap", emptyMap())
    }
}
