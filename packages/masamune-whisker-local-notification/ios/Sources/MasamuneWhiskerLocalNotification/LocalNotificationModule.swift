// Daily and one-off local notifications (UNUserNotificationCenter with calendar triggers) and
// notification taps. Schedules are saved in UserDefaults so `schedule(id)` returns them like on
// Android; iOS keeps pending requests across reboots and app updates by itself. One-off schedules
// are dropped from UserDefaults once their time has passed.
//
// Like Android, notifications are also shown while the app is in the foreground: a notification
// center delegate presents this module's notifications, reports taps on them, and forwards
// everything else to the delegate that was installed before it (for example Firebase Messaging).
// Firebase Messaging sets its delegate when it is registered, possibly after this module, so the
// delegate is installed again once launching finishes (before the launch tap response is
// delivered), when a scene connects and whenever the app becomes active.
//
// Taps: the tap that launched the app is kept for `getInitialTap`; later taps are sent as the
// `tap` event, queued while nobody observes it.
//
// Android notification channels and small icons have no iOS counterpart, so `configure` stores
// nothing and the app icon is used.

import UIKit
import UserNotifications
import WhiskerModule

@WhiskerModule
public final class LocalNotificationModule: Module {
    private var observers: [NSObjectProtocol] = []

    public override func definition() -> ModuleDefinition {
        // Modules are installed from didFinishLaunching: install the delegate now, again right
        // after launching (other modules may replace it meanwhile), and refresh the cached
        // permission every time the app becomes active (the user may change it in Settings).
        TapStore.shared.module = self
        LocalNotifications.installDelegate()
        LocalNotifications.refreshStatus()
        DispatchQueue.main.async { LocalNotifications.installDelegate() }
        let center = NotificationCenter.default
        observers = [
            center.addObserver(forName: UIApplication.didFinishLaunchingNotification, object: nil, queue: .main) { _ in
                LocalNotifications.installDelegate()
            },
            center.addObserver(forName: UIScene.willConnectNotification, object: nil, queue: .main) { _ in
                LocalNotifications.installDelegate()
            },
            center.addObserver(forName: UIApplication.willEnterForegroundNotification, object: nil, queue: .main) { _ in
                LocalNotifications.installDelegate()
            },
            center.addObserver(forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main) { _ in
                LocalNotifications.installDelegate()
                LocalNotifications.refreshStatus()
                TapStore.shared.becameActive()
            },
        ]

        return ModuleDefinition {
            Name("LocalNotification")
            Events("tap")

            OnStartObserving("tap") {
                Bridge.onMain { TapStore.shared.startObserving() }
            }
            OnStopObserving("tap") {
                Bridge.onMain { TapStore.shared.observing = false }
            }

            // configure({ channelId, channelName, channelDescription, smallIcon }) — Android only.
            Function("configure") { _ in
                LocalNotifications.installDelegate()
                return Bridge.ok(true)
            }

            // status() -> "authorized" | "denied" | "notDetermined" (the cached authorization)
            Function("status") { _ in
                Bridge.ok(LocalNotifications.status())
            }

            // requestPermission() -> whether permission was granted
            AsyncFunction("requestPermission") { _, promise in
                UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in
                    LocalNotifications.refreshStatus { authorized in
                        Bridge.resolve(promise, authorized)
                    }
                }
            }

            // scheduleDaily({ id, hour, minute, title, text, payload }) -> next fire time (ms)
            Function("scheduleDaily") { args in
                var json = Bridge.args(args)
                guard let id = json["id"] as? String else { return Bridge.err("No value for id") }
                guard let hour = (json["hour"] as? NSNumber)?.intValue else { return Bridge.err("No value for hour") }
                guard let minute = (json["minute"] as? NSNumber)?.intValue else { return Bridge.err("No value for minute") }
                json["once"] = false
                json["atMs"] = nil
                LocalNotifications.installDelegate()
                LocalNotifications.save(id: id, schedule: json)
                LocalNotifications.arm(id: id, schedule: json)
                return Bridge.ok(LocalNotifications.nextOccurrence(hour: hour, minute: minute))
            }

            // scheduleOnce({ id, atMs, title, text, payload }) -> fire time (ms). Fails for past times.
            Function("scheduleOnce") { args in
                var json = Bridge.args(args)
                guard let id = json["id"] as? String else { return Bridge.err("No value for id") }
                guard let atMs = (json["atMs"] as? NSNumber)?.int64Value else { return Bridge.err("No value for atMs") }
                let date = Date(timeIntervalSince1970: TimeInterval(atMs) / 1000)
                guard date > Date() else { return Bridge.err("at_ms is in the past") }
                let calendar = Calendar.current
                json["once"] = true
                json["hour"] = calendar.component(.hour, from: date)
                json["minute"] = calendar.component(.minute, from: date)
                LocalNotifications.installDelegate()
                LocalNotifications.save(id: id, schedule: json)
                LocalNotifications.arm(id: id, schedule: json)
                return Bridge.ok(atMs)
            }

            // schedule({ id }) -> saved schedule (null if missing or a one-off that has fired)
            Function("schedule") { args in
                let id = Bridge.args(args)["id"] as? String ?? ""
                LocalNotifications.pruneExpired()
                return Bridge.ok(LocalNotifications.load(id: id))
            }

            // rearm() — re-registers the saved schedules (pending requests normally survive by
            // themselves) after dropping one-off schedules whose time has passed.
            Function("rearm") { _ in
                LocalNotifications.pruneExpired()
                for (id, schedule) in LocalNotifications.all() {
                    LocalNotifications.arm(id: id, schedule: schedule)
                }
                return Bridge.ok(true)
            }

            Function("cancel") { args in
                LocalNotifications.remove(id: Bridge.args(args)["id"] as? String ?? "")
                return Bridge.ok(true)
            }

            Function("cancelAll") { _ in
                LocalNotifications.removeAll()
                return Bridge.ok(true)
            }

            // openSettings() — opens this app's notification settings screen.
            Function("openSettings") { _ in
                let raw: String
                if #available(iOS 16.0, *) {
                    raw = UIApplication.openNotificationSettingsURLString
                } else {
                    raw = UIApplication.openSettingsURLString
                }
                guard let url = URL(string: raw) else { return Bridge.err("settings URL unavailable") }
                UIApplication.shared.open(url)
                return Bridge.ok(true)
            }

