// Shared helpers that exchange every value with Rust as a JSON string.
//
// Rust calls `invoke_async(method, [Str(json)])` and receives the result as a JSON string too.
// Failures are returned as `WhiskerValue.Err(message)`.

package net.mathru.whisker.app_control

import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.WhiskerPromise
import rs.whisker.runtime.WhiskerValue

internal object Bridge {
    private val mainHandler = Handler(Looper.getMainLooper())

    /** Reads the first argument as a JSON object (an empty object when missing). */
    fun args(args: List<WhiskerValue>): JSONObject {
        val raw = args.getOrNull(0)?.asString() ?: return JSONObject()
        return if (raw.isBlank()) JSONObject() else JSONObject(raw)
    }

    /** Returns a successful value as a JSON string. */
    fun ok(value: Any?): WhiskerValue = WhiskerValue.Str(JSONObject().put("value", wrap(value)).toString())

    fun err(error: Throwable): WhiskerValue = WhiskerValue.Err(error.message ?: error.javaClass.simpleName)

    fun resolve(promise: WhiskerPromise, value: Any?) {
        promise.resolve(ok(value))
    }

    fun reject(promise: WhiskerPromise, error: Throwable) {
        promise.reject(error.message ?: error.javaClass.simpleName)
    }

    fun reject(promise: WhiskerPromise, message: String) {
        promise.reject(message)
    }

    fun onMain(block: () -> Unit) {
        if (Looper.myLooper() == Looper.getMainLooper()) block() else mainHandler.post(block)
    }

    /** Converts a Kotlin value into a value org.json can hold. */
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

    /** Builds an event payload (a JSON string). */
    fun event(payload: Map<String, Any?>): WhiskerValue = WhiskerValue.Str(wrap(payload).toString())
}
