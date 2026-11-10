// Rust との受け渡しを JSON 文字列に統一するための共通処理。
//
// Rust 側は `invoke_async(method, [Str(json)])` で呼び出し、結果も JSON 文字列で受け取る。
// 失敗は `WhiskerValue.Err(message)` で返す。

package net.mathru.whisker.firebase_ai

import android.os.Handler
import android.os.Looper
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.WhiskerPromise
import rs.whisker.runtime.WhiskerValue

internal object Bridge {
    private val mainHandler = Handler(Looper.getMainLooper())

    /** 引数の先頭を JSON オブジェクトとして読む。未指定なら空オブジェクト。 */
    fun args(args: List<WhiskerValue>): JSONObject {
        val raw = args.getOrNull(0)?.asString() ?: return JSONObject()
        return if (raw.isBlank()) JSONObject() else JSONObject(raw)
    }

    /** 成功値を JSON 文字列で返す。 */
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

    /** Kotlin の値を org.json で扱える値に変換する。 */
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

    /** イベントの送信用。ペイロードは JSON 文字列。 */
    fun event(payload: Map<String, Any?>): WhiskerValue = WhiskerValue.Str(wrap(payload).toString())
}