            // getInitialTap() -> { id, payload } | null. Consumes the tap that launched the app.
            AsyncFunction("getInitialTap") { _, promise in
                Bridge.onMain {
                    TapStore.shared.waitForLaunchTap { tap in Bridge.resolve(promise, tap) }
                }
            }
        }
    }

    fileprivate func send(_ tap: [String: Any?]) {
        sendEvent("tap", Bridge.event(tap))
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }
}

enum LocalNotifications {
    private static let defaultsKey = "MasamuneWhiskerLocalNotification.schedules"
    /// Prefix of this module's request identifiers.
    static let identifierPrefix = "masamune_whisker_local_notification."
    /// userInfo keys of this module's notifications.
    static let idKey = "masamuneWhiskerLocalNotification"
    static let payloadKey = "masamuneWhiskerLocalNotificationPayload"

    private static let lock = NSLock()
    /// "authorized", "denied" or "notDetermined"; `nil` until the first refresh finishes.
    private static var cachedStatus: String?
    private static let proxy = NotificationCenterProxy()

    // MARK: Permission

    /// The cached authorization status. Before the first refresh has finished, waits for it briefly.
    static func status() -> String {
        if let value = locked({ cachedStatus }) { return value }
        let semaphore = DispatchSemaphore(value: 0)
        // The completion runs on a background queue, so waiting here (on the main thread) is safe.
        refreshStatus { _ in semaphore.signal() }
        _ = semaphore.wait(timeout: .now() + 1)
        return locked { cachedStatus } ?? "denied"
    }

    static func authorized() -> Bool {
        status() == "authorized"
    }

    static func refreshStatus(_ completion: ((Bool) -> Void)? = nil) {
        UNUserNotificationCenter.current().getNotificationSettings { settings in
            let status: String
            switch settings.authorizationStatus {
            case .authorized, .provisional, .ephemeral: status = "authorized"
            case .notDetermined: status = "notDetermined"
            default: status = "denied"
            }
            locked { cachedStatus = status }
            completion?(status == "authorized")
        }
    }

    // MARK: Delegate

    /// Installs the foreground-presenting delegate in front of the current one (idempotent).
    static func installDelegate() {
        Bridge.onMain {
            let center = UNUserNotificationCenter.current()
            guard center.delegate !== proxy else { return }
            proxy.next = center.delegate
            center.delegate = proxy
        }
    }

    // MARK: Schedules

    private static func saved() -> [String: String] {
        UserDefaults.standard.dictionary(forKey: defaultsKey) as? [String: String] ?? [:]
    }

