package net.mathru.whisker.messaging_firebase

import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.WhiskerPromise
import rs.whisker.runtime.WhiskerValue

internal object Bridge {
    private val mainHandler = Handler(Looper.getMainLooper())

    fun ok(value: Any?): WhiskerValue = WhiskerValue.Str(JSONObject().put("value", wrap(value)).toString())

    fun resolve(promise: WhiskerPromise, value: Any?) {
        promise.resolve(ok(value))
    }

    fun reject(promise: WhiskerPromise, error: Throwable) {
        promise.reject(error.message ?: error.javaClass.simpleName)
    }

    fun onMain(block: () -> Unit) {
        if (Looper.myLooper() == Looper.getMainLooper()) block() else mainHandler.post(block)
    }

    fun wrap(value: Any?): Any = when (value) {
        null -> JSONObject.NULL
        is JSONObject, is JSONArray -> value
        is Map<*, *> -> JSONObject().also { obj ->
            value.forEach { (k, v) -> obj.put(k.toString(), wrap(v)) }
        }
        is Iterable<*> -> JSONArray().also { arr -> value.forEach { arr.put(wrap(it)) } }
        is Array<*> -> JSONArray().also { arr -> value.forEach { arr.put(wrap(it)) } }
        else -> value
    }

    fun event(payload: Map<String, Any?>): WhiskerValue = WhiskerValue.Str(wrap(payload).toString())
}
