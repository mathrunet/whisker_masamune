// Daily and one-off local notifications (AlarmManager) and notification taps. Schedules and channel settings are saved
// in SharedPreferences and re-armed after device reboots and app updates; one-off schedules are removed once they fire
// (or when they are re-armed after their time).
//
// A tap opens the app's launch activity with the schedule ID and payload as extras (FLAG_ACTIVITY_SINGLE_TOP, so a
// running activity receives it in onNewIntent). The launch intent is read by `getInitialTap`; later intents are sent as
// the `tap` event, queued while nobody observes it. Each intent is marked as consumed so it is delivered once, and
// intents relaunched from the recents screen are ignored.

package net.mathru.whisker.local_notification

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ActivityNotFoundException
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.IconCompat
import androidx.core.util.Consumer
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.RuntimeAttachedListener
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerValue
import java.io.File
import java.util.Calendar
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

internal object LocalNotifications {
    private const val SCHEDULES = "MasamuneWhiskerLocalNotification.schedules"
    private const val CONFIG = "MasamuneWhiskerLocalNotification.config"
    private const val ASSET_PREFIX = "file:///android_asset/"
    private const val DEFAULT_CHANNEL_ID = "default"

    /** Extras of the activity intent a tap opens. */
    const val EXTRA_ID = "net.mathru.whisker.local_notification.ID"
    const val EXTRA_PAYLOAD = "net.mathru.whisker.local_notification.PAYLOAD"
    const val EXTRA_CONSUMED = "net.mathru.whisker.local_notification.CONSUMED"

    private fun schedules(context: Context) = context.getSharedPreferences(SCHEDULES, Context.MODE_PRIVATE)

    private fun config(context: Context): JSONObject =
        context.getSharedPreferences(CONFIG, Context.MODE_PRIVATE).getString("config", null)?.let { JSONObject(it) } ?: JSONObject()

    fun configure(context: Context, json: JSONObject) {
        context.getSharedPreferences(CONFIG, Context.MODE_PRIVATE).edit().putString("config", json.toString()).apply()
        ensureChannel(context)
    }

    private fun channelId(context: Context): String = config(context).optString("channelId").ifBlank { DEFAULT_CHANNEL_ID }

    fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val config = config(context)
        val id = channelId(context)
        val manager = context.getSystemService(NotificationManager::class.java)
        if (manager.getNotificationChannel(id) == null) {
            val name = config.optString("channelName").ifBlank { context.applicationInfo.loadLabel(context.packageManager).toString() }
            val channel = NotificationChannel(id, name, NotificationManager.IMPORTANCE_DEFAULT)
            config.optString("channelDescription").takeIf { it.isNotBlank() }?.let { channel.description = it }
            manager.createNotificationChannel(channel)
        }
    }

    /** Whether notifications are allowed for both the app and the channel. */
    fun authorized(context: Context): Boolean {
        if (!NotificationManagerCompat.from(context).areNotificationsEnabled()) return false
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = context.getSystemService(NotificationManager::class.java).getNotificationChannel(channelId(context))
            if (channel != null && channel.importance == NotificationManager.IMPORTANCE_NONE) return false
        }
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return false
        }
        return true
    }

    fun requestCode(id: String): Int = id.hashCode()

    fun save(context: Context, id: String, schedule: JSONObject) {
        schedules(context).edit().putString(id, schedule.toString()).apply()
    }

    fun load(context: Context, id: String): JSONObject? = schedules(context).getString(id, null)?.let { JSONObject(it) }

    fun all(context: Context): Map<String, JSONObject> =
        schedules(context).all.mapNotNull { (key, value) -> (value as? String)?.let { key to JSONObject(it) } }.toMap()

    /** Removes a saved schedule without touching its alarm (it has fired or will be replaced). */
    fun forget(context: Context, id: String) {
        schedules(context).edit().remove(id).apply()
    }

    /** A string value, `null` when missing or JSON null. */
    fun string(json: JSONObject, key: String): String? = if (json.isNull(key)) null else json.optString(key)

    fun isOnce(schedule: JSONObject): Boolean = schedule.optBoolean("once", false)

    /** The time of a saved schedule: the next daily occurrence, or a one-off schedule's time. */
    fun fireTime(schedule: JSONObject): Long =
        if (isOnce(schedule)) schedule.optLong("atMs") else nextOccurrence(schedule.getInt("hour"), schedule.getInt("minute"))

    /** Re-arms every saved schedule, dropping one-off schedules whose time has passed. */
    fun rearmAll(context: Context) {
        all(context).keys.forEach { arm(context, it) }
    }

    fun remove(context: Context, id: String) {
        schedules(context).edit().remove(id).apply()
        cancelAlarm(context, id)
    }

    fun removeAll(context: Context) {
        all(context).keys.forEach { cancelAlarm(context, it) }
        schedules(context).edit().clear().apply()
    }

    private fun pendingIntent(context: Context, id: String): PendingIntent {
        val intent = Intent(context, DailyNotificationReceiver::class.java).putExtra("id", id)
        return PendingIntent.getBroadcast(
            context,
            requestCode(id),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun cancelAlarm(context: Context, id: String) {
        context.getSystemService(AlarmManager::class.java).cancel(pendingIntent(context, id))
    }

    /** The next occurrence of the time (tomorrow if today's time has already passed). */
    fun nextOccurrence(hour: Int, minute: Int): Long {
        val calendar = Calendar.getInstance()
        calendar.set(Calendar.HOUR_OF_DAY, hour)
        calendar.set(Calendar.MINUTE, minute)
        calendar.set(Calendar.SECOND, 0)
        calendar.set(Calendar.MILLISECOND, 0)
        if (calendar.timeInMillis <= System.currentTimeMillis()) {
            calendar.add(Calendar.DAY_OF_YEAR, 1)
        }
        return calendar.timeInMillis
    }

    /** Sets the next alarm from the saved schedule. A one-off schedule whose time has passed is removed instead. */
    fun arm(context: Context, id: String) {
        val schedule = load(context, id) ?: return
        val at = fireTime(schedule)
        if (isOnce(schedule) && at <= System.currentTimeMillis()) {
            forget(context, id)
            return
        }
        val alarm = context.getSystemService(AlarmManager::class.java)
        val pending = pendingIntent(context, id)
        val exact = Build.VERSION.SDK_INT < 31 || alarm.canScheduleExactAlarms()
        if (exact) {
            alarm.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pending)
        } else {
            alarm.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pending)
        }
    }

    /** The small icon: configure's smallIcon (a whisker-asset URI or file path), or the app icon if unset. */
    private fun smallIcon(context: Context): IconCompat {
        val path = config(context).optString("smallIcon")
        if (path.isNotBlank()) {
            try {
                val bitmap = if (path.startsWith(ASSET_PREFIX)) {
                    context.assets.open(path.removePrefix(ASSET_PREFIX)).use { BitmapFactory.decodeStream(it) }
                } else {
                    BitmapFactory.decodeFile(File(path.removePrefix("file://")).absolutePath)
                }
                if (bitmap != null) {
                    // Status bar icons are 24dp.
                    val size = (24 * context.resources.displayMetrics.density).toInt().coerceAtLeast(1)
                    return IconCompat.createWithBitmap(Bitmap.createScaledBitmap(bitmap, size, size, true))
                }
            } catch (_: Throwable) {
            }
        }
        return IconCompat.createWithResource(context, context.applicationInfo.icon)
    }

    fun show(context: Context, id: String) {
        val schedule = load(context, id) ?: return
        if (!authorized(context)) return
        ensureChannel(context)
        // The launch intent itself (MAIN / LAUNCHER) keeps the task's root intent equal to the launcher's, so opening the
        // app from the launcher later resumes the task instead of stacking a second activity.
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)?.apply {
            addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP)
            putExtra(EXTRA_ID, id)
            string(schedule, "payload")?.let { putExtra(EXTRA_PAYLOAD, it) }
        }
        val content = launch?.let {
            PendingIntent.getActivity(context, requestCode(id), it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        }
        val notification = NotificationCompat.Builder(context, channelId(context))
            .setSmallIcon(smallIcon(context))
            .setContentTitle(schedule.optString("title"))
            .setContentText(schedule.optString("text"))
            .setAutoCancel(true)
            .apply { if (content != null) setContentIntent(content) }
            .build()
        try {
            NotificationManagerCompat.from(context).notify(requestCode(id), notification)
        } catch (_: SecurityException) {
        }
    }
}

