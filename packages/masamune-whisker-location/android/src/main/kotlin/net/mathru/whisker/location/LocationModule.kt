// Location permission, single fixes and the position stream (Android, framework LocationManager).
//
// Providers: the fused provider (Android 12+), then GPS when the precise permission is granted,
// then network. Background updates start LocationForegroundService (type location) before
// registering the listener; the service shows either an existing notification of the app (by id)
// or its own.
//
// Android cannot tell "never asked" from "denied", so the module remembers in SharedPreferences
// that it asked.

package net.mathru.whisker.location

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise
import java.util.concurrent.atomic.AtomicInteger

@WhiskerModule
class LocationModule : Module() {
    private val main = Handler(Looper.getMainLooper())
    private val requestCounter = AtomicInteger(0)
    private var listener: LocationListener? = null

    override fun definition() = ModuleDefinition {
        Name("Location")
        Events("update")

        Function("status") { _ ->
            Bridge.ok(context()?.let { status(it) } ?: "denied")
        }

        AsyncFunction("requestPermission") { _, promise ->
            request(promise, arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION))
        }

        AsyncFunction("requestBackgroundPermission") { _, promise ->
            val context = context()
            when {
                context == null -> Bridge.reject(promise, "no context")
                Build.VERSION.SDK_INT < 29 -> Bridge.resolve(promise, status(context))
                !foregroundGranted(context) -> Bridge.resolve(promise, status(context))
                else -> request(promise, arrayOf(Manifest.permission.ACCESS_BACKGROUND_LOCATION))
            }
        }

        Function("precise") { _ ->
            Bridge.ok(context()?.let { granted(it, Manifest.permission.ACCESS_FINE_LOCATION) } ?: false)
        }

        Function("serviceEnabled") { _ ->
            Bridge.ok(context()?.let { enabled(it) } ?: false)
        }

