// Firebase Remote Config. Fetches and activates, then returns the values of all keys as strings.

import FirebaseCore
import FirebaseRemoteConfig
import Foundation
import WhiskerModule

@WhiskerModule
public final class FirebaseRemoteConfigModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("FirebaseRemoteConfig")

            // fetchAndActivate({ fetchTimeoutSeconds, minimumFetchIntervalSeconds }) -> { key: stringValue }
            // Even if the fetch fails, returns the cached or default values.
            AsyncFunction("fetchAndActivate") { args, promise in
                let json = Bridge.args(args)
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                        let config = RemoteConfig.remoteConfig()
                        let settings = RemoteConfigSettings()
                        settings.fetchTimeout = Self.seconds(json["fetchTimeoutSeconds"], 60)
                        settings.minimumFetchInterval = Self.seconds(json["minimumFetchIntervalSeconds"], 3600)
                        config.configSettings = settings
                        config.fetchAndActivate { _, error in
                            if let error {
                                NSLog("[MasamuneWhiskerRemoteConfig] remote config fetch failed; using cached values: %@", Bridge.message(error))
                            }
                            Bridge.resolve(promise, Self.values(config))
                        }
                    } catch {
                        Bridge.reject(promise, error)
                    }
                }
            }

            // values() -> currently active values (does not fetch)
            Function("values") { _ in
                do {
                    try FirebaseBoot.ensure()
                    return Bridge.ok(Self.values(RemoteConfig.remoteConfig()))
                } catch {
                    return Bridge.err(error)
                }
            }
        }
    }

    /// Active and default values of every key, like Android's `FirebaseRemoteConfig.getAll()`.
    private static func values(_ config: RemoteConfig) -> [String: String] {
        let keys = Set(config.allKeys(from: .default)).union(config.allKeys(from: .remote))
        var result: [String: String] = [:]
        for key in keys {
            result[key] = config.configValue(forKey: key).stringValue
        }
        return result
    }

    private static func seconds(_ value: Any?, _ fallback: Double) -> TimeInterval {
        (value as? NSNumber)?.doubleValue ?? fallback
    }
}
