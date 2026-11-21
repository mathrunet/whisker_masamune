// Location permission, single fixes and the position stream (iOS, CLLocationManager).
//
// - The permission is read from a manager owned by the module. A request resolves on
//   `locationManagerDidChangeAuthorization`, or when the app becomes active again after the prompt
//   (the user may keep the current choice, which reports no change). When no prompt appears (the
//   decision was made before), the request resolves at once.
// - Each `currentPosition` call uses its own manager so it never changes the stream's settings.
// - The stream (`startUpdates` / `stopUpdates`) sends `update` events: `{type: "position", ...}` or
//   `{type: "error", code, message}` (after which it stops).

import CoreLocation
import Foundation
import UIKit
import WhiskerModule

@WhiskerModule
public final class LocationModule: Module {
    private let permission = PermissionRequester()
    private var stream: StreamDelegate?
    private var oneShots: Set<OneShot> = []

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Location")
            Events("update")

            Function("status") { [weak self] _ in
                Bridge.ok(self?.permission.status() ?? "denied")
            }

            AsyncFunction("requestPermission") { [weak self] _, promise in
                Bridge.onMain {
                    guard let self else { return Bridge.resolve(promise, "denied") }
                    self.permission.request(always: false) { Bridge.resolve(promise, $0) }
                }
            }

            AsyncFunction("requestBackgroundPermission") { [weak self] _, promise in
                Bridge.onMain {
                    guard let self else { return Bridge.resolve(promise, "denied") }
                    self.permission.request(always: true) { Bridge.resolve(promise, $0) }
                }
            }

            Function("precise") { [weak self] _ in
                Bridge.ok(self?.permission.manager.accuracyAuthorization == .fullAccuracy)
            }

            Function("serviceEnabled") { _ in
                Bridge.ok(CLLocationManager.locationServicesEnabled())
            }

            Function("openAppSettings") { _ in
                Self.openSettings()
            }

            // iOS has no public URL for the device location settings.
            Function("openLocationSettings") { _ in
                Self.openSettings()
            }

            // currentPosition({ accuracy, timeoutMs }) -> position
            AsyncFunction("currentPosition") { [weak self] args, promise in
                let json = Bridge.args(args)
                Bridge.onMain {
                    guard let self else { return Bridge.reject(promise, "location module is gone") }
                    if let error = Self.unavailable() { return Bridge.reject(promise, error.message) }
                    let timeout = ((json["timeoutMs"] as? NSNumber)?.doubleValue ?? 10_000) / 1000
                    let shot = OneShot(accuracy: Self.accuracy(json["accuracy"] as? String))
                    self.oneShots.insert(shot)
                    shot.start(timeout: timeout) { [weak self, weak shot] result in
                        if let shot { self?.oneShots.remove(shot) }
                        switch result {
                        case .success(let location): Bridge.resolve(promise, Self.payload(location))
                        case .failure(let error): Bridge.reject(promise, error.message)
                        }
                    }
                }
            }

            // lastKnownPosition() -> position | null
            AsyncFunction("lastKnownPosition") { [weak self] _, promise in
                Bridge.onMain {
                    guard let self, Self.unavailable() == nil, let location = self.permission.manager.location else {
                        return Bridge.resolve(promise, nil)
                    }
                    Bridge.resolve(promise, Self.payload(location))
                }
            }

            // startUpdates({ accuracy, distanceFilter, intervalMs, background }) — restarts with new settings.
            Function("startUpdates") { [weak self] args in
                guard let self else { return Bridge.ok(false) }
                let json = Bridge.args(args)
                Bridge.onMain { self.startUpdates(json) }
                return Bridge.ok(true)
            }

