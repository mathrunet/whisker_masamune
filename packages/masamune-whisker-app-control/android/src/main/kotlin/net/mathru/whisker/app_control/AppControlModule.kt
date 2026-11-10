// Opens URLs in external apps (browser, store) and closes the app.

package net.mathru.whisker.app_control

import android.content.ActivityNotFoundException
import android.content.Intent
import android.content.pm.ActivityInfo
import android.graphics.drawable.ColorDrawable
import android.net.Uri
import androidx.core.view.WindowInsetsControllerCompat
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class AppControlModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("AppControl")

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
}
