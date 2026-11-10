// Google sign-in. iOS signs in with the Google Sign-In SDK and hands its tokens to Firebase Auth
// (`GoogleAuthProvider.credential(withIDToken:accessToken:)`). Linking to the current (e.g. anonymous)
// user uses the same screen. Mirrors GoogleSignInModule.kt, which uses Firebase's web flow.
//
// The SDK runs in ASWebAuthenticationSession (AppAuth), which receives the redirect itself, so the
// app delegate does not need to forward `application(_:open:options:)` to `GIDSignIn.handle(_:)`.
// The SDK still requires the REVERSED_CLIENT_ID URL scheme in Info.plist (the crate's CNG plugin adds it);
// without it the call fails instead of raising the SDK's exception.

import FirebaseAuth
import FirebaseCore
import Foundation
import GoogleSignIn
import UIKit
import WhiskerModule

@WhiskerModule
public final class GoogleSignInModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("GoogleSignIn")

            // signIn({ scopes? }) -> { uid, isNewUser } / rejects with "cancelled" on cancel
            AsyncFunction("signIn") { args, promise in
                let scopes = Self.scopes(Bridge.args(args))
                Bridge.onMain {
                    Self.authorize(scopes, promise) { credential in
                        Auth.auth().signIn(with: credential) { result, error in
                            Self.complete(result, error, promise)
                        }
                    }
                }
            }

            // link({ scopes? }) -> { uid, isNewUser } — links Google to the current user.
            // Rejects with "already-registered" if the account belongs to another user, or "cancelled" on cancel.
            AsyncFunction("link") { args, promise in
                let scopes = Self.scopes(Bridge.args(args))
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                    } catch {
                        return Bridge.reject(promise, error)
                    }
                    guard Presenter.topViewController() != nil, Auth.auth().currentUser != nil else {
                        return Bridge.reject(promise, Presenter.topViewController() == nil ? "no activity" : "no user")
                    }
                    Self.authorize(scopes, promise) { credential in
                        guard let user = Auth.auth().currentUser else {
                            return Bridge.reject(promise, "no user")
                        }
                        user.link(with: credential) { result, error in
                            Self.complete(result, error, promise)
                        }
                    }
                }
            }
        }
    }

    /// Shows the Google account screen and hands the Firebase credential to `next`.
    private static func authorize(
        _ scopes: [String],
        _ promise: WhiskerPromise,
        _ next: @escaping (AuthCredential) -> Void
    ) {
        do {
            try FirebaseBoot.ensure()
        } catch {
            return Bridge.reject(promise, error)
        }
        guard let presenter = Presenter.topViewController() else {
            return Bridge.reject(promise, "no activity")
        }
        guard let clientID = FirebaseApp.app()?.options.clientID, !clientID.isEmpty else {
            return Bridge.reject(promise, "GoogleService-Info.plist has no CLIENT_ID; enable Google sign-in in Firebase and download it again")
        }
        let scheme = clientID.components(separatedBy: ".").reversed().joined(separator: ".")
        guard urlSchemes().contains(where: { $0.caseInsensitiveCompare(scheme) == .orderedSame }) else {
            return Bridge.reject(promise, "Info.plist has no URL scheme \(scheme); add the masamune-whisker-google-sign-in CNG plugin")
        }
        let signIn = GIDSignIn.sharedInstance
        signIn.configuration = GIDConfiguration(clientID: clientID)
        // `email` and `profile` are part of the basic profile the SDK always requests.
        let additional = scopes.filter { !["email", "profile", "openid"].contains($0) }
        signIn.signIn(withPresenting: presenter, hint: nil, additionalScopes: additional) { result, error in
            if let error {
                if (error as NSError).domain == kGIDSignInErrorDomain,
                   (error as NSError).code == GIDSignInError.canceled.rawValue {
                    return Bridge.reject(promise, "cancelled")
                }
                return Bridge.reject(promise, error)
            }
            guard let user = result?.user, let idToken = user.idToken?.tokenString else {
                return Bridge.reject(promise, "Google sign-in returned no ID token")
            }
            next(GoogleAuthProvider.credential(withIDToken: idToken, accessToken: user.accessToken.tokenString))
        }
    }

    /// URL schemes registered in the app's Info.plist.
    private static func urlSchemes() -> [String] {
        let types = Bundle.main.object(forInfoDictionaryKey: "CFBundleURLTypes") as? [[String: Any]] ?? []
        return types.flatMap { $0["CFBundleURLSchemes"] as? [String] ?? [] }
    }

    private static func scopes(_ args: [String: Any]) -> [String] {
        args["scopes"] as? [String] ?? ["email", "profile"]
    }

    private static func complete(_ result: AuthDataResult?, _ error: Error?, _ promise: WhiskerPromise) {
        if let error {
            let ns = error as NSError
            let code = ns.domain == AuthErrorDomain ? AuthErrorCode(rawValue: ns.code) : nil
            switch code {
            case .credentialAlreadyInUse, .emailAlreadyInUse, .accountExistsWithDifferentCredential:
                Bridge.reject(promise, "already-registered")
            default:
                let message = Bridge.message(error)
                if message.range(of: "canceled", options: .caseInsensitive) != nil
                    || message.range(of: "cancelled", options: .caseInsensitive) != nil {
                    Bridge.reject(promise, "cancelled")
                } else {
                    Bridge.reject(promise, error)
                }
            }
            return
        }
        Bridge.resolve(promise, [
            "uid": result?.user.uid,
            "isNewUser": result?.additionalUserInfo?.isNewUser ?? false,
        ] as [String: Any?])
    }
}