    static func save(id: String, schedule: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: schedule),
              let text = String(data: data, encoding: .utf8)
        else { return }
        var schedules = saved()
        schedules[id] = text
        UserDefaults.standard.set(schedules, forKey: defaultsKey)
    }

    static func load(id: String) -> [String: Any]? {
        saved()[id].flatMap(decode)
    }

    static func all() -> [String: [String: Any]] {
        saved().compactMapValues(decode)
    }

    private static func decode(_ text: String) -> [String: Any]? {
        guard let data = text.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    static func remove(id: String) {
        var schedules = saved()
        schedules[id] = nil
        UserDefaults.standard.set(schedules, forKey: defaultsKey)
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: [identifierPrefix + id])
    }

    static func removeAll() {
        let ids = saved().keys.map { identifierPrefix + $0 }
        UserDefaults.standard.removeObject(forKey: defaultsKey)
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: ids)
    }

    /// Removes one-off schedules whose time has passed (their requests have fired).
    static func pruneExpired() {
        let now = Date()
        var schedules = saved()
        let expired = schedules.filter { _, text in
            guard let schedule = decode(text), isOnce(schedule) else { return false }
            return fireDate(schedule).map { $0 <= now } ?? true
        }
        guard !expired.isEmpty else { return }
        expired.keys.forEach { schedules[$0] = nil }
        UserDefaults.standard.set(schedules, forKey: defaultsKey)
    }

    static func isOnce(_ schedule: [String: Any]) -> Bool {
        (schedule["once"] as? Bool) ?? (schedule["once"] as? NSNumber)?.boolValue ?? false
    }

    /// The fire time of a one-off schedule.
    static func fireDate(_ schedule: [String: Any]) -> Date? {
        guard let atMs = (schedule["atMs"] as? NSNumber)?.int64Value else { return nil }
        return Date(timeIntervalSince1970: TimeInterval(atMs) / 1000)
    }

    /// Registers (or replaces) the request of a saved schedule: repeating for daily schedules, a
    /// single calendar date for one-off schedules (skipped once the date has passed).
    static func arm(id: String, schedule: [String: Any]) {
        let trigger: UNCalendarNotificationTrigger
        if isOnce(schedule) {
            guard let date = fireDate(schedule), date > Date() else { return }
            // Calendar triggers match whole seconds: round up so the request never fires before atMs.
            let second = Date(timeIntervalSince1970: date.timeIntervalSince1970.rounded(.up))
            let components = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute, .second], from: second)
            trigger = UNCalendarNotificationTrigger(dateMatching: components, repeats: false)
        } else {
            guard let hour = (schedule["hour"] as? NSNumber)?.intValue,
                  let minute = (schedule["minute"] as? NSNumber)?.intValue
            else { return }
            trigger = UNCalendarNotificationTrigger(dateMatching: DateComponents(hour: hour, minute: minute), repeats: true)
        }
        let content = UNMutableNotificationContent()
        content.title = schedule["title"] as? String ?? ""
        content.body = schedule["text"] as? String ?? ""
        content.sound = .default
        var userInfo: [String: Any] = [idKey: id]
        if let payload = schedule["payload"] as? String { userInfo[payloadKey] = payload }
        content.userInfo = userInfo
        let request = UNNotificationRequest(identifier: identifierPrefix + id, content: content, trigger: trigger)
        UNUserNotificationCenter.current().add(request) { error in
            if let error {
                NSLog("MasamuneWhiskerLocalNotification: failed to schedule %@: %@", id, Bridge.message(error))
            }
        }
    }

    /// The tap reported for one of this module's notifications (`nil` for other notifications).
    static func tap(_ notification: UNNotification) -> [String: Any?]? {
        let request = notification.request
        guard request.identifier.hasPrefix(identifierPrefix) else { return nil }
        let userInfo = request.content.userInfo
        let id = userInfo[idKey] as? String ?? String(request.identifier.dropFirst(identifierPrefix.count))
        return ["id": id, "payload": userInfo[payloadKey] as? String]
    }

    /// The next occurrence of the time in milliseconds (tomorrow if today's time has already passed).
    static func nextOccurrence(hour: Int, minute: Int) -> Int64 {
        let calendar = Calendar.current
        let now = Date()
        var date = calendar.date(bySettingHour: hour, minute: minute, second: 0, of: now) ?? now
        if date <= now {
            date = calendar.date(byAdding: .day, value: 1, to: date) ?? date
        }
        return Int64(date.timeIntervalSince1970 * 1000)
    }

    private static func locked<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }
}

