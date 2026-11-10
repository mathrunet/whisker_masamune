// Returns the device language, time zone, and app info. Screen size is also reported via events whenever it changes (rotation, split screen, etc.).

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
    /** Screen size observer. `null` while there are no subscribers. */
    private var insetsRegistration: WhiskerInsetsDispatcher.Registration? = null

    /** Configuration change observer. `null` while there are no subscribers. */
    private var configurationCallbacks: ComponentCallbacks? = null

    /** Observer for configuration changes such as dark mode, language, and orientation. `null` while there are no subscribers. */
    private var appearanceCallbacks: ComponentCallbacks? = null

    private val mainHandler = Handler(Looper.getMainLooper())

    override fun definition() = ModuleDefinition {
        Name("DeviceInfo")
        Events("metricsChanged", "configurationChanged")

        // Rotation and multi-window are handled via MainActivity's configChanges without recreating the
        // Activity, so send the current window size on every configuration change. Also send it via insets
        // to cover the current value right after subscribing and Activity recreation caused by other
        // configuration changes (duplicates are dropped on the Rust side).
        OnStartObserving("metricsChanged") {
            if (insetsRegistration == null) {
                insetsRegistration = WhiskerInsetsDispatcher.addListener { _ -> sendMetrics() }
            }
            if (configurationCallbacks == null) {
                val callbacks = object : ComponentCallbacks {
                    // Send on the next main loop iteration so the window size is read after it is settled.
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

        // Changes to dark mode, language, orientation, and font scale. Including uiMode and locale in configChanges
        // prevents Activity recreation, so receive them here and let the app apply them. The current value is also sent right after subscribing.
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

    /** Dark mode, language, orientation, and font scale. */
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

    /** Size of the entire Activity window (dp). Matches the WhiskerView size because the app is edge-to-edge. */
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
