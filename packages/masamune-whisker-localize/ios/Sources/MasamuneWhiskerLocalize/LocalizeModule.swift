// Returns the preferred languages and reports their changes as events.

import Foundation
import WhiskerModule

@WhiskerModule
public final class LocalizeModule: Module {
    private var token: NSObjectProtocol?

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Localize")
            Events("languagesChanged")

            OnStartObserving("languagesChanged") { [weak self] in
                Bridge.onMain {
                    guard let self else { return }
                    if self.token == nil {
                        self.token = NotificationCenter.default.addObserver(
                            forName: NSLocale.currentLocaleDidChangeNotification, object: nil, queue: .main
                        ) { [weak self] _ in
                            self?.send()
                        }
                    }
                    self.send()
                }
            }
            OnStopObserving("languagesChanged") { [weak self] in
                Bridge.onMain {
                    if let token = self?.token {
                        NotificationCenter.default.removeObserver(token)
                    }
                    self?.token = nil
                }
            }

            // languages() -> [{language, country}], most preferred first
            Function("languages") { _ in
                Bridge.ok(LocalizeModule.locales())
            }
        }
    }

    private func send() {
        sendEvent("languagesChanged", Bridge.event(["locales": LocalizeModule.locales()]))
    }

    /// Preferred languages as `{language, country}` (for example `ja-JP`, `zh-Hans-CN`).
    static func locales() -> [[String: Any]] {
        Locale.preferredLanguages.map { identifier in
            let locale = Locale(identifier: identifier)
            return [
                "language": locale.languageCode ?? identifier.components(separatedBy: "-").first ?? identifier,
                "country": locale.regionCode ?? "",
            ]
        }
    }
}
