package net.mathru.whisker.messaging_firebase

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.RemoteMessage
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerValue

@WhiskerModule
class FirebaseMessagingModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("FirebaseMessaging")
        Events("tokenRefresh", "message", "messageOpened")

        MessagingHub.attach(this@FirebaseMessagingModule)

        AsyncFunction("requestPermission") { _, promise ->
            val context = WhiskerApplication.appContext
            if (context == null) {
                Bridge.resolve(promise, false)
                return@AsyncFunction
            }
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
                Bridge.resolve(promise, true)
                return@AsyncFunction
            }
            val granted = ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
            Bridge.resolve(promise, granted)
        }

        AsyncFunction("getToken") { _, promise ->
            FirebaseMessaging.getInstance().token
                .addOnSuccessListener { Bridge.resolve(promise, it) }
                .addOnFailureListener { Bridge.reject(promise, it) }
        }

        AsyncFunction("deleteToken") { _, promise ->
            FirebaseMessaging.getInstance().deleteToken()
                .addOnSuccessListener { Bridge.resolve(promise, true) }
                .addOnFailureListener { Bridge.reject(promise, it) }
        }

        AsyncFunction("getInitialMessage") { _, promise ->
            Bridge.resolve(promise, MessagingHub.takeInitialMessage())
        }
    }

    internal fun emitToken(token: String) {
        sendEvent("tokenRefresh", WhiskerValue.Str(token))
    }

    internal fun emitMessage(payload: Map<String, Any?>) {
        sendEvent("message", Bridge.event(payload))
    }
}

internal object MessagingHub {
    private var module: FirebaseMessagingModule? = null
    private var initialMessage: Map<String, Any?>? = null

    @Synchronized
    fun attach(instance: FirebaseMessagingModule) {
        module = instance
    }

    @Synchronized
    fun takeInitialMessage(): Map<String, Any?>? {
        val current = initialMessage
        initialMessage = null
        return current
    }

    fun onNewToken(token: String) {
        Bridge.onMain { module?.emitToken(token) }
    }

    fun onMessageReceived(remote: RemoteMessage) {
        val payload = mapOf(
            "id" to remote.messageId,
            "title" to remote.notification?.title,
            "body" to remote.notification?.body,
            "data" to remote.data,
        )
        Bridge.onMain { module?.emitMessage(payload) }
    }
}