            Function("stopUpdates") { [weak self] _ in
                Bridge.onMain {
                    self?.stream?.stop()
                    self?.stream = nil
                }
                return Bridge.ok(true)
            }
        }
    }

    private func startUpdates(_ json: [String: Any]) {
        stream?.stop()
        stream = nil
        if let error = Self.unavailable() {
            return send(error.payload)
        }
        let background = json["background"] as? Bool ?? false
        if background, !Self.hasBackgroundMode() {
            return send(LocationFailure.backgroundUnavailable.payload)
        }
        let delegate = StreamDelegate(
            accuracy: Self.accuracy(json["accuracy"] as? String),
            distanceFilter: (json["distanceFilter"] as? NSNumber)?.doubleValue ?? 0,
            background: background
        ) { [weak self] payload, failed in
            self?.send(payload)
            if failed {
                self?.stream?.stop()
                self?.stream = nil
            }
        }
        stream = delegate
        delegate.start()
    }

    private func send(_ payload: [String: Any?]) {
        Bridge.onMain { [weak self] in self?.sendEvent("update", Bridge.event(payload)) }
    }

    // MARK: - Helpers

    fileprivate static func unavailable() -> LocationFailure? {
        if !CLLocationManager.locationServicesEnabled() { return .serviceDisabled }
        switch CLLocationManager().authorizationStatus {
        case .authorizedAlways, .authorizedWhenInUse: return nil
        default: return .permissionDenied
        }
    }

    private static func hasBackgroundMode() -> Bool {
        let modes = Bundle.main.object(forInfoDictionaryKey: "UIBackgroundModes") as? [String] ?? []
        return modes.contains("location")
    }

    fileprivate static func accuracy(_ value: String?) -> CLLocationAccuracy {
        switch value {
        case "lowest": return kCLLocationAccuracyThreeKilometers
        case "low": return kCLLocationAccuracyKilometer
        case "medium": return kCLLocationAccuracyHundredMeters
        case "best": return kCLLocationAccuracyBest
        default: return kCLLocationAccuracyNearestTenMeters
        }
    }

    fileprivate static func payload(_ location: CLLocation, type: String? = nil) -> [String: Any?] {
        var payload: [String: Any?] = [
            "latitude": location.coordinate.latitude,
            "longitude": location.coordinate.longitude,
            "accuracy": location.horizontalAccuracy,
            "altitude": location.altitude,
            "speed": location.speed,
            "heading": location.course,
            "timestampMs": Int64(location.timestamp.timeIntervalSince1970 * 1000),
        ]
        if let type { payload["type"] = type }
        return payload
    }

    private static func openSettings() -> WhiskerValue {
        guard let url = URL(string: UIApplication.openSettingsURLString) else { return Bridge.ok(false) }
        Bridge.onMain { UIApplication.shared.open(url) }
        return Bridge.ok(true)
    }
}

// MARK: - Errors

fileprivate enum LocationFailure: Error {
    case permissionDenied, serviceDisabled, backgroundUnavailable, timeout, failed(String)

    var code: String {
        switch self {
        case .permissionDenied: return "permissionDenied"
        case .serviceDisabled: return "serviceDisabled"
        case .backgroundUnavailable: return "backgroundUnavailable"
        case .timeout, .failed: return "failed"
        }
    }

    var message: String {
        switch self {
        case .permissionDenied: return "location permission denied"
        case .serviceDisabled: return "location services are disabled"
        case .backgroundUnavailable: return "background location needs `location` in UIBackgroundModes"
        case .timeout: return "location timeout"
        case .failed(let message): return message
        }
    }

    var payload: [String: Any?] {
        ["type": "error", "code": code, "message": message]
    }
}

// MARK: - Permission

private final class PermissionRequester: NSObject, CLLocationManagerDelegate {
    let manager = CLLocationManager()
    private var pending: [(String) -> Void] = []
    private var observers: [NSObjectProtocol] = []
    private var promptShown = false

    override init() {
        super.init()
        manager.delegate = self
    }

    func status() -> String {
        switch manager.authorizationStatus {
        case .notDetermined: return "notDetermined"
        case .restricted: return "restricted"
        case .denied: return "denied"
        case .authorizedAlways: return "always"
        case .authorizedWhenInUse: return "whenInUse"
        @unknown default: return "denied"
        }
    }