/// Presents this module's notifications in the foreground, reports taps on them, and forwards
/// everything else (and every other delegate method) to the previously installed delegate.
private final class NotificationCenterProxy: NSObject, UNUserNotificationCenterDelegate {
    weak var next: UNUserNotificationCenterDelegate?

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        if notification.request.identifier.hasPrefix(LocalNotifications.identifierPrefix) {
            LocalNotifications.pruneExpired()
            completionHandler([.banner, .list, .sound])
        } else if let next, next.responds(to: #selector(userNotificationCenter(_:willPresent:withCompletionHandler:))) {
            next.userNotificationCenter?(center, willPresent: notification, withCompletionHandler: completionHandler)
        } else {
            completionHandler([])
        }
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        if let tap = LocalNotifications.tap(response.notification) {
            LocalNotifications.pruneExpired()
            if response.actionIdentifier == UNNotificationDefaultActionIdentifier {
                Bridge.onMain { TapStore.shared.receive(tap) }
            }
            completionHandler()
        } else if let next, next.responds(to: #selector(userNotificationCenter(_:didReceive:withCompletionHandler:))) {
            next.userNotificationCenter?(center, didReceive: response, withCompletionHandler: completionHandler)
        } else {
            completionHandler()
        }
    }

    // Other delegate methods (for example `openSettingsFor`) go to the previous delegate as is.
    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || (next?.responds(to: selector) ?? false)
    }

    override func forwardingTarget(for selector: Selector!) -> Any? {
        if let next, next.responds(to: selector) { return next }
        return super.forwardingTarget(for: selector)
    }
}

/// The launch tap, queued taps and the observing state. Main thread only.
///
/// The first tap that arrives while launching is the launch tap, returned once by
/// `getInitialTap`. Launching ends when `getInitialTap` answers (it waits until the app has been
/// active for a moment, at most two seconds) or a few seconds after the app first becomes active;
/// a launch tap nobody asked for is then delivered like any other tap.
final class TapStore {
    static let shared = TapStore()

    weak var module: LocalNotificationModule?
    var observing = false
    private var launching = true
    private var everActive = false
    private var launchTap: [String: Any?]?
    private var queued: [[String: Any?]] = []
    private var waiters: [([String: Any?]?) -> Void] = []

    /// How long `getInitialTap` waits after the app is active for the launch tap.
    private static let activeGrace: TimeInterval = 0.5
    /// How long `getInitialTap` waits at most.
    private static let maxWait: TimeInterval = 2
    /// When launching ends without `getInitialTap`, counted from the first activation.
    private static let launchWindow: TimeInterval = 5

    func receive(_ tap: [String: Any?]) {
        if launching && launchTap == nil {
            launchTap = tap
            if !waiters.isEmpty { finishLaunch() }
            return
        }
        deliver(tap)
    }

    func startObserving() {
        observing = true
        let pending = queued
        queued.removeAll()
        pending.forEach(deliver)
    }

    func becameActive() {
        if !everActive {
            everActive = true
            after(Self.launchWindow) { $0.finishLaunch() }
        }
        if !waiters.isEmpty {
            after(Self.activeGrace) { $0.finishLaunch() }
        }
    }

    /// Answers with the launch tap, waiting for it while the app is launching.
    func waitForLaunchTap(_ completion: @escaping ([String: Any?]?) -> Void) {
        guard launching else { return completion(nil) }
        if launchTap != nil {
            waiters.append(completion)
            return finishLaunch()
        }
        waiters.append(completion)
        if everActive || UIApplication.shared.applicationState == .active {
            after(Self.activeGrace) { $0.finishLaunch() }
        }
        after(Self.maxWait) { $0.finishLaunch() }
    }

    /// Ends launching: hands the launch tap to the waiters, or delivers it when nobody asked.
    private func finishLaunch() {
        guard launching else { return }
        launching = false
        let tap = launchTap
        launchTap = nil
        let pending = waiters
        waiters.removeAll()
        if pending.isEmpty {
            if let tap { deliver(tap) }
        } else {
            pending.forEach { $0(tap) }
        }
    }

    private func deliver(_ tap: [String: Any?]) {
        guard observing, let module else {
            queued.append(tap)
            return
        }
        module.send(tap)
    }

    private func after(_ delay: TimeInterval, _ body: @escaping (TapStore) -> Void) {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
            if let self { body(self) }
        }
    }
}
