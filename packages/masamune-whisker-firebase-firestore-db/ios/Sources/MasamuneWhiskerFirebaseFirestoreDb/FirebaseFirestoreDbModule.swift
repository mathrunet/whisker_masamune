// Cloud Firestore reads, writes, and listeners (the default "(default)" database and named databases).
//
// A query is JSON: { database, collection, collectionGroup, filters: [{ field, op, value }], orderBy: [{ field, descending }],
// limit, limitToLast, startAt, startAfter, endAt, endBefore }.
// Documents are converted to JSON-compatible values, with the document ID under "@uid" and the path under "@path".
// On read, Timestamp becomes milliseconds, GeoPoint becomes { latitude, longitude }, and DocumentReference becomes a path string.
// Values in writes and filters can specify their type with tagged objects:
//   { "@timestamp": ms } / { "@geo": { latitude, longitude } } / { "@ref": "path" } /
//   { "@serverTimestamp": true } / { "@increment": n } / { "@arrayUnion": [...] } / { "@arrayRemove": [...] } /
//   { "@delete": true }
//
// Mirrors FirebaseFirestoreDbModule.kt. The SDK raises NSException for invalid arguments where Android
// throws, so those calls run through `MWFirestoreExceptionCatcher` and come back as errors.

import FirebaseCore
import FirebaseFirestore
import Foundation
import MasamuneWhiskerFirebaseFirestoreDbObjC
import WhiskerModule

@WhiskerModule
public final class FirebaseFirestoreDbModule: Module {
    private var listeners: [String: ListenerRegistration] = [:]

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("FirebaseFirestoreDb")
            Events("snapshot")