    func request(always: Bool, completion: @escaping (String) -> Void) {
        let current = manager.authorizationStatus
        let canPrompt = current == .notDetermined || (always && current == .authorizedWhenInUse)
        guard canPrompt else { return completion(status()) }
        pending.append(completion)
        guard pending.count == 1 else { return }
        promptShown = false
        let center = NotificationCenter.default
        observers.append(center.addObserver(forName: UIApplication.willResignActiveNotification, object: nil, queue: .main) { [weak self] _ in
            self?.promptShown = true
        })
        observers.append(center.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { [weak self] _ in
            guard let self, self.promptShown else { return }
            self.finish()
        })
        if always {
            manager.requestAlwaysAuthorization()
        } else {
            manager.requestWhenInUseAuthorization()
        }
        // No prompt (asked before): the app never resigns active.
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
            guard let self, !self.promptShown, !self.pending.isEmpty else { return }
            self.finish()
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        guard manager.authorizationStatus != .notDetermined else { return }
        finish()
    }

    private func finish() {
        guard !pending.isEmpty else { return }
        observers.forEach(NotificationCenter.default.removeObserver)
        observers.removeAll()
        let callbacks = pending
        pending.removeAll()
        let value = status()
        callbacks.forEach { $0(value) }
    }
}

// MARK: - Single fix

private final class OneShot: NSObject, CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    private var completion: ((Result<CLLocation, LocationFailure>) -> Void)?

    init(accuracy: CLLocationAccuracy) {
        super.init()
        manager.desiredAccuracy = accuracy
        manager.delegate = self
    }

    func start(timeout: TimeInterval, completion: @escaping (Result<CLLocation, LocationFailure>) -> Void) {
        self.completion = completion
        manager.requestLocation()
        DispatchQueue.main.asyncAfter(deadline: .now() + max(timeout, 0.1)) { [weak self] in
            self?.finish(.failure(.timeout))
        }
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let location = locations.last else { return }
        finish(.success(location))
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        // `locationUnknown` is transient; requestLocation keeps trying until the timeout.
        if (error as? CLError)?.code == .locationUnknown { return }
        if (error as? CLError)?.code == .denied { return finish(.failure(.permissionDenied)) }
        finish(.failure(.failed(Bridge.message(error))))
    }

    private func finish(_ result: Result<CLLocation, LocationFailure>) {
        guard let completion else { return }
        self.completion = nil
        manager.stopUpdatingLocation()
        manager.delegate = nil
        completion(result)
    }
}

// MARK: - Stream

private final class StreamDelegate: NSObject, CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    private let background: Bool
    private let emit: ([String: Any?], Bool) -> Void

    init(accuracy: CLLocationAccuracy, distanceFilter: Double, background: Bool, emit: @escaping ([String: Any?], Bool) -> Void) {
        self.background = background
        self.emit = emit
        super.init()
        manager.desiredAccuracy = accuracy
        manager.distanceFilter = distanceFilter > 0 ? distanceFilter : kCLDistanceFilterNone
        manager.pausesLocationUpdatesAutomatically = false
        manager.activityType = .other
        manager.delegate = self
    }

    func start() {
        if background {
            manager.allowsBackgroundLocationUpdates = true
            manager.showsBackgroundLocationIndicator = true
        }
        manager.startUpdatingLocation()
    }

    func stop() {
        manager.stopUpdatingLocation()
        if background { manager.allowsBackgroundLocationUpdates = false }
        manager.delegate = nil
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        for location in locations {
            emit(LocationModule.payload(location, type: "position"), false)
        }
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        switch (error as? CLError)?.code {
        case .locationUnknown?:
            return
        case .denied?:
            emit(LocationFailure.permissionDenied.payload, true)
        default:
            emit(LocationFailure.failed(Bridge.message(error)).payload, true)
        }
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        switch manager.authorizationStatus {
        case .denied, .restricted: emit(LocationFailure.permissionDenied.payload, true)
        default: break
        }
    }
}
