// Process-wide audio session state (Android): audio focus, becoming-noisy and device changes, the
// framework MediaSession, the media notification and the foreground service.
//
// Everything runs on the main thread. The module, the service and the notification action
// receiver all go through this object, so the state survives the service being recreated.

package net.mathru.whisker.audio_session

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.AudioAttributes
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.util.Log
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import java.lang.ref.WeakReference

internal object AudioSessions {
    private const val TAG = "MasamuneAudioSession"
    const val NOTIFICATION_ID = 0x4d415553 // "MAUS"
    const val ACTION_PREFIX = "net.mathru.whisker.audio_session.action."
    const val EXTRA_CUSTOM_ID = "net.mathru.whisker.audio_session.CUSTOM_ID"
    const val EXTRA_NOTIFICATION_TAP = "net.mathru.whisker.audio_session.NOTIFICATION_TAP"
    private const val DEFAULT_CHANNEL_ID = "masamune_whisker_audio_session"
    private const val ARTWORK_MAX_SIDE = 512
    private const val WAKE_LOCK_TIMEOUT_MS = 60 * 60 * 1000L

    private val main = Handler(Looper.getMainLooper())
    private var moduleRef: WeakReference<AudioSessionModule>? = null

    private var config = JSONObject()
    private var configured = false
    private var session: MediaSession? = null
    private var item: JSONObject? = null
    private var artworkPath: String? = null
    private var artwork: Bitmap? = null
    private var controls = JSONObject()
    private var playing = false
    private var state = "idle"
    private var positionMs = 0L
    private var speed = 1f
    private var wakeLock: PowerManager.WakeLock? = null

    private var focusRequest: AudioFocusRequest? = null
    private var hasFocus = false

    /** Kind of the transient focus loss in progress (`pause` or `duck`), reported again on regain. */
    private var transientLoss: String? = null

    var service: AudioSessionService? = null

    fun attach(module: AudioSessionModule) {
        moduleRef = WeakReference(module)
    }

    private fun emit(event: String, payload: Map<String, Any?>) {
        main.post { moduleRef?.get()?.emit(event, payload) }
    }

    // MARK: - Configuration

