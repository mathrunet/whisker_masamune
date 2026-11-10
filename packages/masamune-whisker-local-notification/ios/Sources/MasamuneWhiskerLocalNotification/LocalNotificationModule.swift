// Daily local notifications at a fixed time (UNUserNotificationCenter with repeating calendar triggers).
// Schedules are saved in UserDefaults so `schedule(id)` returns them like on Android; iOS keeps
// repeating requests across reboots and app updates by itself.
//
// Like Android, notifications are also shown while the app is in the foreground: a notification
// center delegate presents this module's notifications and forwards everything else to the delegate
// that was installed before it (for example Firebase Messaging).
//
// Android notification channels and small icons have no iOS counterpart, so `configure` stores
// nothing and the app icon is used.

import UIKit
import UserNotifications
import WhiskerModule

@WhiskerModule
public final class LocalNotificationModule: Module {
    private var activeObserver: NSObjectProtocol?

    public override func definition() -> ModuleDefinition {
        // Modules are installed from didFinishLaunching: refresh the cached permission now and every
        // time the app becomes active (the user may change it in Settings meanwhile).
        LocalNotifications.installDelegate()
        LocalNotifications.refreshStatus()
        activeObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { _ in
            LocalNotifications.installDelegate()
            LocalNotifications.refreshStatus()
        }

        return ModuleDefinition {
            Name("LocalNotification")

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

            // scheduleDaily({ id, hour, minute, title, text }) -> next fire time (ms)
            Function("scheduleDaily") { args in
                let json = Bridge.args(args)
                guard let id = json["id"] as? String else { return Bridge.err("No value for id") }
                guard let hour = (json["hour"] as? NSNumber)?.intValue else { return Bridge.err("No value for hour") }
                guard let minute = (json["minute"] as? NSNumber)?.intValue else { return Bridge.err("No value for minute") }
                LocalNotifications.installDelegate()
                LocalNotifications.save(id: id, schedule: json)
                LocalNotifications.arm(id: id, schedule: json)
                return Bridge.ok(LocalNotifications.nextOccurrence(hour: hour, minute: minute))
            }

            // schedule({ id }) -> saved schedule (null if missing)
            Function("schedule") { args in
                let id = Bridge.args(args)["id"] as? String ?? ""
                return Bridge.ok(LocalNotifications.load(id: id))
            }

            // rearm() — re-registers the saved schedules (repeating requests normally survive by themselves).
            Function("rearm") { _ in
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
        }
    }

    deinit {
        if let activeObserver { NotificationCenter.default.removeObserver(activeObserver) }
    }
}

enum LocalNotifications {
    private static let defaultsKey = "MasamuneWhiskerLocalNotification.schedules"
    /// Prefix of this module's request identifiers.
    static let identifierPrefix = "masamune_whisker_local_notification."

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

    /// Registers (or replaces) the repeating request of a saved schedule.
    static func arm(id: String, schedule: [String: Any]) {
        guard let hour = (schedule["hour"] as? NSNumber)?.intValue,
              let minute = (schedule["minute"] as? NSNumber)?.intValue
        else { return }
        let content = UNMutableNotificationContent()
        content.title = schedule["title"] as? String ?? ""
        content.body = schedule["text"] as? String ?? ""
        content.sound = .default
        content.userInfo = ["masamuneWhiskerLocalNotification": id]
        let trigger = UNCalendarNotificationTrigger(dateMatching: DateComponents(hour: hour, minute: minute), repeats: true)
        let request = UNNotificationRequest(identifier: identifierPrefix + id, content: content, trigger: trigger)
        UNUserNotificationCenter.current().add(request) { error in
            if let error {
                NSLog("MasamuneWhiskerLocalNotification: failed to schedule %@: %@", id, Bridge.message(error))
            }
        }
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

/// Presents this module's notifications in the foreground and forwards everything else (and every
/// other delegate method) to the previously installed delegate.
private final class NotificationCenterProxy: NSObject, UNUserNotificationCenterDelegate {
    weak var next: UNUserNotificationCenterDelegate?

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        if notification.request.identifier.hasPrefix(LocalNotifications.identifierPrefix) {
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
        if let next, next.responds(to: #selector(userNotificationCenter(_:didReceive:withCompletionHandler:))) {
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
