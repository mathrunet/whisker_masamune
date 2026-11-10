// Sign in with Apple. iOS uses the native AuthenticationServices sheet (with a SHA-256 nonce) and signs in to
// Firebase Auth with the resulting Apple ID token. Linking to the current (e.g. anonymous) user uses the same sheet.
// Mirrors AppleSignInModule.kt (which uses Firebase's web flow because Android has no OS-level support).
//
// Requires the `com.apple.developer.applesignin` entitlement (the crate's CNG plugin adds it).

import AuthenticationServices
import CryptoKit
import FirebaseAuth
import FirebaseCore
import Foundation
import UIKit
import WhiskerModule

@WhiskerModule
public final class AppleSignInModule: Module {
    /// Requests in flight (kept alive until they finish).
    private var requests: [ObjectIdentifier: AppleIdRequest] = [:]

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("AppleSignIn")

            // signIn({ scopes? }) -> { uid, isNewUser } / rejects with "cancelled" on cancel
            AsyncFunction("signIn") { [weak self] args, promise in
                let scopes = Self.scopes(Bridge.args(args))
                Bridge.onMain {
                    self?.authorize(scopes, promise) { credential in
                        Auth.auth().signIn(with: credential) { result, error in
                            Self.complete(result, error, promise)
                        }
                    }
                }
            }

            // link({ scopes? }) -> { uid, isNewUser } — links Apple to the current user.
            // Rejects with "already-registered" if the account belongs to another user, or "cancelled" on cancel.
            AsyncFunction("link") { [weak self] args, promise in
                let scopes = Self.scopes(Bridge.args(args))
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                    } catch {
                        return Bridge.reject(promise, error)
                    }
                    guard Presenter.keyWindow() != nil, Auth.auth().currentUser != nil else {
                        return Bridge.reject(promise, Presenter.keyWindow() == nil ? "no activity" : "no user")
                    }
                    self?.authorize(scopes, promise) { credential in
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

    /// Shows the Apple ID sheet and hands the Firebase credential to `next`.
    private func authorize(
        _ scopes: [ASAuthorization.Scope],
        _ promise: WhiskerPromise,
        _ next: @escaping (OAuthCredential) -> Void
    ) {
        do {
            try FirebaseBoot.ensure()
        } catch {
            return Bridge.reject(promise, error)
        }
        guard let window = Presenter.keyWindow() else {
            return Bridge.reject(promise, "no activity")
        }
        let request = AppleIdRequest(window: window, scopes: scopes)
        let key = ObjectIdentifier(request)
        requests[key] = request
        request.start { [weak self] result in
            self?.requests.removeValue(forKey: key)
            switch result {
            case let .success(credential):
                next(credential)
            case let .failure(error):
                if (error as? ASAuthorizationError)?.code == .canceled {
                    Bridge.reject(promise, "cancelled")
                } else {
                    Bridge.reject(promise, error)
                }
            }
        }
    }

    private static func scopes(_ args: [String: Any]) -> [ASAuthorization.Scope] {
        let names = args["scopes"] as? [String] ?? ["email", "name"]
        return names.compactMap { name in
            switch name {
            case "email": return .email
            case "name": return .fullName
            default: return nil
            }
        }
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

/// One Apple ID authorization: the nonce, the controller and its delegate callbacks.
private final class AppleIdRequest: NSObject, ASAuthorizationControllerDelegate,
    ASAuthorizationControllerPresentationContextProviding {
    private let window: UIWindow
    private let scopes: [ASAuthorization.Scope]
    private let nonce = AppleIdRequest.randomNonce()
    private var completion: ((Result<OAuthCredential, Error>) -> Void)?

    init(window: UIWindow, scopes: [ASAuthorization.Scope]) {
        self.window = window
        self.scopes = scopes
    }

    func start(_ completion: @escaping (Result<OAuthCredential, Error>) -> Void) {
        self.completion = completion
        let request = ASAuthorizationAppleIDProvider().createRequest()
        request.requestedScopes = scopes
        request.nonce = Self.sha256(nonce)
        let controller = ASAuthorizationController(authorizationRequests: [request])
        controller.delegate = self
        controller.presentationContextProvider = self
        controller.performRequests()
    }

    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        window
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithAuthorization authorization: ASAuthorization) {
        guard let apple = authorization.credential as? ASAuthorizationAppleIDCredential,
              let tokenData = apple.identityToken,
              let token = String(data: tokenData, encoding: .utf8)
        else {
            return finish(.failure(AppleIdError("Apple ID credential has no identity token")))
        }
        let credential = OAuthProvider.appleCredential(withIDToken: token, rawNonce: nonce, fullName: apple.fullName)
        finish(.success(credential))
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithError error: Error) {
        finish(.failure(error))
    }

    private func finish(_ result: Result<OAuthCredential, Error>) {
        let completion = self.completion
        self.completion = nil
        completion?(result)
    }

    private static func randomNonce(length: Int = 32) -> String {
        let charset = Array("0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-._")
        var bytes = [UInt8](repeating: 0, count: length)
        if SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes) != errSecSuccess {
            bytes = bytes.map { _ in UInt8.random(in: 0 ... 255) }
        }
        return String(bytes.map { charset[Int($0) % charset.count] })
    }

    private static func sha256(_ input: String) -> String {
        SHA256.hash(data: Data(input.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}

private struct AppleIdError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}
