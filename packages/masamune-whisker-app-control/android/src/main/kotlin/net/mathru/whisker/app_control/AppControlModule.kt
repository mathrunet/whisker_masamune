// URL を外部アプリ（ブラウザ・ストア）で開く、アプリを閉じる。

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

        // openUrl({ url }) -> 開けたか（対応するアプリが無ければ false）
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

        // setOrientation({ lock }) — 画面の向きを固定する（"unspecified" で解除）。
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

        // setSystemBarsStyle({ lightStatusBar, lightNavigationBar }) — ステータスバー・ナビゲーションバーの
        // アイコンを暗い色（明るい背景用）にするか。
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

        // setWindowBackground({ argb }) — ウィンドウの背景色（起動直後や画面の切れ目に見える色）。
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

        // exitApp() — アプリを閉じる。
        Function("exitApp") { _ ->
            appContext.currentActivity?.finishAndRemoveTask()
            Bridge.ok(true)
        }
    }
}
