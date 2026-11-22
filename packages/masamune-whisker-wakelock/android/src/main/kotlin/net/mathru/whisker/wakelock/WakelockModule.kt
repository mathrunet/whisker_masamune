// Keeps the screen awake (Android) with FLAG_KEEP_SCREEN_ON on the current activity's window.
//
// Window flags belong to one activity, so the requested state is kept here and applied again on
// the UI thread whenever a Whisker host attaches (first launch, configuration changes, a new
// activity). A call made before any activity exists is applied once one attaches.

package net.mathru.whisker.wakelock

import android.os.Handler
import android.os.Looper
import android.view.WindowManager
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.RuntimeAttachedListener
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class WakelockModule : Module() {
    private val main = Handler(Looper.getMainLooper())

    @Volatile
    private var enabled = false
    private var listening = false
    private val attachListener = RuntimeAttachedListener { main.post { apply() } }

    override fun definition() = ModuleDefinition {
        Name("Wakelock")

        // setEnabled({ enabled }) -> the new state
        Function("setEnabled") { args ->
            try {
                enabled = Bridge.args(args).optBoolean("enabled")
                Bridge.onMain {
                    if (!listening) {
                        listening = true
                        // Fires right away when a host is attached, and on every later attach.
                        appContext.addOnRuntimeAttachedListener(attachListener)
                    } else {
                        apply()
                    }
                }
                Bridge.ok(enabled)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // isEnabled() -> the requested state
        Function("isEnabled") { _ -> Bridge.ok(enabled) }
    }

    private fun apply() {
        val window = appContext.currentActivity?.window ?: return
        if (enabled) {
            window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        } else {
            window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
    }
}
