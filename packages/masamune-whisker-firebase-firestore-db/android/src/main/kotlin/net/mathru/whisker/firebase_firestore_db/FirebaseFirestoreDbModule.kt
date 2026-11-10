// Cloud Firestore の名前付きデータベース（既定は "(default)"）に対するクエリの取得と監視。
//
// クエリは { database, collection, filters: [{ field, op, value }], orderBy: [{ field, descending }], limit } の JSON。
// ドキュメントは JSON 化できる値に変換し、ドキュメント ID を "@uid" キーに入れて返す。
// Timestamp はミリ秒、GeoPoint は { latitude, longitude }、DocumentReference はパス文字列。

package net.mathru.whisker.firebase_firestore_db

import android.util.Log
import com.google.firebase.FirebaseApp
import com.google.firebase.Timestamp
import com.google.firebase.firestore.DocumentReference
import com.google.firebase.firestore.DocumentSnapshot
import com.google.firebase.firestore.FirebaseFirestore
import com.google.firebase.firestore.GeoPoint
import com.google.firebase.firestore.ListenerRegistration
import com.google.firebase.firestore.Query
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class FirebaseFirestoreDbModule : Module() {
    private val listeners = mutableMapOf<String, ListenerRegistration>()

    override fun definition() = ModuleDefinition {
        Name("FirebaseFirestoreDb")
        Events("snapshot")

        // get({ query }) -> ドキュメントの配列
        AsyncFunction("get") { args, promise ->
            try {
                buildQuery(Bridge.args(args).getJSONObject("query")).get()
                    .addOnSuccessListener { snapshot -> Bridge.resolve(promise, snapshot.documents.map(::document)) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // listen({ listenerId, query }) — 変更のたびに "snapshot" イベント（{ listenerId, documents } か { listenerId, error }）を送る。
        Function("listen") { args ->
            try {
                val json = Bridge.args(args)
                val listenerId = json.getString("listenerId")
                listeners.remove(listenerId)?.remove()
                listeners[listenerId] = buildQuery(json.getJSONObject("query")).addSnapshotListener { snapshot, error ->
                    if (error != null) {
                        Log.w(TAG, "listen failed", error)
                        sendEvent("snapshot", Bridge.event(mapOf("listenerId" to listenerId, "error" to (error.message ?: "error"))))
                        return@addSnapshotListener
                    }
                    val documents = snapshot?.documents?.map(::document) ?: emptyList()
                    sendEvent("snapshot", Bridge.event(mapOf("listenerId" to listenerId, "documents" to documents)))
                }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // stop({ listenerId })
        Function("stop") { args ->
            listeners.remove(Bridge.args(args).optString("listenerId"))?.remove()
            Bridge.ok(true)
        }
    }

    private fun buildQuery(json: JSONObject): Query {
        val database = json.optString("database").ifBlank { "(default)" }
        val firestore = FirebaseFirestore.getInstance(FirebaseApp.getInstance(), database)
        var query: Query = firestore.collection(json.getString("collection"))
        json.optJSONArray("filters")?.let { filters ->
            for (i in 0 until filters.length()) {
                val filter = filters.getJSONObject(i)
                val field = filter.getString("field")
                val value = fromJson(filter.opt("value"))
                query = when (filter.getString("op")) {
                    "==" -> query.whereEqualTo(field, value)
                    "!=" -> query.whereNotEqualTo(field, value)
                    "<" -> query.whereLessThan(field, value!!)
                    "<=" -> query.whereLessThanOrEqualTo(field, value!!)
                    ">" -> query.whereGreaterThan(field, value!!)
                    ">=" -> query.whereGreaterThanOrEqualTo(field, value!!)
                    "array-contains" -> query.whereArrayContains(field, value!!)
                    "in" -> query.whereIn(field, value as List<Any?>)
                    else -> throw IllegalArgumentException("unknown filter op: ${filter.getString("op")}")
                }
            }
        }
        json.optJSONArray("orderBy")?.let { orders ->
            for (i in 0 until orders.length()) {
                val order = orders.getJSONObject(i)
                val direction = if (order.optBoolean("descending")) Query.Direction.DESCENDING else Query.Direction.ASCENDING
                query = query.orderBy(order.getString("field"), direction)
            }
        }
        if (json.has("limit")) query = query.limit(json.getLong("limit"))
        return query
    }

    private fun document(doc: DocumentSnapshot): Map<String, Any?> {
        val data = (doc.data ?: emptyMap()).mapValues { toJson(it.value) }
        return data + ("@uid" to doc.id)
    }

    private fun toJson(value: Any?): Any? = when (value) {
        is Timestamp -> value.toDate().time
        is GeoPoint -> mapOf("latitude" to value.latitude, "longitude" to value.longitude)
        is DocumentReference -> value.path
        is Map<*, *> -> value.entries.associate { it.key.toString() to toJson(it.value) }
        is List<*> -> value.map { toJson(it) }
        else -> value
    }

    private fun fromJson(value: Any?): Any? = when (value) {
        null, JSONObject.NULL -> null
        is JSONArray -> (0 until value.length()).map { fromJson(value.get(it)) }
        is JSONObject -> value.keys().asSequence().associateWith { fromJson(value.get(it)) }
        else -> value
    }

    private companion object {
        const val TAG = "MasamuneWhiskerFirestoreDb"
    }
}
