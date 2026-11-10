// Auth0 Universal Login and credentials storage with Auth0.swift. Mirrors Auth0Module.kt.
//
// Login opens Universal Login in ASWebAuthenticationSession with the callback
// `{scheme}://{domain}/ios/{bundle id}/callback` and stores the credentials in the Keychain
// (CredentialsManager). Rejections are `cancelled`, `no-credentials` or `<code>: <message>`.
//
// Every call receives the account as JSON: { domain, clientId, scheme, scope, prompt?, minTtl? }.
// Credentials are returned as
// { accessToken, idToken, refreshToken?, expiresAtMs, scope?, user: { sub, email?, name? } }.

import Auth0
import Foundation
import WhiskerModule

@WhiskerModule
public final class Auth0Module: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Auth0")

            // login({ ..., prompt? }) -> credentials / rejects with "cancelled"
            AsyncFunction("login") { args, promise in
                let args = Bridge.args(args)
                Bridge.onMain {
                    guard let account = Account(args) else {
                        return Bridge.reject(promise, "invalid-config: domain, clientId and scheme are required")
                    }
                    guard let redirect = account.redirectURL() else {
                        return Bridge.reject(promise, "invalid-config: the callback URL cannot be built")
                    }
                    var parameters: [String: String] = [:]
                    if let prompt = args["prompt"] as? String, !prompt.isEmpty {
                        parameters["prompt"] = prompt
                    }
                    Auth0
                        .webAuth(clientId: account.clientId, domain: account.domain)
                        .scope(account.scope)
                        .parameters(parameters)
                        .redirectURL(redirect)
                        .start { result in
                            switch result {
                            case let .success(credentials):
                                _ = Self.manager(account).store(credentials: credentials)
                                Bridge.resolve(promise, Self.encode(credentials))
                            case let .failure(error):
                                if error == .userCancelled {
                                    Bridge.reject(promise, "cancelled")
                                } else {
                                    Bridge.reject(promise, Self.message(error))
                                }
                            }
                        }
                }
            }

            // logout({ ... }) -> null. Clears the browser session, then the stored credentials.
            AsyncFunction("logout") { args, promise in
                let args = Bridge.args(args)
                Bridge.onMain {
                    guard let account = Account(args) else {
                        return Bridge.reject(promise, "invalid-config: domain, clientId and scheme are required")
                    }
                    var webAuth = Auth0.webAuth(clientId: account.clientId, domain: account.domain)
                    if let redirect = account.redirectURL() {
                        webAuth = webAuth.redirectURL(redirect)
                    }
                    webAuth.clearSession(federated: false) { result in
                        _ = Self.manager(account).clear()
                        switch result {
                        case .success:
                            Bridge.resolve(promise, nil)
                        case let .failure(error):
                            if error == .userCancelled {
                                Bridge.reject(promise, "cancelled")
                            } else {
                                Bridge.reject(promise, Self.message(error))
                            }
                        }
                    }
                }
            }

            // credentials({ ..., minTtl }) -> credentials (renewed and stored when needed)
            // / rejects with "no-credentials" when nothing usable is stored.
            AsyncFunction("credentials") { args, promise in
                let args = Bridge.args(args)
                guard let account = Account(args) else {
                    return Bridge.reject(promise, "invalid-config: domain, clientId and scheme are required")
                }
                let minTTL = args["minTtl"] as? Int ?? 0
                let manager = Self.manager(account)
                guard manager.canRenew() || manager.hasValid(minTTL: minTTL) else {
                    return Bridge.reject(promise, "no-credentials")
                }
                manager.credentials(withScope: account.scope, minTTL: minTTL) { result in
                    switch result {
                    case let .success(credentials):
                        Bridge.resolve(promise, Self.encode(credentials))
                    case let .failure(error):
                        if error == .noCredentials {
                            Bridge.reject(promise, "no-credentials")
                        } else {
                            Bridge.reject(promise, Self.message(error))
                        }
                    }
                }
            }

            // hasValidCredentials({ ..., minTtl }) -> Bool (valid for minTtl or renewable)
            AsyncFunction("hasValidCredentials") { args, promise in
                let args = Bridge.args(args)
                guard let account = Account(args) else {
                    return Bridge.reject(promise, "invalid-config: domain, clientId and scheme are required")
                }
                let manager = Self.manager(account)
                let minTTL = args["minTtl"] as? Int ?? 0
                Bridge.resolve(promise, manager.canRenew() || manager.hasValid(minTTL: minTTL))
            }

            // clear({ ... }) -> null. Removes the stored credentials only.
            AsyncFunction("clear") { args, promise in
                let args = Bridge.args(args)
                guard let account = Account(args) else {
                    return Bridge.reject(promise, "invalid-config: domain, clientId and scheme are required")
                }
                _ = Self.manager(account).clear()
                Bridge.resolve(promise, nil)
            }
        }
    }

    private struct Account: Hashable {
        let domain: String
        let clientId: String
        let scheme: String
        let scope: String

        init?(_ args: [String: Any]) {
            guard let domain = args["domain"] as? String, !domain.isEmpty,
                  let clientId = args["clientId"] as? String, !clientId.isEmpty,
                  let scheme = args["scheme"] as? String, !scheme.isEmpty
            else { return nil }
            self.domain = domain
            self.clientId = clientId
            self.scheme = scheme
            self.scope = args["scope"] as? String ?? "openid profile email offline_access"
        }

        /// `{scheme}://{domain}/ios/{bundle id}/callback`, as registered in the Auth0 application.
        func redirectURL() -> URL? {
            guard let bundleId = Bundle.main.bundleIdentifier else { return nil }
            return URL(string: "\(scheme)://\(domain)/ios/\(bundleId)/callback")
        }
    }

    private static let lock = NSLock()
    private static var managers: [Account: CredentialsManager] = [:]

    /// One CredentialsManager per account, so concurrent renewals are serialized by the SDK.
    private static func manager(_ account: Account) -> CredentialsManager {
        lock.lock()
        defer { lock.unlock() }
        if let manager = managers[account] { return manager }
        let manager = CredentialsManager(
            authentication: Auth0.authentication(clientId: account.clientId, domain: account.domain)
        )
        managers[account] = manager
        return manager
    }

    private static func message(_ error: Error) -> String {
        if let error = error as? Auth0Error {
            let cause = error.cause.map { " (\(Bridge.message($0)))" } ?? ""
            return "\(String(describing: type(of: error))): \(error.debugDescription)\(cause)"
        }
        return "native: \(Bridge.message(error))"
    }

    private static func encode(_ credentials: Credentials) -> [String: Any?] {
        let claims = Self.claims(credentials.idToken)
        return [
            "accessToken": credentials.accessToken,
            "idToken": credentials.idToken,
            "refreshToken": credentials.refreshToken,
            "expiresAtMs": Int64(credentials.expiresIn.timeIntervalSince1970 * 1000),
            "scope": credentials.scope,
            "user": [
                "sub": claims["sub"] as? String ?? "",
                "email": claims["email"] as? String,
                "name": claims["name"] as? String,
            ] as [String: Any?],
        ]
    }

    /// The payload of a JWT (unverified; the SDK has already validated the ID token).
    private static func claims(_ jwt: String) -> [String: Any] {
        let parts = jwt.split(separator: ".")
        guard parts.count >= 2 else { return [:] }
        var base64 = String(parts[1])
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        while base64.count % 4 != 0 { base64 += "=" }
        guard let data = Data(base64Encoded: base64),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return object
    }
}