/** Posts the notification at the scheduled time, then re-arms a daily schedule for the next day or removes a one-off one. */
class DailyNotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getStringExtra("id") ?: return
        val schedule = LocalNotifications.load(context, id) ?: return
        LocalNotifications.show(context, id)
        if (LocalNotifications.isOnce(schedule)) {
            LocalNotifications.forget(context, id)
        } else {
            LocalNotifications.arm(context, id)
        }
    }
}

/** Re-arms schedules after a reboot, app update, or time change (dropping one-off schedules whose time has passed). */
class DailyNotificationBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        LocalNotifications.rearmAll(context)
    }
}

@WhiskerModule
class LocalNotificationModule : Module() {
    private val requestCounter = AtomicInteger(0)
    private val main = Handler(Looper.getMainLooper())

    /** Main thread only: whether `tap` has subscribers, and the taps that arrived while it had none. */
    private var observing = false
    private val queued = mutableListOf<JSONObject>()

    /** Activity whose onNewIntent is observed, and whether an activity has been attached before. */
    private var intentActivity: ComponentActivity? = null
    private var attachedBefore = false
    private val newIntentListener = Consumer<Intent> { intent -> claim(intent)?.let(::deliver) }
    private val attachListener = RuntimeAttachedListener { main.post { attachIntentListener() } }
    private val attachListenerAdded = AtomicBoolean(false)

