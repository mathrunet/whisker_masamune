// 毎日決まった時刻のローカル通知（AlarmManager）。予約内容とチャンネル設定は SharedPreferences に保存し、
// 端末の再起動・アプリの更新後に入れ直す。

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
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.IconCompat
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import java.io.File
import java.util.Calendar
import java.util.concurrent.atomic.AtomicInteger

internal object LocalNotifications {
    private const val SCHEDULES = "MasamuneWhiskerLocalNotification.schedules"
    private const val CONFIG = "MasamuneWhiskerLocalNotification.config"
    private const val ASSET_PREFIX = "file:///android_asset/"
    private const val DEFAULT_CHANNEL_ID = "default"

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

    /** アプリ全体とチャンネルの両方で通知が許可されているか。 */
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

    /** 次にその時刻になる日時（今日の時刻が過ぎていれば明日）。 */
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

    /** 保存済みの予約から次のアラームを入れる。 */
    fun arm(context: Context, id: String) {
        val schedule = load(context, id) ?: return
        val at = nextOccurrence(schedule.getInt("hour"), schedule.getInt("minute"))
        val alarm = context.getSystemService(AlarmManager::class.java)
        val pending = pendingIntent(context, id)
        val exact = Build.VERSION.SDK_INT < 31 || alarm.canScheduleExactAlarms()
        if (exact) {
            alarm.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pending)
        } else {
            alarm.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pending)
        }
    }

    /** 小さいアイコン。configure の smallIcon（whisker-asset の URI かファイルパス）、無ければアプリのアイコン。 */
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
                    // ステータスバーのアイコンは 24dp。
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
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName)
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

/** 予約時刻に通知を出し、翌日の分を入れ直す。 */
class DailyNotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val id = intent.getStringExtra("id") ?: return
        LocalNotifications.show(context, id)
        LocalNotifications.arm(context, id)
    }
}

/** 再起動・アプリ更新・時刻変更のあとに予約を入れ直す。 */
class DailyNotificationBootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        LocalNotifications.all(context).keys.forEach { LocalNotifications.arm(context, it) }
    }
}

@WhiskerModule
class LocalNotificationModule : Module() {
    private val requestCounter = AtomicInteger(0)

    override fun definition() = ModuleDefinition {
        Name("LocalNotification")

        // configure({ channelId, channelName, channelDescription, smallIcon }) — 通知チャンネルと小さいアイコン。
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

        // requestPermission() -> 許可されたか
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

        // scheduleDaily({ id, hour, minute, title, text }) -> 次の通知の時刻（ms）
        Function("scheduleDaily") { args ->
            try {
                val json = Bridge.args(args)
                val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
                val id = json.getString("id")
                LocalNotifications.ensureChannel(context)
                LocalNotifications.save(context, id, json)
                LocalNotifications.arm(context, id)
                Bridge.ok(LocalNotifications.nextOccurrence(json.getInt("hour"), json.getInt("minute")))
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // schedule({ id }) -> 保存済みの予約（無ければ null）
        Function("schedule") { args ->
            val context = WhiskerApplication.appContext
            val id = Bridge.args(args).optString("id")
            Bridge.ok(context?.let { LocalNotifications.load(it, id) })
        }

        // rearm() — 保存済みの予約を全て入れ直す（起動時）。
        Function("rearm") { _ ->
            val context = WhiskerApplication.appContext
            if (context != null) {
                LocalNotifications.all(context).keys.forEach { LocalNotifications.arm(context, it) }
            }
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

        // openSettings() — このアプリの通知設定画面を開く。
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
}
