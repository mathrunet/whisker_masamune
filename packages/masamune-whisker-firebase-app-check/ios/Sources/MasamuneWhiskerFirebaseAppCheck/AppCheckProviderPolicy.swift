import Foundation

enum AppCheckProviderPolicy {
    case appAttest
    #if DEBUG
    case debug
    #endif

    static func activate(override: String?, debugToken: String, defaults: UserDefaults) throws -> Self {
        #if DEBUG
        if override == "appAttest" { return .appAttest }
        if !debugToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            // AppCheckCore reads this default; FIRAAppCheckDebugToken still takes precedence.
            defaults.set(debugToken, forKey: "GACAppCheckDebugToken")
        }
        return .debug
        #else
        if override == "debug" { throw DebugProviderUnavailable() }
        return .appAttest
        #endif
    }

    private struct DebugProviderUnavailable: LocalizedError {
        var errorDescription: String? { "The App Check debug provider requires a Debug build" }
    }
}
