// Firebase App Check. Release builds use App Attest; debug builds use the debug provider.
//
// iOS only accepts a provider factory BEFORE `FirebaseApp.configure()`. Whisker evaluates every
// module's `definition()` from `WhiskerModuleBehaviors.registerAll()` in
// `application(_:didFinishLaunchingWithOptions:)`, before the Rust app starts, and Firebase is
// configured only later (by `whisker_firebase::FirebaseApp::initialize()` or the first call into
// a masamune Firebase module). So `definition()` installs a gated factory, and `activate` opens
// the gate, which mirrors Android where no provider exists until `activate` installs one.

import FirebaseAppCheck
import FirebaseCore
import Foundation
import WhiskerModule

@WhiskerModule
public final class FirebaseAppCheckModule: Module {
    public override func definition() -> ModuleDefinition {
        AppCheckGate.install()
        return ModuleDefinition {
            Name("FirebaseAppCheck")

            // activate({ debugToken? }) — no-op after the first call.
            Function("activate") { args in
                do {
                    try AppCheckGate.activate(debugToken: Bridge.args(args)["debugToken"] as? String ?? "")
                    return Bridge.ok(true)
                } catch {
                    return Bridge.err(error)
                }
            }

            // token({ forceRefresh }) -> token string
            AsyncFunction("token") { args, promise in
                let force = Bridge.args(args)["forceRefresh"] as? Bool ?? false
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                        AppCheck.appCheck().token(forcingRefresh: force) { token, error in
                            if let error { return Bridge.reject(promise, error) }
                            guard let token else { return Bridge.reject(promise, "no App Check token") }
                            Bridge.resolve(promise, token.token)
                        }
                    } catch {
                        Bridge.reject(promise, error)
                    }
                }
            }
        }
    }
}

/// Owns the provider factory and the activation state.
enum AppCheckGate {
    /// Optional Info.plist override written by the crate's CNG plugin: `debug` or `appAttest`.
    /// When absent, debug builds of this package use the debug provider and release builds App Attest.
    static let providerInfoKey = "MasamuneFirebaseAppCheckProvider"
    /// The key the App Check debug provider reads its fixed debug token from (AppCheckCore).
    static let debugTokenDefaultsKey = "GACAppCheckDebugToken"

    private static let lock = NSLock()
    private static var installed = false
    private static var activated = false

    /// Installs the gated factory while Firebase is not configured yet.
    static func install() {
        lock.lock()
        defer { lock.unlock() }
        guard !installed else { return }
        guard FirebaseApp.app() == nil else {
            NSLog("[MasamuneWhiskerAppCheck] FirebaseApp was configured before App Check could install its provider factory; App Check stays disabled")
            return
        }
        AppCheck.setAppCheckProviderFactory(GatedProviderFactory())
        installed = true
    }

    static var isActivated: Bool {
        lock.lock()
        defer { lock.unlock() }
        return activated
    }

    static var usesDebugProvider: Bool {
        switch Bundle.main.object(forInfoDictionaryKey: providerInfoKey) as? String {
        case "debug": return true
        case "appAttest": return false
        default:
            #if DEBUG
            return true
            #else
            return false
            #endif
        }
    }

    static func activate(debugToken: String) throws {
        try open(debugToken: debugToken)
        // Configure Firebase now so App Check starts refreshing tokens, like Android after activation.
        // Outside the lock: configuring may ask the provider for a token right away.
        try FirebaseBoot.ensure()
    }

    private static func open(debugToken: String) throws {
        lock.lock()
        defer { lock.unlock() }
        if activated { return }
        guard installed else {
            throw GateError("App Check could not install its provider factory because FirebaseApp was configured first")
        }
        if usesDebugProvider, !debugToken.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            // The debug provider reads the stored token on every request, so a fixed token set here
            // is used from the first exchange on (the `FIRAAppCheckDebugToken` environment variable
            // still wins, as in the Firebase SDK).
            UserDefaults.standard.set(debugToken, forKey: debugTokenDefaultsKey)
        }
        activated = true
    }

    struct GateError: LocalizedError {
        let message: String
        init(_ message: String) { self.message = message }
        var errorDescription: String? { message }
    }
}

/// Creates a provider that stays closed until `activate` is called.
private final class GatedProviderFactory: NSObject, AppCheckProviderFactory {
    func createProvider(with app: FirebaseApp) -> AppCheckProvider? {
        GatedProvider(app: app)
    }
}

/// Delegates to the debug or App Attest provider once activated; fails before that.
private final class GatedProvider: NSObject, AppCheckProvider {
    private let app: FirebaseApp
    private let lock = NSLock()
    private var delegate: AppCheckProvider?

    init(app: FirebaseApp) {
        self.app = app
    }

    func getToken(completion handler: @escaping (AppCheckToken?, Error?) -> Void) {
        do {
            try provider().getToken(completion: handler)
        } catch {
            handler(nil, error)
        }
    }

    func getLimitedUseToken(completion handler: @escaping (AppCheckToken?, Error?) -> Void) {
        do {
            let provider = try provider()
            if provider.responds(to: #selector(AppCheckProvider.getLimitedUseToken(completion:))) {
                provider.getLimitedUseToken?(completion: handler)
            } else {
                provider.getToken(completion: handler)
            }
        } catch {
            handler(nil, error)
        }
    }

    /// Creates the real provider lazily, after `activate` stored the debug token.
    private func provider() throws -> AppCheckProvider {
        guard AppCheckGate.isActivated else {
            throw AppCheckGate.GateError("App Check is not activated; call activate() first")
        }
        lock.lock()
        defer { lock.unlock() }
        if let delegate { return delegate }
        let created: AppCheckProvider?
        if AppCheckGate.usesDebugProvider {
            created = AppCheckDebugProvider(app: app)
        } else {
            created = AppAttestProvider(app: app)
        }
        guard let created else {
            throw AppCheckGate.GateError("App Check provider is unavailable (check the Firebase options and the OS version)")
        }
        delegate = created
        return created
    }
}
