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
print("Swift security policies passed")
