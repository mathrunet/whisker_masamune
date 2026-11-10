// 端末の言語・タイムゾーン・アプリ情報を返す。画面サイズは回転・分割画面などで変わるたびにイベントでも通知する。

package net.mathru.whisker.device_info

import android.app.Activity
import android.content.ComponentCallbacks
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.os.Build
import android.os.Handler
import android.os.LocaleList
import android.os.Looper
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerInsetsDispatcher
import rs.whisker.runtime.WhiskerModule
import java.util.Locale
import java.util.TimeZone

@WhiskerModule
class DeviceInfoModule : Module() {
    /** 画面サイズの監視。購読が無い間は null。 */
    private var insetsRegistration: WhiskerInsetsDispatcher.Registration? = null

    /** 構成変更の監視。購読が無い間は null。 */
    private var configurationCallbacks: ComponentCallbacks? = null

    /** ダークモード・言語・向きなどの構成変更の監視。購読が無い間は null。 */
    private var appearanceCallbacks: ComponentCallbacks? = null

    private val mainHandler = Handler(Looper.getMainLooper())

    override fun definition() = ModuleDefinition {
        Name("DeviceInfo")
        Events("metricsChanged", "configurationChanged")

        // 回転・マルチウィンドウは MainActivity の configChanges で作り直さずに受けるため、
        // 構成変更のたびに今のウィンドウサイズを送る。インセット経由でも送り、購読直後の現在値と
        // それ以外の構成変更で Activity が作り直された場合も拾う（重複は Rust 側で捨てる）。
        OnStartObserving("metricsChanged") {
            if (insetsRegistration == null) {
                insetsRegistration = WhiskerInsetsDispatcher.addListener { _ -> sendMetrics() }
            }
            if (configurationCallbacks == null) {
                val callbacks = object : ComponentCallbacks {
                    // ウィンドウの大きさが確定してから読むため、次のメインループで送る。
                    override fun onConfigurationChanged(newConfig: Configuration) {
                        mainHandler.post { sendMetrics() }
                    }

                    @Deprecated("Deprecated in Java")
                    override fun onLowMemory() {
                    }
                }
                WhiskerApplication.appContext?.registerComponentCallbacks(callbacks)
                configurationCallbacks = callbacks
            }
        }

        OnStopObserving("metricsChanged") {
            insetsRegistration?.let { WhiskerInsetsDispatcher.removeListener(it) }
            insetsRegistration = null
            configurationCallbacks?.let { WhiskerApplication.appContext?.unregisterComponentCallbacks(it) }
            configurationCallbacks = null
        }

        // ダークモード・言語・向き・文字の大きさの変化。uiMode・locale を configChanges に含めると
        // Activity が作り直されないため、ここで受けてアプリ側で反映する。購読直後にも現在値を送る。
        OnStartObserving("configurationChanged") {
            if (appearanceCallbacks == null) {
                val callbacks = object : ComponentCallbacks {
                    override fun onConfigurationChanged(newConfig: Configuration) {
                        mainHandler.post { sendConfiguration(newConfig) }
                    }

                    @Deprecated("Deprecated in Java")
                    override fun onLowMemory() {
                    }
                }
                WhiskerApplication.appContext?.registerComponentCallbacks(callbacks)
                appearanceCallbacks = callbacks
            }
            mainHandler.post { WhiskerApplication.appContext?.resources?.configuration?.let { sendConfiguration(it) } }
        }

        OnStopObserving("configurationChanged") {
            appearanceCallbacks?.let { WhiskerApplication.appContext?.unregisterComponentCallbacks(it) }
            appearanceCallbacks = null
        }

        // info() -> { locales: [{language, country}], timezone, offsetMinutes, packageName, versionName, versionCode, sdkInt, debuggable }
        Function("info") { _ ->
            try {
                Bridge.ok(info())
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }
    }

    private fun sendMetrics() {
        val activity = appContext.currentActivity ?: return
        sendEvent("metricsChanged", Bridge.event(metrics(activity)))
    }

    private fun sendConfiguration(config: Configuration) {
        val activity = appContext.currentActivity
        val size = activity?.let { metrics(it) } ?: emptyMap()
        sendEvent("configurationChanged", Bridge.event(size + configuration(config)))
    }

    /** ダークモード・言語・向き・文字の大きさ。 */
    private fun configuration(config: Configuration): Map<String, Any?> {
        val locales = config.locales
        val list = (0 until locales.size()).map { index ->
            val locale: Locale = locales.get(index)
            mapOf("language" to locale.language, "country" to locale.country)
        }
        return mapOf(
            "nightMode" to ((config.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES),
            "orientation" to if (config.orientation == Configuration.ORIENTATION_LANDSCAPE) "landscape" else "portrait",
            "fontScale" to config.fontScale.toDouble(),
            "locales" to list,
        )
    }

    private fun info(): Map<String, Any?> {
        val context = WhiskerApplication.appContext
        val locales = LocaleList.getDefault()
        val list = (0 until locales.size()).map { index ->
            val locale: Locale = locales.get(index)
            mapOf("language" to locale.language, "country" to locale.country)
        }
        val zone = TimeZone.getDefault()
        var versionName: String? = null
        var versionCode: Long? = null
        var debuggable = false
        if (context != null) {
            try {
                val info = context.packageManager.getPackageInfo(context.packageName, 0)
                versionName = info.versionName
                versionCode = if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else {
                    @Suppress("DEPRECATION")
                    info.versionCode.toLong()
                }
            } catch (_: PackageManager.NameNotFoundException) {
            }
            debuggable = (context.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0
        }
        val activity = appContext.currentActivity
        val metrics = context?.resources?.displayMetrics
        val size = activity?.let { metrics(it) } ?: mapOf(
            "widthDp" to (metrics?.let { it.widthPixels / it.density } ?: 0f).toDouble(),
            "heightDp" to (metrics?.let { it.heightPixels / it.density } ?: 0f).toDouble(),
            "density" to (metrics?.density ?: 1f).toDouble(),
        )
        val appearance = context?.resources?.configuration?.let { configuration(it) } ?: emptyMap()
        return size + appearance + mapOf(
            "locales" to list,
            "timezone" to zone.id,
            "offsetMinutes" to zone.getOffset(System.currentTimeMillis()) / 60000,
            "packageName" to context?.packageName,
            "versionName" to versionName,
            "versionCode" to versionCode,
            "sdkInt" to Build.VERSION.SDK_INT,
            "debuggable" to debuggable,
        )
    }

    /** Activity のウィンドウ全体の大きさ（dp）。edge-to-edge のため WhiskerView の大きさと一致する。 */
    private fun metrics(activity: Activity): Map<String, Any?> {
        val display = activity.resources.displayMetrics
        val density = display.density.takeIf { it > 0f } ?: 1f
        val (width, height) = if (Build.VERSION.SDK_INT >= 30) {
            val bounds = activity.windowManager.currentWindowMetrics.bounds
            bounds.width() to bounds.height()
        } else {
            display.widthPixels to display.heightPixels
        }
        return mapOf(
            "widthDp" to (width / density).toDouble(),
            "heightDp" to (height / density).toDouble(),
            "density" to density.toDouble(),
        )
    }
}
