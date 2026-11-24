// Foreground service (type mediaPlayback) that keeps the process alive while a Now Playing item
// is set, and the receiver of the media notification buttons.
//
// The service also holds a Whisker runtime retention lease. Removing the task from Recents
// destroys the Activity while playback continues; the lease keeps the Rust runtime that answers
// the notification buttons running until the Now Playing item is cleared.

package net.mathru.whisker.audio_session

import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.ServiceCompat
import rs.whisker.runtime.WhiskerRuntimeRetention

class AudioSessionService : Service() {
    private var retention: WhiskerRuntimeRetention.Lease? = null

    // Android can leave the service instance alive after denying or revoking foreground status.
    internal fun isPlaybackForeground(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.Q ||
            foregroundServiceType and ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK != 0

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        AudioSessions.service = this
        retention = WhiskerRuntimeRetention.acquire("masamune-whisker-audio-session")
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        ServiceCompat.startForeground(
            this,
            AudioSessions.NOTIFICATION_ID,
            AudioSessions.notification(this),
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK,
        )
        AudioSessions.updateWakeLock(this)
        // Not restarted after the process is killed: the Rust app has to set the item again.
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        AudioSessions.service = null
        AudioSessions.releaseWakeLock()
        retention?.release()
        retention = null
        super.onDestroy()
    }
}

/** Forwards media notification buttons (`<prefix><command>[:<id>]`) as remote commands. */
class AudioSessionActionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return
        if (!action.startsWith(AudioSessions.ACTION_PREFIX)) return
        val command = action.removePrefix(AudioSessions.ACTION_PREFIX).substringBefore(':')
        if (command == "custom") {
            val id = intent.getStringExtra(AudioSessions.EXTRA_CUSTOM_ID) ?: return
            AudioSessions.dispatch(mapOf("command" to "custom", "id" to id))
        } else if (command in COMMANDS) {
            AudioSessions.dispatch(mapOf("command" to command))
        }
    }

    private companion object {
        val COMMANDS = setOf("play", "pause", "next", "previous", "stop")
    }
}
