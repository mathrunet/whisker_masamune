// Returns the system languages and reports their changes as events.

package net.mathru.whisker.localize

import android.content.ComponentCallbacks
import android.content.res.Configuration
import android.os.Handler
import android.os.LocaleList
import android.os.Looper
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class LocalizeModule : Module() {
    /** Configuration change observer. `null` while there are no subscribers. */
    private var callbacks: ComponentCallbacks? = null

    /** Languages sent last, so unrelated configuration changes are not reported. */
    private var sent: List<Map<String, String>>? = null

    private val mainHandler = Handler(Looper.getMainLooper())

    override fun definition() = ModuleDefinition {
        Name("Localize")
        Events("languagesChanged")

        // Including locale in configChanges prevents Activity recreation, so receive the change here.
        // The current value is also sent right after subscribing.
        OnStartObserving("languagesChanged") {
            if (callbacks == null) {
                val observer = object : ComponentCallbacks {
                    override fun onConfigurationChanged(newConfig: Configuration) {
                        mainHandler.post { send(locales(newConfig.locales), force = false) }
                    }

                    @Deprecated("Deprecated in Java")
                    override fun onLowMemory() {
                    }
                }
                WhiskerApplication.appContext?.registerComponentCallbacks(observer)
                callbacks = observer
            }
            mainHandler.post { send(current(), force = true) }
        }

        OnStopObserving("languagesChanged") {
            callbacks?.let { WhiskerApplication.appContext?.unregisterComponentCallbacks(it) }
            callbacks = null
            sent = null
        }

        // languages() -> [{language, country}], most preferred first
        Function("languages") { _ ->
            try {
                Bridge.ok(current())
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }
    }

    private fun send(locales: List<Map<String, String>>, force: Boolean) {
        if (!force && locales == sent) return
        sent = locales
        sendEvent("languagesChanged", Bridge.event(mapOf("locales" to locales)))
    }

    private fun current(): List<Map<String, String>> =
        locales(WhiskerApplication.appContext?.resources?.configuration?.locales ?: LocaleList.getDefault())

    private fun locales(list: LocaleList): List<Map<String, String>> =
        (0 until list.size()).map { index ->
            val locale = list.get(index)
            mapOf("language" to locale.language, "country" to locale.country)
        }
}
