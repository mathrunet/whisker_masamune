// URL を外部アプリ（ブラウザ・ストア）で開く、アプリを閉じる。

package net.mathru.whisker.app_control

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
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

        // exitApp() — アプリを閉じる。
        Function("exitApp") { _ ->
            appContext.currentActivity?.finishAndRemoveTask()
            Bridge.ok(true)
        }
    }
}
