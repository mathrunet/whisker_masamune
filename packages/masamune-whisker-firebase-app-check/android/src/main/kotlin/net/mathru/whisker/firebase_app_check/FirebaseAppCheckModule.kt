// Firebase App Check. Release builds use Play Integrity; debug builds use the debug provider.

package net.mathru.whisker.firebase_app_check

import android.content.Context
import android.content.pm.ApplicationInfo
import com.google.firebase.FirebaseApp
import com.google.firebase.appcheck.FirebaseAppCheck
import com.google.firebase.appcheck.debug.DebugAppCheckProviderFactory
import com.google.firebase.appcheck.playintegrity.PlayIntegrityAppCheckProviderFactory
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class FirebaseAppCheckModule : Module() {
    private var activated = false

    override fun definition() = ModuleDefinition {
        Name("FirebaseAppCheck")

        // activate({ debugToken? }) — no-op after the first call.
        Function("activate") { args ->
            try {
                activate(Bridge.args(args).optString("debugToken", ""))
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // token({ forceRefresh }) -> token string
        AsyncFunction("token") { args, promise ->
            try {
                val force = Bridge.args(args).optBoolean("forceRefresh", false)
                FirebaseAppCheck.getInstance().getAppCheckToken(force)
                    .addOnSuccessListener { Bridge.resolve(promise, it.token) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }
    }

    private fun activate(debugToken: String) {
        if (activated) return
        val context = WhiskerApplication.appContext ?: throw IllegalStateException("no app context")
        val debuggable = (context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
        val appCheck = FirebaseAppCheck.getInstance()
        if (debuggable) {
            if (debugToken.isNotBlank()) {
                // DebugAppCheckProviderFactory uses the fixed token stored in SharedPreferences.
                context.getSharedPreferences(
                    "com.google.firebase.appcheck.debug.store.${FirebaseApp.getInstance().persistenceKey}",
                    Context.MODE_PRIVATE,
                ).edit().putString("com.google.firebase.appcheck.debug.DEBUG_SECRET", debugToken).apply()
            }
            appCheck.installAppCheckProviderFactory(DebugAppCheckProviderFactory.getInstance())
        } else {
            appCheck.installAppCheckProviderFactory(PlayIntegrityAppCheckProviderFactory.getInstance())
        }
        activated = true
    }
}
