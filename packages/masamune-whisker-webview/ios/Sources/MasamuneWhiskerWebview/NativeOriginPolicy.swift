import Foundation

/// Native privileges are opt-in, and never granted to an iframe or opaque origin.
enum NativeOriginPolicy {
    static func allows(_ origin: String?, allowedOrigins: [String], isMainFrame: Bool = true) -> Bool {
        guard isMainFrame, let origin else { return false }
        return allowedOrigins.contains(origin)
    }
}