            // configure({ database, persistence, cacheSizeBytes }) — call before first use (configures offline persistence).
            Function("configure") { args in
                do {
                    let json = Bridge.args(args)
                    let persistence = json["persistence"] as? Bool ?? true
                    let cacheSize = Self.int64(json["cacheSizeBytes"]) ?? 0
                    let settings = FirestoreSettings()
                    if persistence {
                        settings.cacheSettings = cacheSize > 0
                            ? PersistentCacheSettings(sizeBytes: NSNumber(value: cacheSize))
                            : PersistentCacheSettings()
                    } else {
                        settings.cacheSettings = MemoryCacheSettings()
                    }
                    let db = try Self.firestore(json["database"])
                    try Self.guarded { db.settings = settings }
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // get({ query, source }) -> array of documents
            AsyncFunction("get") { args, promise in
                do {
                    let json = Bridge.args(args)
                    let query = try Self.buildQuery(Self.object(json, "query"))
                    let source = Self.source(json)
                    try Self.guarded {
                        query.getDocuments(source: source) { snapshot, error in
                            guard let snapshot else {
                                return Bridge.reject(promise, error ?? Self.failure("get failed"))
                            }
                            Bridge.resolve(promise, snapshot.documents.map(Self.document))
                        }
                    }
                } catch {
                    Bridge.reject(promise, error)
                }
            }

            // count({ query }) -> number of documents (counted on the server)
            AsyncFunction("count") { args, promise in
                do {
                    let query = try Self.buildQuery(Self.object(Bridge.args(args), "query"))
                    try Self.guarded {
                        query.count.getAggregation(source: .server) { snapshot, error in
                            guard let snapshot else {
                                return Bridge.reject(promise, error ?? Self.failure("count failed"))
                            }
                            Bridge.resolve(promise, snapshot.count.int64Value)
                        }
                    }
                } catch {
                    Bridge.reject(promise, error)
                }
            }

            // getDoc({ database, path, source }) -> document (null if missing)
            AsyncFunction("getDoc") { args, promise in
                do {
                    let json = Bridge.args(args)
                    let ref = try Self.docRef(json)
                    let source = Self.source(json)
                    try Self.guarded {
                        ref.getDocument(source: source) { snapshot, error in
                            guard let snapshot else {
                                return Bridge.reject(promise, error ?? Self.failure("getDoc failed"))
                            }
                            Bridge.resolve(promise, snapshot.exists ? Self.document(snapshot) : nil)
                        }
                    }
                } catch {
                    Bridge.reject(promise, error)
                }
            }

            // set({ database, path, data, merge })
            AsyncFunction("set") { args, promise in
                do {
                    let json = Bridge.args(args)
                    let ref = try Self.docRef(json)
                    let data = try Self.map(Self.object(json, "data"))
                    let merge = json["merge"] as? Bool ?? false
                    try Self.guarded {
                        ref.setData(data, merge: merge) { error in Self.settle(promise, error) }
                    }
                } catch {
                    Bridge.reject(promise, error)
                }
            }

            // update({ database, path, data }) — fails if the document does not exist.
            AsyncFunction("update") { args, promise in
                do {
                    let json = Bridge.args(args)
                    let ref = try Self.docRef(json)
                    let data = try Self.map(Self.object(json, "data"))
                    try Self.guarded {
                        ref.updateData(data) { error in Self.settle(promise, error) }
                    }
                } catch {
                    Bridge.reject(promise, error)
                }
            }

            // delete({ database, path })
            AsyncFunction("delete") { args, promise in
                do {
                    let ref = try Self.docRef(Bridge.args(args))
                    try Self.guarded {
                        ref.delete { error in Self.settle(promise, error) }
                    }
                } catch {
                    Bridge.reject(promise, error)
                }
            }

            // batch({ database, operations: [{ type: "set" | "update" | "delete", path, data, merge }] }) — writes all operations together in one batch.
            AsyncFunction("batch") { args, promise in
                do {
                    let json = Bridge.args(args)
                    let db = try Self.firestore(json["database"])
                    let batch = db.batch()
                    guard let operations = json["operations"] as? [Any] else {
                        throw Self.failure("JSONObject[\"operations\"] not found.")
                    }
                    for case let op as [String: Any] in operations {
                        let path = try Self.string(op, "path")
                        let type = try Self.string(op, "type")
                        switch type {
                        case "set":
                            let data = try Self.map(Self.object(op, "data"))
                            let merge = op["merge"] as? Bool ?? false
                            try Self.guarded { batch.setData(data, forDocument: db.document(path), merge: merge) }
                        case "update":
                            let data = try Self.map(Self.object(op, "data"))
                            try Self.guarded { batch.updateData(data, forDocument: db.document(path)) }
                        case "delete":
                            try Self.guarded { batch.deleteDocument(db.document(path)) }
                        default:
                            throw Self.failure("unknown batch operation: \(type)")
                        }
                    }
                    try Self.guarded {
                        batch.commit { error in Self.settle(promise, error) }
                    }
                } catch {
                    Bridge.reject(promise, error)
                }
            }

            // listen({ listenerId, query }) — sends a "snapshot" event on every change:
            // { listenerId, documents, changes: [{ type, id, oldIndex, newIndex }], fromCache, hasPendingWrites } or { listenerId, error }
            Function("listen") { [weak self] args in
                guard let self else { return Bridge.err("FirebaseFirestoreDb module released") }
                do {
                    let json = Bridge.args(args)
                    let listenerId = try Self.string(json, "listenerId")
                    self.listeners.removeValue(forKey: listenerId)?.remove()
                    let query = try Self.buildQuery(Self.object(json, "query"))
                    var registration: ListenerRegistration?
                    try Self.guarded {
                        registration = query.addSnapshotListener { [weak self] snapshot, error in
                            guard let self else { return }
                            guard let snapshot else {
                                NSLog("[MasamuneWhiskerFirestoreDb] listen failed: %@", error.map(Bridge.message) ?? "error")
                                self.sendEvent("snapshot", Bridge.event([
                                    "listenerId": listenerId,
                                    "error": error.map(Bridge.message) ?? "error",
                                ]))
                                return
                            }
                            let changes: [[String: Any?]] = snapshot.documentChanges.map { change in
                                [
                                    "type": Self.changeType(change.type),
                                    "id": change.document.documentID,
                                    "oldIndex": Self.index(change.oldIndex),
                                    "newIndex": Self.index(change.newIndex),
                                ]
                            }
                            self.sendEvent("snapshot", Bridge.event([
                                "listenerId": listenerId,
                                "documents": snapshot.documents.map(Self.document),
                                "changes": changes,
                                "fromCache": snapshot.metadata.isFromCache,
                                "hasPendingWrites": snapshot.metadata.hasPendingWrites,
                            ]))
                        }
                    }
                    if let registration { self.listeners[listenerId] = registration }
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // listenDoc({ listenerId, database, path }) — sends a "snapshot" event on every change:
            // { listenerId, document (null if missing), exists, fromCache, hasPendingWrites } or { listenerId, error }
            Function("listenDoc") { [weak self] args in
                guard let self else { return Bridge.err("FirebaseFirestoreDb module released") }
                do {
                    let json = Bridge.args(args)
                    let listenerId = try Self.string(json, "listenerId")
                    self.listeners.removeValue(forKey: listenerId)?.remove()
                    let ref = try Self.docRef(json)
                    var registration: ListenerRegistration?
                    try Self.guarded {
                        registration = ref.addSnapshotListener { [weak self] snapshot, error in
                            guard let self else { return }
                            guard let snapshot else {
                                NSLog("[MasamuneWhiskerFirestoreDb] listenDoc failed: %@", error.map(Bridge.message) ?? "error")
                                self.sendEvent("snapshot", Bridge.event([
                                    "listenerId": listenerId,
                                    "error": error.map(Bridge.message) ?? "error",
                                ]))
                                return
                            }
                            self.sendEvent("snapshot", Bridge.event([
                                "listenerId": listenerId,
                                "document": snapshot.exists ? Self.document(snapshot) : nil,
                                "exists": snapshot.exists,
                                "fromCache": snapshot.metadata.isFromCache,
                                "hasPendingWrites": snapshot.metadata.hasPendingWrites,
                            ]))
                        }
                    }
                    if let registration { self.listeners[listenerId] = registration }
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // stop({ listenerId })
            Function("stop") { [weak self] args in
                let listenerId = Bridge.args(args)["listenerId"] as? String ?? ""
                self?.listeners.removeValue(forKey: listenerId)?.remove()
                return Bridge.ok(true)
            }
        }
    }

    // MARK: - References

    private static func firestore(_ database: Any?) throws -> Firestore {
        try FirebaseBoot.ensure()
        guard let app = FirebaseApp.app() else { throw FirebaseBoot.MissingConfig() }
        let name = (database as? String).flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
        return Firestore.firestore(app: app, database: name ?? "(default)")
    }

    private static func docRef(_ json: [String: Any]) throws -> DocumentReference {
        let db = try firestore(json["database"])
        let path = try string(json, "path")
        var ref: DocumentReference?
        try guarded { ref = db.document(path) }
        guard let ref else { throw failure("invalid document path: \(path)") }
        return ref
    }

    private static func source(_ json: [String: Any]) -> FirestoreSource {
        switch json["source"] as? String {
        case "server": return .server
        case "cache": return .cache
        default: return .default
        }
    }

    // MARK: - Queries

    private static func buildQuery(_ json: [String: Any]) throws -> Query {
        let db = try firestore(json["database"])
        let collection = try string(json, "collection")
        let group = json["collectionGroup"] as? Bool ?? false
        var query: Query?
        try guarded { query = group ? db.collectionGroup(collection) : db.collection(collection) }
        guard var query else { throw failure("invalid collection: \(collection)") }

        for case let filter as [String: Any] in json["filters"] as? [Any] ?? [] {
            let field = try string(filter, "field")
            let op = try string(filter, "op")
            let value = try fromJson(filter["value"])
            let current = query
            var next: Query?
            switch op {
            case "==":
                try guarded { next = current.whereField(field, isEqualTo: value ?? NSNull()) }
            case "!=":
                try guarded { next = current.whereField(field, isNotEqualTo: value ?? NSNull()) }
            case "<":
                let value = try required(value, op)
                try guarded { next = current.whereField(field, isLessThan: value) }
            case "<=":
                let value = try required(value, op)
                try guarded { next = current.whereField(field, isLessThanOrEqualTo: value) }
            case ">":
                let value = try required(value, op)
                try guarded { next = current.whereField(field, isGreaterThan: value) }
            case ">=":
                let value = try required(value, op)
                try guarded { next = current.whereField(field, isGreaterThanOrEqualTo: value) }
            case "array-contains":
                let value = try required(value, op)
                try guarded { next = current.whereField(field, arrayContains: value) }
            case "array-contains-any":
                let values = try list(value, op)
                try guarded { next = current.whereField(field, arrayContainsAny: values) }
            case "in":
                let values = try list(value, op)
                try guarded { next = current.whereField(field, in: values) }
            case "not-in":
                let values = try list(value, op)
                try guarded { next = current.whereField(field, notIn: values) }
            default:
                throw failure("unknown filter op: \(op)")
            }
            if let next { query = next }
        }
        for case let order as [String: Any] in json["orderBy"] as? [Any] ?? [] {
            let field = try string(order, "field")
            let descending = order["descending"] as? Bool ?? false
            let current = query
            try guarded { query = current.order(by: field, descending: descending) }
        }
        if let values = try cursor(json, "startAt") {
            let current = query
            try guarded { query = current.start(at: values) }
        }
        if let values = try cursor(json, "startAfter") {
            let current = query
            try guarded { query = current.start(after: values) }
        }
        if let values = try cursor(json, "endAt") {
            let current = query
            try guarded { query = current.end(at: values) }
        }
        if let values = try cursor(json, "endBefore") {
            let current = query
            try guarded { query = current.end(before: values) }
        }
        if let limit = int64(json["limit"]) {
            let current = query
            try guarded { query = current.limit(to: Int(limit)) }
        }
        if let limit = int64(json["limitToLast"]) {
            let current = query
            try guarded { query = current.limit(toLast: Int(limit)) }
        }
        return query
    }

    private static func cursor(_ json: [String: Any], _ key: String) throws -> [Any]? {
        guard let values = json[key] as? [Any] else { return nil }
        return try values.map { try fromJson($0) ?? NSNull() }
    }

    private static func required(_ value: Any?, _ op: String) throws -> Any {
        guard let value, !(value is NSNull) else { throw failure("null is not a valid value for the \(op) filter") }
        return value
    }

    private static func list(_ value: Any?, _ op: String) throws -> [Any] {
        guard let values = value as? [Any] else { throw failure("the \(op) filter needs an array value") }
        return values
    }

    // MARK: - Reads

    private static func document(_ snapshot: DocumentSnapshot) -> [String: Any?] {
        var result: [String: Any?] = [:]
        for (key, value) in snapshot.data() ?? [:] {
            result[key] = toJson(value)
        }
        result["@uid"] = snapshot.documentID
        result["@path"] = snapshot.reference.path
        return result
    }

    private static func toJson(_ value: Any) -> Any? {
        switch value {
        case is NSNull:
            return nil
        case let value as Timestamp:
            // Same as Android's `Timestamp.toDate().time`.
            return value.seconds * 1000 + Int64(value.nanoseconds / 1_000_000)
        case let value as GeoPoint:
            return ["latitude": value.latitude, "longitude": value.longitude]
        case let value as DocumentReference:
            return value.path
        case let value as [String: Any]:
            return value.mapValues { toJson($0) }
        case let value as [Any]:
            return value.map { toJson($0) }
        case let value as NSNumber:
            return value
        case let value as String:
            return value
        case let value as Data:
            // Blobs have no JSON form on Android either; base64 keeps the bytes readable.
            return value.base64EncodedString()
        default:
            return String(describing: value)
        }
    }

    private static func changeType(_ type: DocumentChangeType) -> String {
        switch type {
        case .added: return "added"
        case .modified: return "modified"
        case .removed: return "removed"
        @unknown default: return "modified"
        }
    }

    /// Android reports -1 where iOS reports `NSNotFound` (added / removed documents).
    private static func index(_ value: UInt) -> Int64 {
        value == UInt(NSNotFound) ? -1 : Int64(value)
    }

    // MARK: - Writes

    private static func map(_ json: [String: Any]) throws -> [String: Any] {
        var result: [String: Any] = [:]
        for (key, value) in json where key != "@uid" && key != "@path" {
            result[key] = try fromJson(value) ?? NSNull()
        }
        return result
    }

    private static func fromJson(_ value: Any?) throws -> Any? {
        switch value {
        case nil, is NSNull:
            return nil
        case let value as [Any]:
            return try value.map { try fromJson($0) ?? NSNull() }
        case let value as [String: Any]:
            if let tagged = try tagged(value) { return tagged }
            var result: [String: Any] = [:]
            for (key, item) in value {
                result[key] = try fromJson(item) ?? NSNull()
            }
            return result
        case let value as NSNumber:
            return number(value)
        default:
            return value
        }
    }

    /// Converts a tagged object (e.g. { "@timestamp": ms }) to a Firestore type. Returns nil if it is not a tag.
    private static func tagged(_ json: [String: Any]) throws -> Any? {
        guard json.count == 1, let (key, value) = json.first else { return nil }
        switch key {
        case "@timestamp":
            guard let millis = int64(value) else { throw failure("JSONObject[\"@timestamp\"] is not a long.") }
            let seconds = Int64((Double(millis) / 1000).rounded(.down))
            return Timestamp(seconds: seconds, nanoseconds: Int32((millis - seconds * 1000) * 1_000_000))
        case "@geo":
            guard let geo = value as? [String: Any],
                  let latitude = (geo["latitude"] as? NSNumber)?.doubleValue,
                  let longitude = (geo["longitude"] as? NSNumber)?.doubleValue
            else { throw failure("JSONObject[\"@geo\"] needs latitude and longitude.") }
            var point: GeoPoint?
            try guarded { point = GeoPoint(latitude: latitude, longitude: longitude) }
            return point
        case "@ref":
            guard let path = value as? String else { throw failure("JSONObject[\"@ref\"] is not a string.") }
            // Like Android, references point into the default database.
            let db = try firestore(nil)
            var ref: DocumentReference?
            try guarded { ref = db.document(path) }
            return ref
        case "@serverTimestamp":
            return FieldValue.serverTimestamp()
        case "@increment":
            guard let number = value as? NSNumber else { throw failure("JSONObject[\"@increment\"] is not a number.") }
            if let integer = self.number(number) as? Int64 {
                return FieldValue.increment(integer)
            }
            return FieldValue.increment(number.doubleValue)
        case "@arrayUnion":
            guard let values = value as? [Any] else { throw failure("JSONObject[\"@arrayUnion\"] is not a JSONArray.") }
            return FieldValue.arrayUnion(try values.map { try fromJson($0) ?? NSNull() })
        case "@arrayRemove":
            guard let values = value as? [Any] else { throw failure("JSONObject[\"@arrayRemove\"] is not a JSONArray.") }
            return FieldValue.arrayRemove(try values.map { try fromJson($0) ?? NSNull() })
        case "@delete":
            return FieldValue.delete()
        default:
            return nil
        }
    }

    // MARK: - JSON helpers

    /// JSON numbers as Firestore expects them: Bool, Int64 or Double.
    private static func number(_ value: NSNumber) -> Any {
        if CFGetTypeID(value) == CFBooleanGetTypeID() { return value.boolValue }
        switch String(cString: value.objCType) {
        case "f", "d": return value.doubleValue
        default: return value.int64Value
        }
    }

    private static func int64(_ value: Any?) -> Int64? {
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
        return number.int64Value
    }

    private static func string(_ json: [String: Any], _ key: String) throws -> String {
        guard let value = json[key] as? String else { throw failure("JSONObject[\"\(key)\"] not found.") }
        return value
    }

    private static func object(_ json: [String: Any], _ key: String) throws -> [String: Any] {
        guard let value = json[key] as? [String: Any] else { throw failure("JSONObject[\"\(key)\"] not found.") }
        return value
    }

    private static func settle(_ promise: WhiskerPromise, _ error: Error?) {
        if let error { Bridge.reject(promise, error) } else { Bridge.resolve(promise, true) }
    }

    /// Runs SDK calls that may raise NSException and rethrows them as Swift errors.
    private static func guarded(_ block: () -> Void) throws {
        try MWFirestoreExceptionCatcher.perform(block)
    }

    private static func failure(_ message: String) -> NSError {
        NSError(domain: "MasamuneWhiskerFirestoreDb", code: 0, userInfo: [NSLocalizedDescriptionKey: message])
    }
}
