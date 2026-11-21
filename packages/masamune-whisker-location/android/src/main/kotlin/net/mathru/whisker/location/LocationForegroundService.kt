// Foreground service (type location) that keeps location updates running in the background.
//
// The notification is either one the app already shows (`reuseNotificationId`, for example the
// media notification of masamune-whisker-audio-session), which is left in place when the service
// stops, or the service's own notification built from `channelId` / `title` / `text`.

package net.mathru.whisker.location

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject

class LocationForegroundService : Service() {
    private var reused = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val config = settings ?: JSONObject()
        val reuseId = if (config.isNull("reuseNotificationId")) null else config.optInt("reuseNotificationId")
        val existing = reuseId?.let { id ->
            getSystemService(NotificationManager::class.java)?.activeNotifications?.firstOrNull { it.id == id }
        }
        try {
            if (existing != null) {
                reused = true
                startInForeground(existing.id, existing.notification)
            } else {
                reused = false
                startInForeground(OWN_NOTIFICATION_ID, ownNotification(config))
            }
        } catch (error: RuntimeException) {
            // ForegroundServiceStartNotAllowedException / SecurityException (missing permission).
            Log.w(TAG, "cannot start the location service: ${error.javaClass.simpleName}")
            onFailure?.invoke("the location service cannot start: ${error.javaClass.simpleName}")
            stopSelf()
        }
        return START_NOT_STICKY
    }

    private fun startInForeground(id: Int, notification: Notification) {
        ServiceCompat.startForeground(this, id, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_LOCATION)
    }

    private fun ownNotification(config: JSONObject): Notification {
        val channelId = config.optString("channelId").ifEmpty { DEFAULT_CHANNEL_ID }
        if (Build.VERSION.SDK_INT >= 26) {
            getSystemService(NotificationManager::class.java)?.createNotificationChannel(
                NotificationChannel(
                    channelId,
                    config.optString("channelName").ifEmpty { "Location" },
                    NotificationManager.IMPORTANCE_LOW,
                ).apply { setShowBadge(false) },
            )
        }
        val builder = if (Build.VERSION.SDK_INT >= 26) {
            Notification.Builder(this, channelId)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
        val launch = packageManager.getLaunchIntentForPackage(packageName)?.let {
            PendingIntent.getActivity(this, OWN_NOTIFICATION_ID, it, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        }
        return builder
            .setSmallIcon(smallIcon(config.optString("smallIcon")))
            .setContentTitle(config.optString("title"))
            .setContentText(config.optString("text"))
            .setContentIntent(launch)
            .setOngoing(true)
            .setShowWhen(false)
            .build()
    }

    private fun smallIcon(name: String): Int {
        if (name.isEmpty()) return applicationInfo.icon
        val (type, entry) = if (name.contains('/')) name.substringBefore('/') to name.substringAfter('/') else "drawable" to name
        return resources.getIdentifier(entry, type, packageName).takeIf { it != 0 } ?: applicationInfo.icon
    }

    override fun onDestroy() {
        // A reused notification belongs to another service: detach instead of removing it.
        ServiceCompat.stopForeground(this, if (reused) ServiceCompat.STOP_FOREGROUND_DETACH else ServiceCompat.STOP_FOREGROUND_REMOVE)
        super.onDestroy()
    }

    companion object {
        private const val TAG = "MasamuneLocation"
        private const val DEFAULT_CHANNEL_ID = "masamune_whisker_location"
        const val OWN_NOTIFICATION_ID = 0x4d4c4f43 // "MLOC"

        private var settings: JSONObject? = null
        private var onFailure: ((String) -> Unit)? = null

        /** Starts (or updates) the service. Returns false when Android refuses to start it. */
        fun start(context: Context, notification: JSONObject, failure: (String) -> Unit): Boolean {
            settings = notification
            onFailure = failure
            return try {
                ContextCompat.startForegroundService(context, Intent(context, LocationForegroundService::class.java))
                true
            } catch (error: IllegalStateException) {
                Log.w(TAG, "cannot start the location service: ${error.javaClass.simpleName}")
                false
            }
        }

        fun stop(context: Context) {
            onFailure = null
            context.stopService(Intent(context, LocationForegroundService::class.java))
        }
    }
}
