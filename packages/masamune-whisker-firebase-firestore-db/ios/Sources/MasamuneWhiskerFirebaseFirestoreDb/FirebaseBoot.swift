// Configures the default Firebase app on first use.
//
// Android initializes Firebase from `google-services.json` before the activity starts
// (FirebaseInitProvider). On iOS nothing configures Firebase until Rust calls
// `whisker_firebase::FirebaseApp::initialize()`, so every entry point of this module calls
// `FirebaseBoot.ensure()` first and configures the app from the bundled
// `GoogleService-Info.plist` the same way whisker-firebase-core does.

import FirebaseCore
import Foundation

enum FirebaseBoot {
    struct MissingConfig: LocalizedError {
        var errorDescription: String? {
            "Missing or invalid GoogleService-Info.plist in the application bundle"
        }
    }

    /// Configures the default app when it does not exist yet.
    static func ensure() throws {
        if FirebaseApp.app() != nil { return }
        if Thread.isMainThread {
            try configure()
        } else {
            try DispatchQueue.main.sync { try configure() }
        }
    }

    private static func configure() throws {
        if FirebaseApp.app() != nil { return }
        guard let path = Bundle.main.path(forResource: "GoogleService-Info", ofType: "plist"),
              let options = FirebaseOptions(contentsOfFile: path)
        else { throw MissingConfig() }
        FirebaseApp.configure(options: options)
    }
}
