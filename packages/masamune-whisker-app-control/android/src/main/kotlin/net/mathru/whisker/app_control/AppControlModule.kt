// Opens URLs in external apps (browser, store), closes the app, styles the window and reports
// foreground / background changes (`lifecycle` event) from ProcessLifecycleOwner.

package net.mathru.whisker.app_control

import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.ActivityInfo
import android.graphics.drawable.ColorDrawable
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.ProcessLifecycleOwner
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerValue

@WhiskerModule
class AppControlModule : Module() {
    private val main = Handler(Looper.getMainLooper())

    /** Last state sent, so repeats are skipped. Main thread only. */
    private var lastLifecycle: String? = null

    /** True while `addObserver` replays the events up to the current state, which are not changes. */
    private var replaying = false

    private val lifecycleObserver = LifecycleEventObserver { _, event ->
        val state = when (event) {
            Lifecycle.Event.ON_START -> "foreground"
            Lifecycle.Event.ON_STOP -> "background"
            else -> return@LifecycleEventObserver
        }
        if (replaying || state == lastLifecycle) return@LifecycleEventObserver
        lastLifecycle = state
        sendEvent("lifecycle", WhiskerValue.Str(JSONObject.quote(state)))
    }

    override fun definition() = ModuleDefinition {
        Name("AppControl")
        Events("lifecycle")

        OnStartObserving("lifecycle") {
            main.post {
                val lifecycle = ProcessLifecycleOwner.get().lifecycle
                lifecycle.removeObserver(lifecycleObserver)
                lastLifecycle = currentLifecycle()
                replaying = true
                try {
                    lifecycle.addObserver(lifecycleObserver)
                } finally {
                    replaying = false
                }
            }
        }
        OnStopObserving("lifecycle") {
            main.post { ProcessLifecycleOwner.get().lifecycle.removeObserver(lifecycleObserver) }
        }

        // currentLifecycle() -> "foreground" | "background"
        Function("currentLifecycle") { _ ->
            try {
                Bridge.ok(currentLifecycle())
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // openUrl({ url }) -> whether it was opened (false if no app can handle it)
        Function("openUrl") { args ->
            try {
                val json = Bridge.args(args)
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse(json.getString("url")))
                activity.startActivity(intent)
                Bridge.ok(true)
            } catch (_: ActivityNotFoundException) {
                Bridge.ok(false)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // setOrientation({ lock }) — locks the screen orientation ("unspecified" unlocks it).
        Function("setOrientation") { args ->
            try {
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                val orientation = when (Bridge.args(args).optString("lock")) {
                    "portrait" -> ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
                    "landscape" -> ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
                    "sensorPortrait" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_PORTRAIT
                    "sensorLandscape" -> ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE
                    else -> ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED
                }
                activity.runOnUiThread { activity.requestedOrientation = orientation }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // setSystemBarsStyle({ lightStatusBar, lightNavigationBar }) — whether the status bar and navigation bar
        // use dark icons (for light backgrounds).
        Function("setSystemBarsStyle") { args ->
            try {
                val json = Bridge.args(args)
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                activity.runOnUiThread {
                    val controller = WindowInsetsControllerCompat(activity.window, activity.window.decorView)
                    controller.isAppearanceLightStatusBars = json.optBoolean("lightStatusBar")
                    controller.isAppearanceLightNavigationBars = json.optBoolean("lightNavigationBar")
                }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // setWindowBackground({ argb }) — window background color (visible right after launch and at screen edges).
        Function("setWindowBackground") { args ->
            try {
                val argb = Bridge.args(args).getLong("argb").toInt()
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                activity.runOnUiThread { activity.window.setBackgroundDrawable(ColorDrawable(argb)) }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // exitApp() — closes the app.
        Function("exitApp") { _ ->
            appContext.currentActivity?.finishAndRemoveTask()
            Bridge.ok(true)
        }
    }

    private fun currentLifecycle(): String =
        if (ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) "foreground" else "background"
}
