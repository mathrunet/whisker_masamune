package net.mathru.whisker.messaging_firebase

import android.Manifest
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.util.Consumer
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.RemoteMessage
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.RuntimeAttachedListener
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerValue
import java.lang.ref.WeakReference
import java.util.concurrent.atomic.AtomicInteger

@WhiskerModule
class FirebaseMessagingModule : Module() {
    private val main = Handler(Looper.getMainLooper())
    private val permissionRequests = AtomicInteger()
    private var activity = WeakReference<ComponentActivity>(null)
    private var observingOpened = false
    private var attachedBefore = false
    private val queued = ArrayDeque<Map<String, Any?>>()
    private val onNewIntent = Consumer<Intent> { intent -> claim(intent)?.let(::deliver) }
    private val onAttached = RuntimeAttachedListener { main.post(::attachIntentListener) }

    override fun definition() = ModuleDefinition {
        Name("FirebaseMessaging")
        Events("tokenRefresh", "message", "messageOpened")

        MessagingHub.attach(this@FirebaseMessagingModule)

        OnStartObserving("messageOpened") {
            appContext.addOnRuntimeAttachedListener(onAttached)
            main.post {
                observingOpened = true
                while (queued.isNotEmpty()) deliver(queued.removeFirst())
            }
        }
        OnStopObserving("messageOpened") {
            appContext.removeOnRuntimeAttachedListener(onAttached)
            main.post {
                observingOpened = false
                activity.get()?.removeOnNewIntentListener(onNewIntent)
                activity.clear()
            }
        }

        AsyncFunction("requestPermission") { _, promise ->
            val context = WhiskerApplication.appContext
            if (context == null) {
                Bridge.resolve(promise, false)
                return@AsyncFunction
            }
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
                ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
            ) {
                Bridge.resolve(promise, NotificationManagerCompat.from(context).areNotificationsEnabled())
                return@AsyncFunction
            }
            Bridge.onMain {
                val current = appContext.currentActivity as? ComponentActivity
                if (current == null) {
                    Bridge.resolve(promise, false)
                    return@onMain
                }
                val key = "masamune_whisker_push_permission_${permissionRequests.incrementAndGet()}"
                var launcher: androidx.activity.result.ActivityResultLauncher<String>? = null
                launcher = current.activityResultRegistry.register(key, ActivityResultContracts.RequestPermission()) { granted ->
                    launcher?.unregister()
                    Bridge.resolve(promise, granted && NotificationManagerCompat.from(context).areNotificationsEnabled())
                }
                launcher.launch(Manifest.permission.POST_NOTIFICATIONS)
            }
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
            lateinit var listener: RuntimeAttachedListener
            listener = RuntimeAttachedListener {
                main.post {
                    appContext.removeOnRuntimeAttachedListener(listener)
                    Bridge.resolve(promise, claim(appContext.currentActivity?.intent))
                }
            }
            appContext.addOnRuntimeAttachedListener(listener)
        }
    }

    private fun attachIntentListener() {
        val current = appContext.currentActivity as? ComponentActivity ?: return
        if (current === activity.get()) return
        activity.get()?.removeOnNewIntentListener(onNewIntent)
        current.addOnNewIntentListener(onNewIntent)
        activity = WeakReference(current)
        if (attachedBefore) claim(current.intent)?.let(::deliver)
        attachedBefore = true
    }

    private fun deliver(payload: Map<String, Any?>) {
        if (observingOpened) sendEvent("messageOpened", Bridge.event(payload))
        else queued.add(payload)
    }

    private fun claim(intent: Intent?): Map<String, Any?>? {
        if (intent == null || intent.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY != 0) return null
        val consumed = "net.mathru.whisker.messaging_firebase.opened"
        if (intent.getBooleanExtra(consumed, false)) return null
        val id = intent.getStringExtra("google.message_id") ?: intent.getStringExtra("message_id") ?: return null
        val extras = intent.extras ?: return null
        val data = extras.keySet()
            .filterNot { it.startsWith("google.") || it.startsWith("gcm.") || it in setOf("message_id", "from", "collapse_key") }
            .mapNotNull { key -> extras.getString(key)?.let { key to it } }
            .toMap()
        intent.putExtra(consumed, true)
        return mapOf("id" to id, "data" to data)
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
    @Synchronized
    fun attach(instance: FirebaseMessagingModule) {
        module = instance
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
