// Firebase Remote Config. Fetches and activates, then returns the values of all keys as strings.

package net.mathru.whisker.firebase_remote_config

import android.util.Log
import com.google.firebase.remoteconfig.FirebaseRemoteConfig
import com.google.firebase.remoteconfig.FirebaseRemoteConfigSettings
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class FirebaseRemoteConfigModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("FirebaseRemoteConfig")

        // fetchAndActivate({ fetchTimeoutSeconds, minimumFetchIntervalSeconds }) -> { key: stringValue }
        // Even if the fetch fails, returns the cached or default values.
        AsyncFunction("fetchAndActivate") { args, promise ->
            try {
                val json = Bridge.args(args)
                val config = FirebaseRemoteConfig.getInstance()
                config.setConfigSettingsAsync(
                    FirebaseRemoteConfigSettings.Builder()
                        .setFetchTimeoutInSeconds(json.optLong("fetchTimeoutSeconds", 60))
                        .setMinimumFetchIntervalInSeconds(json.optLong("minimumFetchIntervalSeconds", 3600))
                        .build(),
                ).continueWithTask { config.fetchAndActivate() }
                    .addOnCompleteListener { task ->
                        if (!task.isSuccessful) {
                            Log.w(TAG, "remote config fetch failed; using cached values", task.exception)
                        }
                        Bridge.resolve(promise, config.all.mapValues { it.value.asString() })
                    }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // values() -> currently active values (does not fetch)
        Function("values") { _ ->
            try {
                Bridge.ok(FirebaseRemoteConfig.getInstance().all.mapValues { it.value.asString() })
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }
    }

    private companion object {
        const val TAG = "MasamuneWhiskerRemoteConfig"
    }
}