        Function("openAppSettings") { _ ->
            open { context ->
                Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}"))
            }
        }

        Function("openLocationSettings") { _ ->
            open { Intent(Settings.ACTION_LOCATION_SOURCE_SETTINGS) }
        }

        // currentPosition({ accuracy, timeoutMs }) -> position
        AsyncFunction("currentPosition") { args, promise ->
            val json = Bridge.args(args)
            Bridge.onMain { currentPosition(json, promise) }
        }

        // lastKnownPosition() -> position | null
        AsyncFunction("lastKnownPosition") { _, promise ->
            Bridge.onMain {
                val context = context()
                if (context == null || !foregroundGranted(context)) {
                    Bridge.resolve(promise, null)
                    return@onMain
                }
                val manager = manager(context)
                val latest = try {
                    manager.getProviders(true).mapNotNull { manager.getLastKnownLocation(it) }.maxByOrNull { it.time }
                } catch (_: SecurityException) {
                    null
                }
                Bridge.resolve(promise, latest?.let { payload(it) })
            }
        }

        // startUpdates({ accuracy, distanceFilter, intervalMs, background, androidNotification })
        Function("startUpdates") { args ->
            val json = Bridge.args(args)
            Bridge.onMain { startUpdates(json) }
            Bridge.ok(true)
        }

        Function("stopUpdates") { _ ->
            Bridge.onMain { stopUpdates() }
            Bridge.ok(true)
        }
    }

    // MARK: - Permission

    private fun context(): Context? = WhiskerApplication.appContext

    private fun granted(context: Context, permission: String) =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    private fun foregroundGranted(context: Context) =
        granted(context, Manifest.permission.ACCESS_FINE_LOCATION) || granted(context, Manifest.permission.ACCESS_COARSE_LOCATION)

    private fun status(context: Context): String = when {
        foregroundGranted(context) &&
            (Build.VERSION.SDK_INT < 29 || granted(context, Manifest.permission.ACCESS_BACKGROUND_LOCATION)) -> "always"
        foregroundGranted(context) -> "whenInUse"
        !prefs(context).getBoolean(ASKED, false) -> "notDetermined"
        else -> "denied"
    }

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private fun request(promise: WhiskerPromise, permissions: Array<String>) {
        val context = context()
        val activity = appContext.currentActivity as? ComponentActivity
        if (context == null || activity == null) {
            Bridge.reject(promise, "no activity")
            return
        }
        if (permissions.all { granted(context, it) }) {
            Bridge.resolve(promise, status(context))
            return
        }
        Bridge.onMain {
            prefs(context).edit().putBoolean(ASKED, true).apply()
            val key = "masamune_whisker_location_permission_${requestCounter.incrementAndGet()}"
            var launcher: ActivityResultLauncher<Array<String>>? = null
            launcher = activity.activityResultRegistry.register(key, ActivityResultContracts.RequestMultiplePermissions()) {
                launcher?.unregister()
                Bridge.resolve(promise, status(context))
            }
            launcher.launch(permissions)
        }
    }

    private fun open(intent: (Context) -> Intent): rs.whisker.runtime.WhiskerValue = try {
        val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
        try {
            activity.startActivity(intent(activity))
        } catch (_: ActivityNotFoundException) {
            activity.startActivity(
                Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}")),
            )
        }
        Bridge.ok(true)
    } catch (error: Throwable) {
        Bridge.err(error)
    }

    // MARK: - Providers

    private fun manager(context: Context) = context.getSystemService(Context.LOCATION_SERVICE) as LocationManager

    private fun enabled(context: Context): Boolean {
        val manager = manager(context)
        return if (Build.VERSION.SDK_INT >= 28) {
            manager.isLocationEnabled
        } else {
            manager.isProviderEnabled(LocationManager.GPS_PROVIDER) || manager.isProviderEnabled(LocationManager.NETWORK_PROVIDER)
        }
    }

    /** The provider for `accuracy`, or null when none is enabled. */
    private fun provider(context: Context, accuracy: String): String? {
        val providers = manager(context).getProviders(true)
        val fine = granted(context, Manifest.permission.ACCESS_FINE_LOCATION)
        val coarse = accuracy == "lowest" || accuracy == "low"
        return when {
            coarse && LocationManager.NETWORK_PROVIDER in providers -> LocationManager.NETWORK_PROVIDER
            Build.VERSION.SDK_INT >= 31 && LocationManager.FUSED_PROVIDER in providers -> LocationManager.FUSED_PROVIDER
            fine && LocationManager.GPS_PROVIDER in providers -> LocationManager.GPS_PROVIDER
            LocationManager.NETWORK_PROVIDER in providers -> LocationManager.NETWORK_PROVIDER
            LocationManager.PASSIVE_PROVIDER in providers -> LocationManager.PASSIVE_PROVIDER
            else -> null
        }
    }

    /** The reason updates cannot run, or null. */
    private fun unavailable(context: Context): Pair<String, String>? = when {
        !foregroundGranted(context) -> "permissionDenied" to "location permission denied"
        !enabled(context) -> "serviceDisabled" to "location services are disabled"
        else -> null
    }

    // MARK: - Single fix

    private fun currentPosition(json: JSONObject, promise: WhiskerPromise) {
        val context = context() ?: return Bridge.reject(promise, "no context")
        unavailable(context)?.let { return Bridge.reject(promise, it.second) }
        val provider = provider(context, json.optString("accuracy", "high"))
            ?: return Bridge.reject(promise, "location services are disabled")
        val manager = manager(context)
        var done = false
        val cancel = CancellationSignal()
        var single: LocationListener? = null
        fun finish(location: Location?, error: String?) {
            if (done) return
            done = true
            cancel.cancel()
            single?.let { manager.removeUpdates(it) }
            if (location != null) Bridge.resolve(promise, payload(location)) else Bridge.reject(promise, error ?: "no location")
        }
        main.postDelayed({ finish(null, "location timeout") }, json.optLong("timeoutMs", 10_000).coerceAtLeast(100))
        try {
            if (Build.VERSION.SDK_INT >= 30) {
                manager.getCurrentLocation(provider, cancel, ContextCompat.getMainExecutor(context)) { location ->
                    finish(location, "no location")
                }
            } else {
                val listener = object : LocationListener {
                    override fun onLocationChanged(location: Location) = finish(location, null)
                    override fun onProviderDisabled(provider: String) = finish(null, "location services are disabled")
                    override fun onProviderEnabled(provider: String) {}

                    @Deprecated("Deprecated in Java")
                    override fun onStatusChanged(provider: String?, status: Int, extras: Bundle?) {}
                }
                single = listener
                @Suppress("DEPRECATION")
                manager.requestSingleUpdate(provider, listener, Looper.getMainLooper())
            }
        } catch (error: SecurityException) {
            finish(null, "location permission denied")
        } catch (error: Throwable) {
            finish(null, error.message ?: error.javaClass.simpleName)
        }
    }

    // MARK: - Stream

    private fun startUpdates(json: JSONObject) {
        stopUpdates()
        val context = context() ?: return
        unavailable(context)?.let { return fail(it.first, it.second) }
        val provider = provider(context, json.optString("accuracy", "high"))
            ?: return fail("serviceDisabled", "location services are disabled")
        if (json.optBoolean("background")) {
            val started = LocationForegroundService.start(context, json.optJSONObject("androidNotification") ?: JSONObject()) { error ->
                main.post {
                    stopUpdates()
                    fail("backgroundUnavailable", error)
                }
            }
            if (!started) return fail("backgroundUnavailable", "the location service cannot start while the app is in the background")
        }
        val listener = object : LocationListener {
            override fun onLocationChanged(location: Location) {
                if (listener !== this) return
                sendEvent("update", Bridge.event(payload(location) + ("type" to "position")))
            }

            override fun onProviderDisabled(value: String) {
                if (listener !== this || value != provider) return
                stopUpdates()
                fail("serviceDisabled", "location services are disabled")
            }

            override fun onProviderEnabled(provider: String) {}

            @Deprecated("Deprecated in Java")
            override fun onStatusChanged(provider: String?, status: Int, extras: Bundle?) {}
        }
        try {
            manager(context).requestLocationUpdates(
                provider,
                json.optLong("intervalMs", 10_000).coerceAtLeast(0),
                json.optDouble("distanceFilter", 0.0).toFloat().coerceAtLeast(0f),
                listener,
                Looper.getMainLooper(),
            )
            this.listener = listener
        } catch (_: SecurityException) {
            stopUpdates()
            fail("permissionDenied", "location permission denied")
        } catch (error: Throwable) {
            stopUpdates()
            fail("failed", error.message ?: error.javaClass.simpleName)
        }
    }

    private fun stopUpdates() {
        val context = context()
        listener?.let { current -> context?.let { manager(it).removeUpdates(current) } }
        listener = null
        context?.let { LocationForegroundService.stop(it) }
    }

    private fun fail(code: String, message: String) {
        sendEvent("update", Bridge.event(mapOf("type" to "error", "code" to code, "message" to message)))
    }

    private fun payload(location: Location): Map<String, Any?> = mapOf(
        "latitude" to location.latitude,
        "longitude" to location.longitude,
        "accuracy" to location.accuracy.toDouble(),
        "altitude" to location.altitude,
        "speed" to location.speed.toDouble(),
        "heading" to location.bearing.toDouble(),
        "timestampMs" to location.time,
    )

    private companion object {
        const val PREFS = "MasamuneWhiskerLocation"
        const val ASKED = "asked"
    }
}
