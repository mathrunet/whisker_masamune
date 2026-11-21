import Foundation

let cache = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
try FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: cache) }
let good = try captureFileURL(cache: cache, fileName: "capture-1.png")
precondition(good.lastPathComponent == "capture-1.png")
for name in ["", "../secret", "/tmp/secret", "x/y.png", "x\\y.png", "..", ".hidden", "x\0.png", "x\n.png", String(repeating: "a", count: 129)] {
    precondition((try? captureFileURL(cache: cache, fileName: name)) == nil, "accepted unsafe name")
}
let directory = good.deletingLastPathComponent()
try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
try FileManager.default.createSymbolicLink(at: directory.appendingPathComponent("escape.png"), withDestinationURL: cache.appendingPathComponent("private.png"))
precondition((try? captureFileURL(cache: cache, fileName: "escape.png")) == nil)
precondition(!NativeOriginPolicy.allows("https://a.test", allowedOrigins: []))
let allowed = ["https://a.test"]
precondition(NativeOriginPolicy.allows("https://a.test", allowedOrigins: allowed))
precondition(!NativeOriginPolicy.allows("https://a.test", allowedOrigins: allowed, isMainFrame: false))
precondition(!NativeOriginPolicy.allows("https://evil.test", allowedOrigins: allowed))
precondition(!NativeOriginPolicy.allows(nil, allowedOrigins: allowed))
precondition(!NativeOriginPolicy.allows("null", allowedOrigins: allowed))

let suite = "masamune-app-check-test-\(UUID().uuidString)"
let defaults = UserDefaults(suiteName: suite)!
defer { defaults.removePersistentDomain(forName: suite) }
let debugTokenKey = "GACAppCheckDebugToken"
let debugToken = "dummy-token"
let appAttest = try AppCheckProviderPolicy.activate(override: "appAttest", debugToken: debugToken, defaults: defaults)
precondition(appAttest == .appAttest)
precondition(defaults.object(forKey: debugTokenKey) == nil)
#if DEBUG
let auto = try AppCheckProviderPolicy.activate(override: nil, debugToken: "", defaults: defaults)
precondition(auto == .debug)
precondition(defaults.object(forKey: debugTokenKey) == nil)
let debug = try AppCheckProviderPolicy.activate(override: "debug", debugToken: debugToken, defaults: defaults)
precondition(debug == .debug)
precondition(defaults.string(forKey: debugTokenKey) == debugToken)
#else
let auto = try AppCheckProviderPolicy.activate(override: nil, debugToken: debugToken, defaults: defaults)
precondition(auto == .appAttest)
do {
    _ = try AppCheckProviderPolicy.activate(override: "debug", debugToken: debugToken, defaults: defaults)
    preconditionFailure("Release accepted the debug provider")
} catch {
    precondition(error.localizedDescription == "The App Check debug provider requires a Debug build")
}
precondition(defaults.object(forKey: debugTokenKey) == nil)
#endif
print("Swift security policies passed")