    override fun definition() = ModuleDefinition {
        Name("LocalNotification")
        Events("tap")

        OnStartObserving("tap") {
            // The onNewIntent listener stays registered after the last subscriber leaves, so taps keep being queued.
            if (attachListenerAdded.compareAndSet(false, true)) appContext.addOnRuntimeAttachedListener(attachListener)
            main.post {
                observing = true
                val pending = queued.toList()
                queued.clear()
                pending.forEach(::deliver)
            }
        }
        OnStopObserving("tap") {
            main.post { observing = false }
        }

        // getInitialTap() -> { id, payload } | null — waits for the host activity, then consumes its launch intent.
        AsyncFunction("getInitialTap") { _, promise ->
            var listener: RuntimeAttachedListener? = null
            val pending = RuntimeAttachedListener {
                main.post {
                    val current = listener ?: return@post
                    listener = null
                    appContext.removeOnRuntimeAttachedListener(current)
                    val intent = appContext.currentActivity?.intent
                    Bridge.resolve(promise, intent?.let { claim(it) })
                }
            }
            listener = pending
            appContext.addOnRuntimeAttachedListener(pending)
        }

        // configure({ channelId, channelName, channelDescription, smallIcon }) — notification channel and small icon.
        Function("configure") { args ->
            try {
                val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
                LocalNotifications.configure(context, Bridge.args(args))
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // status() -> "authorized" | "denied"
        Function("status") { _ ->
            val context = WhiskerApplication.appContext
            Bridge.ok(if (context != null && LocalNotifications.authorized(context)) "authorized" else "denied")
        }

        // requestPermission() -> whether permission was granted
        AsyncFunction("requestPermission") { _, promise ->
            val activity = appContext.currentActivity
            val context = WhiskerApplication.appContext
            if (activity == null || context == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            LocalNotifications.ensureChannel(context)
            if (Build.VERSION.SDK_INT < 33 ||
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
            ) {
                Bridge.resolve(promise, LocalNotifications.authorized(context))
                return@AsyncFunction
            }
            val component = activity as? ComponentActivity
            if (component == null) {
                Bridge.resolve(promise, false)
                return@AsyncFunction
            }
            Bridge.onMain {
                val key = "masamune_whisker_notification_permission_${requestCounter.incrementAndGet()}"
                var launcher: androidx.activity.result.ActivityResultLauncher<String>? = null
                launcher = component.activityResultRegistry.register(key, ActivityResultContracts.RequestPermission()) {
                    launcher?.unregister()
                    Bridge.resolve(promise, LocalNotifications.authorized(context))
                }
                launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
        }

        // scheduleDaily({ id, hour, minute, title, text, payload }) -> next fire time (ms)
        Function("scheduleDaily") { args ->
            try {
                val json = Bridge.args(args)
                val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
                val id = json.getString("id")
                json.put("once", false)
                json.remove("atMs")
                LocalNotifications.ensureChannel(context)
                LocalNotifications.save(context, id, json)
                LocalNotifications.arm(context, id)
                Bridge.ok(LocalNotifications.nextOccurrence(json.getInt("hour"), json.getInt("minute")))
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // scheduleOnce({ id, atMs, title, text, payload }) -> fire time (ms). Fails for past times.
        Function("scheduleOnce") { args ->
            try {
                val json = Bridge.args(args)
                val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
                val id = json.getString("id")
                val at = json.getLong("atMs")
                if (at <= System.currentTimeMillis()) throw IllegalArgumentException("at_ms is in the past")
                val calendar = Calendar.getInstance().apply { timeInMillis = at }
                json.put("once", true)
                json.put("hour", calendar.get(Calendar.HOUR_OF_DAY))
                json.put("minute", calendar.get(Calendar.MINUTE))
                LocalNotifications.ensureChannel(context)
                LocalNotifications.save(context, id, json)
                LocalNotifications.arm(context, id)
                Bridge.ok(at)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // schedule({ id }) -> saved schedule (null if missing or a one-off that has fired)
        Function("schedule") { args ->
            val context = WhiskerApplication.appContext ?: return@Function Bridge.ok(null)
            val id = Bridge.args(args).optString("id")
            val schedule = LocalNotifications.load(context, id)
            val expired = schedule != null && LocalNotifications.isOnce(schedule) &&
                LocalNotifications.fireTime(schedule) <= System.currentTimeMillis()
            if (expired) LocalNotifications.forget(context, id)
            Bridge.ok(if (expired) null else schedule)
        }

        // rearm() — re-arms all saved schedules (at startup), dropping one-off schedules whose time has passed.
        Function("rearm") { _ ->
            val context = WhiskerApplication.appContext
            if (context != null) LocalNotifications.rearmAll(context)
            Bridge.ok(true)
        }

        Function("cancel") { args ->
            val context = WhiskerApplication.appContext
            if (context != null) LocalNotifications.remove(context, Bridge.args(args).optString("id"))
            Bridge.ok(true)
        }

        Function("cancelAll") { _ ->
            val context = WhiskerApplication.appContext
            if (context != null) LocalNotifications.removeAll(context)
            Bridge.ok(true)
        }

        // openSettings() — opens this app's notification settings screen.
        Function("openSettings") { _ ->
            try {
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                val intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                    .putExtra(Settings.EXTRA_APP_PACKAGE, activity.packageName)
                try {
                    activity.startActivity(intent)
                } catch (_: ActivityNotFoundException) {
                    activity.startActivity(
                        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}")),
                    )
                }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }
    }

    /** Observes onNewIntent of the attached activity. A later activity (re-created while subscribed) also has its launch
     *  intent delivered; the first one's is left to `getInitialTap`. */
    private fun attachIntentListener() {
        val activity = appContext.currentActivity as? ComponentActivity ?: return
        if (activity === intentActivity) return
        intentActivity?.removeOnNewIntentListener(newIntentListener)
        activity.addOnNewIntentListener(newIntentListener)
        intentActivity = activity
        if (attachedBefore) claim(activity.intent)?.let(::deliver)
        attachedBefore = true
    }

    /** Sends a tap, or queues it while nobody observes `tap`. Main thread. */
    private fun deliver(tap: JSONObject) {
        if (!observing) {
            queued.add(tap)
            return
        }
        sendEvent("tap", WhiskerValue.Str(tap.toString()))
    }

    /** Marks a tap intent as consumed and returns `{ id, payload }`. Null when it is not one, or was handled. */
    private fun claim(intent: Intent?): JSONObject? {
        if (intent == null) return null
        if (intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return null
        if (intent.getBooleanExtra(LocalNotifications.EXTRA_CONSUMED, false)) return null
        val id = intent.getStringExtra(LocalNotifications.EXTRA_ID)?.takeIf { it.isNotEmpty() } ?: return null
        intent.putExtra(LocalNotifications.EXTRA_CONSUMED, true)
        return JSONObject()
            .put("id", id)
            .put("payload", intent.getStringExtra(LocalNotifications.EXTRA_PAYLOAD) ?: JSONObject.NULL)
    }
}
