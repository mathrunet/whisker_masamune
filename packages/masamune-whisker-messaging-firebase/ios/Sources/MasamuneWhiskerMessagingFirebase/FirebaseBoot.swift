// Configures the default Firebase app on first use.

import FirebaseCore
import Foundation

enum FirebaseBoot {
    struct MissingConfig: LocalizedError {
        var errorDescription: String? {
            "Missing or invalid GoogleService-Info.plist in the application bundle"
        }
    }

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
