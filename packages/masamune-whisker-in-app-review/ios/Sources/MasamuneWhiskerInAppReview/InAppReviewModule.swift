// In-app review (StoreKit). Opens the store page if no window scene is available.
//
// iOS decides by itself whether the review prompt actually appears (at most three times a year, never
// in TestFlight builds) and does not report it, so a successful request resolves `true` either way.

import StoreKit
import UIKit
import WhiskerModule

@WhiskerModule
public final class InAppReviewModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("InAppReview")

            // requestReview({ storeUrl }) -> true if requested in-app, false if the store was opened
            AsyncFunction("requestReview") { args, promise in
                let storeUrl = (Bridge.args(args)["storeUrl"] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
                Task { @MainActor in
                    if let scene = Self.activeScene() {
                        if #available(iOS 16.0, *) {
                            AppStore.requestReview(in: scene)
                        } else {
                            SKStoreReviewController.requestReview(in: scene)
                        }
                        Bridge.resolve(promise, true)
                        return
                    }
                    if let url = URL(string: storeUrl), url.scheme != nil {
                        UIApplication.shared.open(url)
                    }
                    Bridge.resolve(promise, false)
                }
            }
        }
    }

    /// The foreground window scene the prompt is shown in.
    @MainActor
    private static func activeScene() -> UIWindowScene? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        return scenes.first { $0.activationState == .foregroundActive }
    }
}