    fun configure(context: Context, json: JSONObject) {
        config = json
        ensureChannel(context)
        if (configured) return
        configured = true
        val app = context.applicationContext
        ContextCompat.registerReceiver(
            app,
            object : BroadcastReceiver() {
                override fun onReceive(context: Context, intent: Intent) {
                    if (intent.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) {
                        emit("routeChange", mapOf("reason" to "oldDeviceUnavailable", "outputs" to outputs(app)))
                    }
                }
            },
            IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY),
            ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        audioManager(app).registerAudioDeviceCallback(
            object : AudioDeviceCallback() {
                // The callback also reports the devices present at registration; skip that first call.
                private var initial = true

                override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>) {
                    if (initial) {
                        initial = false
                        return
                    }
                    if (addedDevices.any { it.isSink }) {
                        emit("routeChange", mapOf("reason" to "newDeviceAvailable", "outputs" to outputs(app)))
                    }
                }

                override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>) {
                    if (removedDevices.any { it.isSink }) {
                        // ACTION_AUDIO_BECOMING_NOISY reports the removal of the output in use.
                        emit("routeChange", mapOf("reason" to "other", "outputs" to outputs(app)))
                    }
                }
            },
            main,
        )
    }

    private fun channelId(): String = config.optString("channelId").ifEmpty { DEFAULT_CHANNEL_ID }

    private fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < 26) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val channel = NotificationChannel(
            channelId(),
            config.optString("channelName").ifEmpty { "Playback" },
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = config.optString("channelDescription")
            setShowBadge(false)
            setSound(null, null)
            enableVibration(false)
        }
        manager.createNotificationChannel(channel)
    }

    private fun audioManager(context: Context): AudioManager =
        context.getSystemService(Context.AUDIO_SERVICE) as AudioManager

    private fun outputs(context: Context): List<String> =
        audioManager(context).getDevices(AudioManager.GET_DEVICES_OUTPUTS).map { deviceType(it.type) }.distinct()

    private fun deviceType(type: Int): String = when (type) {
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "builtinSpeaker"
        AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> "builtinEarpiece"
        AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> "wiredHeadphones"
        AudioDeviceInfo.TYPE_WIRED_HEADSET -> "wiredHeadset"
        AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> "bluetoothA2dp"
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "bluetoothSco"
        AudioDeviceInfo.TYPE_USB_DEVICE, AudioDeviceInfo.TYPE_USB_HEADSET -> "usb"
        AudioDeviceInfo.TYPE_HDMI -> "hdmi"
        else -> "other"
    }

    private fun attributes(): AudioAttributes = AudioAttributes.Builder()
        .setUsage(AudioAttributes.USAGE_MEDIA)
        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
        .build()

    // MARK: - Audio focus

    private val focusListener = AudioManager.OnAudioFocusChangeListener { change ->
        when (change) {
            AudioManager.AUDIOFOCUS_LOSS -> {
                hasFocus = false
                transientLoss = null
                emit("interruption", mapOf("begin" to true, "kind" to "unknown", "shouldResume" to false))
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> {
                transientLoss = "pause"
                emit("interruption", mapOf("begin" to true, "kind" to "pause", "shouldResume" to false))
            }
            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> {
                transientLoss = "duck"
                emit("interruption", mapOf("begin" to true, "kind" to "duck", "shouldResume" to false))
            }
            AudioManager.AUDIOFOCUS_GAIN -> {
                hasFocus = true
                val kind = transientLoss ?: return@OnAudioFocusChangeListener
                transientLoss = null
                emit("interruption", mapOf("begin" to false, "kind" to kind, "shouldResume" to true))
            }
        }
    }

    /** Requests (or abandons) audio focus. Returns whether the app may play. */
    fun setActive(context: Context, active: Boolean): Boolean {
        if (config.optBoolean("mixWithOthers")) return active
        val manager = audioManager(context)
        if (!active) {
            if (Build.VERSION.SDK_INT >= 26) {
                focusRequest?.let { manager.abandonAudioFocusRequest(it) }
            } else {
                @Suppress("DEPRECATION")
                manager.abandonAudioFocus(focusListener)
            }
            hasFocus = false
            transientLoss = null
            return false
        }
        if (hasFocus) return true
        val gain = if (config.optBoolean("duckOthers")) {
            AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK
        } else {
            AudioManager.AUDIOFOCUS_GAIN
        }
        val result = if (Build.VERSION.SDK_INT >= 26) {
            val request = AudioFocusRequest.Builder(gain)
                .setAudioAttributes(attributes())
                .setOnAudioFocusChangeListener(focusListener, main)
                .setWillPauseWhenDucked(false)
                .build()
            focusRequest = request
            manager.requestAudioFocus(request)
        } else {
            @Suppress("DEPRECATION")
            manager.requestAudioFocus(focusListener, AudioManager.STREAM_MUSIC, gain)
        }
        hasFocus = result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        if (hasFocus) transientLoss = null
        return hasFocus
    }

    // MARK: - Media session

    private fun session(context: Context): MediaSession {
        session?.let { return it }
        val created = MediaSession(context.applicationContext, TAG)
        created.setCallback(
            object : MediaSession.Callback() {
                override fun onPlay() = dispatch(mapOf("command" to "play"))
                override fun onPause() = dispatch(mapOf("command" to "pause"))
                override fun onStop() = dispatch(mapOf("command" to "stop"))
                override fun onSkipToNext() = dispatch(mapOf("command" to "next"))
                override fun onSkipToPrevious() = dispatch(mapOf("command" to "previous"))
                override fun onSeekTo(pos: Long) = dispatch(mapOf("command" to "seek", "positionMs" to pos.coerceAtLeast(0)))
                override fun onCustomAction(action: String, extras: android.os.Bundle?) =
                    dispatch(mapOf("command" to "custom", "id" to action))
            },
            main,
        )
        created.setSessionActivity(contentIntent(context))
        session = created
        return created
    }

    fun dispatch(command: Map<String, Any?>) {
        emit("command", command)
    }

    fun setNowPlaying(context: Context, json: JSONObject) {
        val app = context.applicationContext
        item = json
        val path = json.optString("artworkPath").takeIf { it.isNotEmpty() && !json.isNull("artworkPath") }
        if (path != artworkPath) {
            artworkPath = path
            artwork = path?.let { decodeArtwork(it) }
        }
        val session = session(app)
        val metadata = MediaMetadata.Builder()
            .putString(MediaMetadata.METADATA_KEY_MEDIA_ID, json.optString("id"))
            .putString(MediaMetadata.METADATA_KEY_TITLE, json.optString("title"))
            .putString(MediaMetadata.METADATA_KEY_ARTIST, json.optString("artist"))
            .putString(MediaMetadata.METADATA_KEY_ALBUM, json.optString("album"))
        if (!json.isNull("durationMs") && json.has("durationMs")) {
            metadata.putLong(MediaMetadata.METADATA_KEY_DURATION, json.optLong("durationMs"))
        }
        artwork?.let { metadata.putBitmap(MediaMetadata.METADATA_KEY_ART, it) }
        session.setMetadata(metadata.build())
        session.isActive = true
        publishState(app)
        startService(app)
        refresh(app)
    }

    fun clear(context: Context) {
        val app = context.applicationContext
        item = null
        artwork = null
        artworkPath = null
        playing = false
        state = "idle"
        session?.let {
            it.isActive = false
            it.release()
        }
        session = null
        releaseWakeLock()
        app.stopService(Intent(app, AudioSessionService::class.java))
        val manager = app.getSystemService(NotificationManager::class.java)
        manager?.cancel(NOTIFICATION_ID)
    }

    fun setPlaybackState(context: Context, json: JSONObject) {
        playing = json.optBoolean("playing")
        state = json.optString("state", "idle")
        positionMs = json.optLong("positionMs").coerceAtLeast(0)
        speed = json.optDouble("speed", 1.0).toFloat()
        val app = context.applicationContext
        if (session != null) publishState(app)
        updateWakeLock(app)
        refresh(app)
    }

    fun setControls(context: Context, json: JSONObject) {
        controls = json
        val app = context.applicationContext
        if (session != null) publishState(app)
        refresh(app)
    }

    private fun publishState(context: Context) {
        val session = session ?: return
        var actions = PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or
            PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_STOP
        if (controls.optBoolean("previous", true)) actions = actions or PlaybackState.ACTION_SKIP_TO_PREVIOUS
        if (controls.optBoolean("next", true)) actions = actions or PlaybackState.ACTION_SKIP_TO_NEXT
        if (controls.optBoolean("seek", true)) actions = actions or PlaybackState.ACTION_SEEK_TO
        val code = when {
            state == "error" -> PlaybackState.STATE_ERROR
            state == "loading" || state == "buffering" -> PlaybackState.STATE_BUFFERING
            playing -> PlaybackState.STATE_PLAYING
            state == "idle" -> PlaybackState.STATE_STOPPED
            else -> PlaybackState.STATE_PAUSED
        }
        val builder = PlaybackState.Builder()
            .setActions(actions)
            .setState(code, positionMs, if (playing) speed else 0f, SystemClock.elapsedRealtime())
        for (action in customActions()) {
            builder.addCustomAction(
                PlaybackState.CustomAction.Builder(
                    action.optString("id"),
                    action.optString("label"),
                    customIcon(context, action),
                ).build(),
            )
        }
        session.setPlaybackState(builder.build())
    }

    private fun customActions(): List<JSONObject> {
        val list = controls.optJSONArray("custom") ?: JSONArray()
        return (0 until list.length()).mapNotNull { list.optJSONObject(it) }.filter { it.optString("id").isNotEmpty() }
    }

    // MARK: - Notification and service

    private fun startService(context: Context) {
        if (service != null) return
        try {
            ContextCompat.startForegroundService(context, Intent(context, AudioSessionService::class.java))
        } catch (error: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException (Android 12+) when the app is in the background.
            Log.w(TAG, "cannot start the playback service: ${error.javaClass.simpleName}")
            throw IllegalStateException("the playback service cannot start while the app is in the background", error)
        }
    }

    private fun refresh(context: Context) {
        if (service == null || item == null) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        try {
            manager.notify(NOTIFICATION_ID, notification(context))
        } catch (error: SecurityException) {
            Log.w(TAG, "cannot post the media notification")
        }
    }

    fun notification(context: Context): Notification {
        val builder = if (Build.VERSION.SDK_INT >= 26) {
            Notification.Builder(context, channelId())
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(context)
        }
        val item = item ?: JSONObject()
        val compact = mutableListOf<Int>()
        var index = 0
        if (controls.optBoolean("previous", true)) {
            builder.addAction(action(context, android.R.drawable.ic_media_previous, "Previous", "previous"))
            compact += index++
        }
        builder.addAction(
            if (playing) {
                action(context, android.R.drawable.ic_media_pause, "Pause", "pause")
            } else {
                action(context, android.R.drawable.ic_media_play, "Play", "play")
            },
        )
        compact += index++
        if (controls.optBoolean("next", true)) {
            builder.addAction(action(context, android.R.drawable.ic_media_next, "Next", "next"))
            compact += index++
        }
        for (custom in customActions()) {
            builder.addAction(
                action(context, customIcon(context, custom), custom.optString("label"), "custom", custom.optString("id")),
            )
        }
        val style = Notification.MediaStyle().setShowActionsInCompactView(*compact.take(3).toIntArray())
        session?.let { style.setMediaSession(it.sessionToken) }
        builder
            .setSmallIcon(smallIcon(context))
            .setContentTitle(item.optString("title"))
            .setContentText(item.optString("artist"))
            .setSubText(item.optString("album").ifEmpty { null })
            .setLargeIcon(artwork)
            .setContentIntent(contentIntent(context))
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setOngoing(true)
            .setStyle(style)
        if (Build.VERSION.SDK_INT >= 31) {
            builder.setForegroundServiceBehavior(Notification.FOREGROUND_SERVICE_IMMEDIATE)
        }
        return builder.build()
    }

    private fun action(context: Context, icon: Int, title: String, command: String, id: String? = null): Notification.Action {
        val intent = Intent(context, AudioSessionActionReceiver::class.java)
            .setAction(ACTION_PREFIX + command + (id?.let { ":$it" } ?: ""))
        if (id != null) intent.putExtra(EXTRA_CUSTOM_ID, id)
        val pending = PendingIntent.getBroadcast(
            context,
            intent.action.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        @Suppress("DEPRECATION")
        return Notification.Action.Builder(icon, title, pending).build()
    }

    private fun contentIntent(context: Context): PendingIntent? {
        val launch = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
        launch.putExtra(EXTRA_NOTIFICATION_TAP, true)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_REORDER_TO_FRONT)
        return PendingIntent.getActivity(
            context,
            NOTIFICATION_ID,
            launch,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    /** `"mipmap/name"`, `"drawable/name"` or `"name"` (drawable, then mipmap). */
    private fun resource(context: Context, name: String): Int {
        if (name.isEmpty()) return 0
        val resources = context.resources
        val (type, entry) = if (name.contains('/')) name.substringBefore('/') to name.substringAfter('/') else null to name
        if (type != null) return resources.getIdentifier(entry, type, context.packageName)
        val drawable = resources.getIdentifier(entry, "drawable", context.packageName)
        return if (drawable != 0) drawable else resources.getIdentifier(entry, "mipmap", context.packageName)
    }

    private fun smallIcon(context: Context): Int =
        resource(context, config.optString("smallIcon")).takeIf { it != 0 } ?: context.applicationInfo.icon

    private fun customIcon(context: Context, action: JSONObject): Int =
        resource(context, action.optString("androidIcon")).takeIf { it != 0 }
            ?: if (action.optBoolean("active")) android.R.drawable.star_on else android.R.drawable.star_off

    private fun decodeArtwork(path: String): Bitmap? = try {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(path, bounds)
        var sample = 1
        while (bounds.outWidth / (sample * 2) >= ARTWORK_MAX_SIDE && bounds.outHeight / (sample * 2) >= ARTWORK_MAX_SIDE) {
            sample *= 2
        }
        BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = sample })
    } catch (error: Throwable) {
        Log.w(TAG, "cannot decode the artwork: ${error.javaClass.simpleName}")
        null
    }

    // MARK: - Wake lock

    fun updateWakeLock(context: Context) {
        if (playing && service != null) {
            val lock = wakeLock ?: (context.getSystemService(Context.POWER_SERVICE) as PowerManager)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "masamune:audio_session")
                .apply { setReferenceCounted(false) }
                .also { wakeLock = it }
            // Refreshed on every state update; the timeout only guards against a missed release.
            lock.acquire(WAKE_LOCK_TIMEOUT_MS)
        } else {
            releaseWakeLock()
        }
    }

    fun releaseWakeLock() {
        wakeLock?.let { if (it.isHeld) it.release() }
    }
}
