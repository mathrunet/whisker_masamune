// Motion activity recognition (iOS, CMMotionActivityManager).
//
// iOS has no explicit permission request: querying past activity shows the Motion & Fitness
// prompt, so `requestPermission` runs a one-minute query and reports the status afterwards.
// `startUpdates` streams `update` events: `{type: "activity", kind, confidence, timestampMs}` or
// `{type: "error", message}`.

import CoreMotion
import Foundation
import UIKit
import WhiskerModule

@WhiskerModule
public final class ActivityModule: Module {
    private let manager = CMMotionActivityManager()
    private let queue: OperationQueue = {
        let queue = OperationQueue()
        queue.name = "net.mathru.whisker.activity"
        queue.maxConcurrentOperationCount = 1
        return queue
    }()
    private var running = false

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Activity")
            Events("update")

            Function("status") { _ in
                Bridge.ok(Self.status())
            }

            AsyncFunction("requestPermission") { [weak self] _, promise in
                guard let self, CMMotionActivityManager.isActivityAvailable() else {
                    return Bridge.resolve(promise, Self.status())
                }
                guard CMMotionActivityManager.authorizationStatus() == .notDetermined else {
                    return Bridge.resolve(promise, Self.status())
                }
                let now = Date()
                self.manager.queryActivityStarting(from: now.addingTimeInterval(-60), to: now, to: self.queue) { _, _ in
                    Bridge.resolve(promise, Self.status())
                }
            }

            Function("openAppSettings") { _ in
                guard let url = URL(string: UIApplication.openSettingsURLString) else { return Bridge.ok(false) }
                Bridge.onMain { UIApplication.shared.open(url) }
                return Bridge.ok(true)
            }

            Function("startUpdates") { [weak self] _ in
                Bridge.onMain { self?.start() }
                return Bridge.ok(true)
            }

            Function("stopUpdates") { [weak self] _ in
                Bridge.onMain { self?.stop() }
                return Bridge.ok(true)
            }
        }
    }

    private static func status() -> String {
        guard CMMotionActivityManager.isActivityAvailable() else { return "unsupported" }
        switch CMMotionActivityManager.authorizationStatus() {
        case .notDetermined: return "notDetermined"
        case .restricted: return "restricted"
        case .denied: return "denied"
        case .authorized: return "granted"
        @unknown default: return "denied"
        }
    }

    private func start() {
        stop()
        guard CMMotionActivityManager.isActivityAvailable() else {
            return send(["type": "error", "message": "motion activity is not available on this device"])
        }
        switch CMMotionActivityManager.authorizationStatus() {
        case .denied, .restricted:
            return send(["type": "error", "message": "motion activity permission denied"])
        default:
            break
        }
        running = true
        manager.startActivityUpdates(to: queue) { [weak self] activity in
            guard let self, let activity, let kind = Self.kind(activity) else { return }
            let payload: [String: Any?] = [
                "type": "activity",
                "kind": kind,
                "confidence": Self.confidence(activity.confidence),
                "timestampMs": Int64(activity.startDate.timeIntervalSince1970 * 1000),
            ]
            Bridge.onMain {
                guard self.running else { return }
                self.send(payload)
            }
        }
    }

    private func stop() {
        guard running else { return }
        running = false
        manager.stopActivityUpdates()
    }

    private func send(_ payload: [String: Any?]) {
        sendEvent("update", Bridge.event(payload))
    }

    /// The most specific activity, or nil when no flag is set.
    private static func kind(_ activity: CMMotionActivity) -> String? {
        if activity.automotive { return "automotive" }
        if activity.cycling { return "cycling" }
        if activity.running { return "running" }
        if activity.walking { return "walking" }
        if activity.stationary { return "stationary" }
        if activity.unknown { return "unknown" }
        return nil
    }

    private static func confidence(_ confidence: CMMotionActivityConfidence) -> Double {
        switch confidence {
        case .low: return 0.33
        case .medium: return 0.66
        case .high: return 1.0
        @unknown default: return 0.33
        }
    }
}
