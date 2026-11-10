// Cloud Firestore（既定のデータベース "(default)" と名前付きデータベース）の読み書きと監視。
//
// クエリは { database, collection, collectionGroup, filters: [{ field, op, value }], orderBy: [{ field, descending }],
// limit, limitToLast, startAt, startAfter, endAt, endBefore } の JSON。
// ドキュメントは JSON 化できる値に変換し、ドキュメント ID を "@uid"、パスを "@path" キーに入れて返す。
// 読み取りでは Timestamp はミリ秒、GeoPoint は { latitude, longitude }、DocumentReference はパス文字列。
// 書き込み・条件の値はタグ付きオブジェクトで型を指定できる:
//   { "@timestamp": ms } / { "@geo": { latitude, longitude } } / { "@ref": "path" } /
//   { "@serverTimestamp": true } / { "@increment": n } / { "@arrayUnion": [...] } / { "@arrayRemove": [...] } /
//   { "@delete": true }

package net.mathru.whisker.firebase_firestore_db

import android.util.Log
import com.google.firebase.FirebaseApp
import com.google.firebase.Timestamp
import com.google.firebase.firestore.AggregateSource
import com.google.firebase.firestore.DocumentChange
import com.google.firebase.firestore.DocumentReference
import com.google.firebase.firestore.DocumentSnapshot
import com.google.firebase.firestore.FieldValue
import com.google.firebase.firestore.FirebaseFirestore
import com.google.firebase.firestore.FirebaseFirestoreSettings
import com.google.firebase.firestore.GeoPoint
import com.google.firebase.firestore.ListenerRegistration
import com.google.firebase.firestore.MemoryCacheSettings
import com.google.firebase.firestore.PersistentCacheSettings
import com.google.firebase.firestore.Query
import com.google.firebase.firestore.SetOptions
import com.google.firebase.firestore.Source
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import java.util.Date

@WhiskerModule
class FirebaseFirestoreDbModule : Module() {
    private val listeners = mutableMapOf<String, ListenerRegistration>()

