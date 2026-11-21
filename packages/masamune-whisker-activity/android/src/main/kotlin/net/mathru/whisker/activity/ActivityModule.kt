// Motion activity recognition (Android, Activity Recognition Transition API).
//
// `startUpdates` registers ENTER transitions for IN_VEHICLE, ON_BICYCLE, RUNNING, WALKING and
// STILL with a PendingIntent to ActivityTransitionReceiver (non-exported; Google Play services
// sends the PendingIntent with this app's identity). The receiver forwards the transitions to the
// module, which sends `update` events while Rust subscribes.

package net.mathru.whisker.activity

import android.Manifest
import android.app.PendingIntent
import android.content.ActivityNotFoundException
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.SystemClock
import android.provider.Settings
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.android.gms.location.ActivityRecognition
import com.google.android.gms.location.ActivityTransition
import com.google.android.gms.location.ActivityTransitionRequest
import com.google.android.gms.location.ActivityTransitionResult
import com.google.android.gms.location.DetectedActivity
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import java.lang.ref.WeakReference
import java.util.concurrent.atomic.AtomicInteger

@WhiskerModule
class ActivityModule : Module() {
    private val requestCounter = AtomicInteger(0)
    private var running = false

    init {
        current = WeakReference(this)
    }

    override fun definition() = ModuleDefinition {
        Name("Activity")
        Events("update")

        Function("status") { _ ->
            Bridge.ok(WhiskerApplication.appContext?.let { status(it) } ?: "unsupported")
        }

        AsyncFunction("requestPermission") { _, promise ->
            val context = WhiskerApplication.appContext
            val activity = appContext.currentActivity as? ComponentActivity
            when {
                context == null -> Bridge.resolve(promise, "unsupported")
                Build.VERSION.SDK_INT < 29 || status(context) != "notDetermined" && status(context) != "denied" ->
                    Bridge.resolve(promise, status(context))
                activity == null -> Bridge.reject(promise, "no activity")
                else -> Bridge.onMain {
                    prefs(context).edit().putBoolean(ASKED, true).apply()
                    val key = "masamune_whisker_activity_permission_${requestCounter.incrementAndGet()}"
                    var launcher: ActivityResultLauncher<String>? = null
                    launcher = activity.activityResultRegistry.register(key, ActivityResultContracts.RequestPermission()) {
                        launcher?.unregister()
                        Bridge.resolve(promise, status(context))
                    }
                    launcher.launch(Manifest.permission.ACTIVITY_RECOGNITION)
                }
            }
        }

        Function("openAppSettings") { _ ->
            try {
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                try {
                    activity.startActivity(
                        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}")),
                    )
                } catch (_: ActivityNotFoundException) {
                    activity.startActivity(Intent(Settings.ACTION_SETTINGS))
                }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        Function("startUpdates") { _ ->
            Bridge.onMain { start() }
            Bridge.ok(true)
        }

        Function("stopUpdates") { _ ->
            Bridge.onMain { stop() }
            Bridge.ok(true)
        }
    }

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private fun status(context: Context): String {
        val available = GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(context) == ConnectionResult.SUCCESS
        if (!available) return "unsupported"
        if (Build.VERSION.SDK_INT < 29) return "granted"
        return when {
            granted(context) -> "granted"
            !prefs(context).getBoolean(ASKED, false) -> "notDetermined"
            else -> "denied"
        }
    }

    private fun granted(context: Context) = Build.VERSION.SDK_INT < 29 ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.ACTIVITY_RECOGNITION) == PackageManager.PERMISSION_GRANTED

    private fun pendingIntent(context: Context): PendingIntent {
        val intent = Intent(context, ActivityTransitionReceiver::class.java).setAction(ACTION)
        // Google Play services fills in the result extras, so the intent must be mutable.
        val flags = PendingIntent.FLAG_UPDATE_CURRENT or
            if (Build.VERSION.SDK_INT >= 31) PendingIntent.FLAG_MUTABLE else 0
        return PendingIntent.getBroadcast(context, 0, intent, flags)
    }

    @Suppress("MissingPermission")
    private fun start() {
        val context = WhiskerApplication.appContext ?: return
        when (status(context)) {
            "granted" -> Unit
            "unsupported" -> return error("Google Play services are not available")
            else -> return error("activity recognition permission denied")
        }
        val transitions = KINDS.keys.map { type ->
            ActivityTransition.Builder()
                .setActivityType(type)
                .setActivityTransition(ActivityTransition.ACTIVITY_TRANSITION_ENTER)
                .build()
        }
        try {
            ActivityRecognition.getClient(context)
                .requestActivityTransitionUpdates(ActivityTransitionRequest(transitions), pendingIntent(context))
                .addOnSuccessListener { running = true }
                .addOnFailureListener { failure ->
                    Log.w(TAG, "transition updates failed: ${failure.javaClass.simpleName}")
                    error(failure.message ?: "activity recognition failed")
                }
        } catch (failure: SecurityException) {
            error("activity recognition permission denied")
        }
    }

    @Suppress("MissingPermission")
    private fun stop() {
        val context = WhiskerApplication.appContext ?: return
        if (!running) return
        running = false
        try {
            ActivityRecognition.getClient(context).removeActivityTransitionUpdates(pendingIntent(context))
        } catch (_: SecurityException) {
        }
    }

    private fun error(message: String) {
        sendEvent("update", Bridge.event(mapOf("type" to "error", "message" to message)))
    }

    internal fun deliver(result: ActivityTransitionResult) {
        if (!running) return
        // Elapsed-realtime nanoseconds to wall clock milliseconds.
        val offset = System.currentTimeMillis() - SystemClock.elapsedRealtime()
        for (event in result.transitionEvents) {
            if (event.transitionType != ActivityTransition.ACTIVITY_TRANSITION_ENTER) continue
            val kind = KINDS[event.activityType] ?: "unknown"
            sendEvent(
                "update",
                Bridge.event(
                    mapOf(
                        "type" to "activity",
                        "kind" to kind,
                        "confidence" to 1.0,
                        "timestampMs" to offset + event.elapsedRealTimeNanos / 1_000_000,
                    ),
                ),
            )
        }
    }

    companion object {
        private const val TAG = "MasamuneActivity"
        private const val PREFS = "MasamuneWhiskerActivity"
        private const val ASKED = "asked"
        internal const val ACTION = "net.mathru.whisker.activity.TRANSITION"

        private val KINDS = mapOf(
            DetectedActivity.IN_VEHICLE to "automotive",
            DetectedActivity.ON_BICYCLE to "cycling",
            DetectedActivity.RUNNING to "running",
            DetectedActivity.WALKING to "walking",
            DetectedActivity.STILL to "stationary",
        )

        internal var current: WeakReference<ActivityModule>? = null
    }
}

/** Receives the transition results and forwards them to the module on the main thread. */
class ActivityTransitionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ActivityModule.ACTION || !ActivityTransitionResult.hasResult(intent)) return
        val result = ActivityTransitionResult.extractResult(intent) ?: return
        Bridge.onMain { ActivityModule.current?.get()?.deliver(result) }
    }
}
