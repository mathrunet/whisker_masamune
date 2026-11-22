// Shared helpers that exchange every value with Rust as a JSON string.
//
// Rust calls `invoke_async(method, [String(json)])` and receives the result as a JSON string too.
// Failures are returned as `WhiskerValue.error(message)`. Mirrors `Bridge.kt` on Android.

import Foundation
import WhiskerModule

enum Bridge {
    /// Reads the first argument as a JSON object (an empty object when missing or invalid).
    static func args(_ args: [WhiskerValue]) -> [String: Any] {
        guard let raw = args.first?.asString, !raw.isEmpty,
              let data = raw.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return object
    }

    /// Returns a successful value as a `{"value": ...}` JSON string.
    static func ok(_ value: Any?) -> WhiskerValue {
        .string(json(["value": wrap(value)]))
    }

    static func err(_ error: Error) -> WhiskerValue {
        .error(message(error))
    }

    static func err(_ message: String) -> WhiskerValue {
        .error(message)
    }

    static func resolve(_ promise: WhiskerPromise, _ value: Any?) {
        promise.resolve(ok(value))
    }

    static func reject(_ promise: WhiskerPromise, _ error: Error) {
        promise.reject(message(error))
    }

    static func reject(_ promise: WhiskerPromise, _ message: String) {
        promise.reject(message)
    }

    /// Runs `block` on the main thread, immediately when already there.
    static func onMain(_ block: @escaping () -> Void) {
        if Thread.isMainThread { block() } else { DispatchQueue.main.async(execute: block) }
    }

    /// Builds an event payload (a JSON string).
    static func event(_ payload: [String: Any?]) -> WhiskerValue {
        .string(json(wrap(payload)))
    }

    static func message(_ error: Error) -> String {
        let ns = error as NSError
        return ns.localizedDescription.isEmpty ? "\(ns.domain) \(ns.code)" : ns.localizedDescription
    }

    /// Converts a Swift value into a value `JSONSerialization` can hold.
    static func wrap(_ value: Any?) -> Any {
        switch value {
        case nil:
            return NSNull()
        case let value as [String: Any?]:
            return value.mapValues { wrap($0) }
        case let value as [String: Any]:
            return value.mapValues { wrap($0) }
        case let value as [Any?]:
            return value.map { wrap($0) }
        case let value as Date:
            return Int64(value.timeIntervalSince1970 * 1000)
        case let value as URL:
            return value.absoluteString
        case let value?:
            if let optional = value as? OptionalProtocol { return optional.isNil ? NSNull() : wrap(optional.unwrapped) }
            return value
        }
    }

    private static func json(_ object: Any) -> String {
        guard JSONSerialization.isValidJSONObject(object),
              let data = try? JSONSerialization.data(withJSONObject: object),
              let text = String(data: data, encoding: .utf8)
        else { return "{}" }
        return text
    }
}

/// Lets `wrap` see through `Any` values that hold a nested `Optional`.
private protocol OptionalProtocol {
    var isNil: Bool { get }
    var unwrapped: Any? { get }
}

extension Optional: OptionalProtocol {
    var isNil: Bool { self == nil }
    var unwrapped: Any? { self.map { $0 as Any } }
}