    override fun definition() = ModuleDefinition {
        Name("FirebaseFirestoreDb")
        Events("snapshot")

        // configure({ database, persistence, cacheSizeBytes }) — 最初に使う前に呼ぶ（オフライン永続化の設定）。
        Function("configure") { args ->
            try {
                val json = Bridge.args(args)
                val persistence = json.optBoolean("persistence", true)
                val cacheSize = json.optLong("cacheSizeBytes", 0L)
                val cache = if (persistence) {
                    val builder = PersistentCacheSettings.newBuilder()
                    if (cacheSize > 0) builder.setSizeBytes(cacheSize)
                    builder.build()
                } else {
                    MemoryCacheSettings.newBuilder().build()
                }
                firestore(json.optString("database")).firestoreSettings =
                    FirebaseFirestoreSettings.Builder().setLocalCacheSettings(cache).build()
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // get({ query, source }) -> ドキュメントの配列
        AsyncFunction("get") { args, promise ->
            try {
                val json = Bridge.args(args)
                buildQuery(json.getJSONObject("query")).get(source(json))
                    .addOnSuccessListener { snapshot -> Bridge.resolve(promise, snapshot.documents.map(::document)) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // count({ query }) -> 件数（サーバーで数える）
        AsyncFunction("count") { args, promise ->
            try {
                buildQuery(Bridge.args(args).getJSONObject("query")).count().get(AggregateSource.SERVER)
                    .addOnSuccessListener { snapshot -> Bridge.resolve(promise, snapshot.count) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // getDoc({ database, path, source }) -> ドキュメント（無ければ null）
        AsyncFunction("getDoc") { args, promise ->
            try {
                val json = Bridge.args(args)
                docRef(json).get(source(json))
                    .addOnSuccessListener { snapshot -> Bridge.resolve(promise, if (snapshot.exists()) document(snapshot) else null) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // set({ database, path, data, merge })
        AsyncFunction("set") { args, promise ->
            try {
                val json = Bridge.args(args)
                val data = map(json.getJSONObject("data"))
                val task = if (json.optBoolean("merge")) docRef(json).set(data, SetOptions.merge()) else docRef(json).set(data)
                task.addOnSuccessListener { Bridge.resolve(promise, true) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // update({ database, path, data }) — 存在しなければ失敗する。
        AsyncFunction("update") { args, promise ->
            try {
                val json = Bridge.args(args)
                docRef(json).update(map(json.getJSONObject("data")))
                    .addOnSuccessListener { Bridge.resolve(promise, true) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // delete({ database, path })
        AsyncFunction("delete") { args, promise ->
            try {
                docRef(Bridge.args(args)).delete()
                    .addOnSuccessListener { Bridge.resolve(promise, true) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // batch({ database, operations: [{ type: "set" | "update" | "delete", path, data, merge }] }) — まとめて書く。
        AsyncFunction("batch") { args, promise ->
            try {
                val json = Bridge.args(args)
                val db = firestore(json.optString("database"))
                val batch = db.batch()
                val operations = json.getJSONArray("operations")
                for (i in 0 until operations.length()) {
                    val op = operations.getJSONObject(i)
                    val ref = db.document(op.getString("path"))
                    when (op.getString("type")) {
                        "set" -> if (op.optBoolean("merge")) batch.set(ref, map(op.getJSONObject("data")), SetOptions.merge())
                        else batch.set(ref, map(op.getJSONObject("data")))
                        "update" -> batch.update(ref, map(op.getJSONObject("data")))
                        "delete" -> batch.delete(ref)
                        else -> throw IllegalArgumentException("unknown batch operation: ${op.getString("type")}")
                    }
                }
                batch.commit()
                    .addOnSuccessListener { Bridge.resolve(promise, true) }
                    .addOnFailureListener { Bridge.reject(promise, it) }
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }

        // listen({ listenerId, query }) — 変更のたびに "snapshot" イベントを送る:
        // { listenerId, documents, changes: [{ type, id, oldIndex, newIndex }], fromCache, hasPendingWrites } か { listenerId, error }
        Function("listen") { args ->
            try {
                val json = Bridge.args(args)
                val listenerId = json.getString("listenerId")
                listeners.remove(listenerId)?.remove()
                listeners[listenerId] = buildQuery(json.getJSONObject("query")).addSnapshotListener { snapshot, error ->
                    if (error != null || snapshot == null) {
                        Log.w(TAG, "listen failed", error)
                        sendEvent("snapshot", Bridge.event(mapOf("listenerId" to listenerId, "error" to (error?.message ?: "error"))))
                        return@addSnapshotListener
                    }
                    val changes = snapshot.documentChanges.map { change ->
                        mapOf(
                            "type" to when (change.type) {
                                DocumentChange.Type.ADDED -> "added"
                                DocumentChange.Type.MODIFIED -> "modified"
                                DocumentChange.Type.REMOVED -> "removed"
                            },
                            "id" to change.document.id,
                            "oldIndex" to change.oldIndex,
                            "newIndex" to change.newIndex,
                        )
                    }
                    sendEvent("snapshot", Bridge.event(mapOf(
                        "listenerId" to listenerId,
                        "documents" to snapshot.documents.map(::document),
                        "changes" to changes,
                        "fromCache" to snapshot.metadata.isFromCache,
                        "hasPendingWrites" to snapshot.metadata.hasPendingWrites(),
                    )))
                }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // listenDoc({ listenerId, database, path }) — 変更のたびに "snapshot" イベントを送る:
        // { listenerId, document（無ければ null）, exists, fromCache, hasPendingWrites } か { listenerId, error }
        Function("listenDoc") { args ->
            try {
                val json = Bridge.args(args)
                val listenerId = json.getString("listenerId")
                listeners.remove(listenerId)?.remove()
                listeners[listenerId] = docRef(json).addSnapshotListener { snapshot, error ->
                    if (error != null || snapshot == null) {
                        Log.w(TAG, "listenDoc failed", error)
                        sendEvent("snapshot", Bridge.event(mapOf("listenerId" to listenerId, "error" to (error?.message ?: "error"))))
                        return@addSnapshotListener
                    }
                    sendEvent("snapshot", Bridge.event(mapOf(
                        "listenerId" to listenerId,
                        "document" to if (snapshot.exists()) document(snapshot) else null,
                        "exists" to snapshot.exists(),
                        "fromCache" to snapshot.metadata.isFromCache,
                        "hasPendingWrites" to snapshot.metadata.hasPendingWrites(),
                    )))
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

    private fun firestore(database: String?): FirebaseFirestore {
        val name = database?.takeIf { it.isNotBlank() } ?: "(default)"
        return FirebaseFirestore.getInstance(FirebaseApp.getInstance(), name)
    }

    private fun docRef(json: JSONObject): DocumentReference = firestore(json.optString("database")).document(json.getString("path"))

    private fun source(json: JSONObject): Source = when (json.optString("source")) {
        "server" -> Source.SERVER
        "cache" -> Source.CACHE
        else -> Source.DEFAULT
    }

    private fun buildQuery(json: JSONObject): Query {
        val db = firestore(json.optString("database"))
        val collection = json.getString("collection")
        var query: Query = if (json.optBoolean("collectionGroup")) db.collectionGroup(collection) else db.collection(collection)
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
                    "array-contains-any" -> query.whereArrayContainsAny(field, value as List<Any?>)
                    "in" -> query.whereIn(field, value as List<Any?>)
                    "not-in" -> query.whereNotIn(field, value as List<Any?>)
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
        cursor(json, "startAt")?.let { query = query.startAt(*it) }
        cursor(json, "startAfter")?.let { query = query.startAfter(*it) }
        cursor(json, "endAt")?.let { query = query.endAt(*it) }
        cursor(json, "endBefore")?.let { query = query.endBefore(*it) }
        if (json.has("limit")) query = query.limit(json.getLong("limit"))
        if (json.has("limitToLast")) query = query.limitToLast(json.getLong("limitToLast"))
        return query
    }

    private fun cursor(json: JSONObject, key: String): Array<Any?>? {
        val values = json.optJSONArray(key) ?: return null
        return (0 until values.length()).map { fromJson(values.get(it)) }.toTypedArray()
    }

    private fun document(doc: DocumentSnapshot): Map<String, Any?> {
        val data = (doc.data ?: emptyMap()).mapValues { toJson(it.value) }
        return data + mapOf("@uid" to doc.id, "@path" to doc.reference.path)
    }

    private fun toJson(value: Any?): Any? = when (value) {
        is Timestamp -> value.toDate().time
        is GeoPoint -> mapOf("latitude" to value.latitude, "longitude" to value.longitude)
        is DocumentReference -> value.path
        is Map<*, *> -> value.entries.associate { it.key.toString() to toJson(it.value) }
        is List<*> -> value.map { toJson(it) }
        else -> value
    }

    private fun map(json: JSONObject): Map<String, Any?> =
        json.keys().asSequence().filterNot { it == "@uid" || it == "@path" }.associateWith { fromJson(json.get(it)) }

    private fun fromJson(value: Any?): Any? = when (value) {
        null, JSONObject.NULL -> null
        is JSONArray -> (0 until value.length()).map { fromJson(value.get(it)) }
        is JSONObject -> tagged(value) ?: value.keys().asSequence().associateWith { fromJson(value.get(it)) }
        else -> value
    }

    /** タグ付きオブジェクト（{ "@timestamp": ms } など）を Firestore の型にする。タグでなければ null。 */
    private fun tagged(json: JSONObject): Any? {
        if (json.length() != 1) return null
        val key = json.keys().next()
        return when (key) {
            "@timestamp" -> Timestamp(Date(json.getLong(key)))
            "@geo" -> json.getJSONObject(key).let { GeoPoint(it.getDouble("latitude"), it.getDouble("longitude")) }
            "@ref" -> FirebaseFirestore.getInstance().document(json.getString(key))
            "@serverTimestamp" -> FieldValue.serverTimestamp()
            "@increment" -> json.get(key).let { if (it is Int || it is Long) FieldValue.increment((it as Number).toLong()) else FieldValue.increment((it as Number).toDouble()) }
            "@arrayUnion" -> FieldValue.arrayUnion(*(fromJson(json.getJSONArray(key)) as List<*>).toTypedArray())
            "@arrayRemove" -> FieldValue.arrayRemove(*(fromJson(json.getJSONArray(key)) as List<*>).toTypedArray())
            "@delete" -> FieldValue.delete()
            else -> null
        }
    }

    private companion object {
        const val TAG = "MasamuneWhiskerFirestoreDb"
    }
}
