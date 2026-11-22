// Keeps the screen awake (iOS) with `UIApplication.shared.isIdleTimerDisabled`.
//
// The flag is written on the main thread. The requested state is kept here as well so
// `isEnabled` can answer from any thread without waiting for the main thread.

import Foundation
import UIKit
import WhiskerModule

@WhiskerModule
public final class WakelockModule: Module {
    private var enabled = false

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Wakelock")

            // setEnabled({ enabled }) -> the new state
            Function("setEnabled") { [weak self] args in
                let enabled = Bridge.args(args)["enabled"] as? Bool ?? false
                self?.enabled = enabled
                Bridge.onMain {
                    MainActor.assumeIsolated {
                        UIApplication.shared.isIdleTimerDisabled = enabled
                    }
                }
                return Bridge.ok(enabled)
            }

            // isEnabled() -> whether the idle timer is disabled
            Function("isEnabled") { [weak self] _ in
                if Thread.isMainThread {
                    let disabled = MainActor.assumeIsolated { UIApplication.shared.isIdleTimerDisabled }
                    return Bridge.ok(disabled)
                }
                return Bridge.ok(self?.enabled ?? false)
            }
        }
    }
}
