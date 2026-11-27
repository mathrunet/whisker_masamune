// Firebase Cloud Messaging (FCM) native module for iOS.

import FirebaseCore
import FirebaseMessaging
import Foundation
import UIKit
import UserNotifications
import WhiskerModule

@WhiskerModule
public final class FirebaseMessagingModule: Module {
    public override func definition() -> ModuleDefinition {
        MessagingBridge.shared.module = self
        MessagingBridge.shared.installNotificationDelegate()
        return ModuleDefinition {
            Name("FirebaseMessaging")
            Events("tokenRefresh", "message", "messageOpened")

            OnStartObserving("tokenRefresh") {
                Bridge.onMain { MessagingBridge.shared.startObservingToken() }
            }
            OnStartObserving("message") {
                Bridge.onMain { MessagingBridge.shared.startObservingMessages() }
            }
            OnStartObserving("messageOpened") {
                Bridge.onMain { MessagingBridge.shared.startObservingOpened() }
            }

            // requestPermission() -> Bool
            AsyncFunction("requestPermission") { _, promise in
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                        MessagingBridge.shared.ensureDelegate()
                        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
                            if let error {
                                Bridge.reject(promise, error)
                                return
                            }
                            if granted {
                                Bridge.onMain { UIApplication.shared.registerForRemoteNotifications() }
                            }
                            Bridge.resolve(promise, granted)
                        }
                    } catch {
                        Bridge.reject(promise, error)
                    }
                }
            }

            // getToken() -> String?
            AsyncFunction("getToken") { _, promise in
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                        MessagingBridge.shared.ensureDelegate()
                        MessagingBridge.shared.requestToken(promise)
                    } catch {
                        Bridge.reject(promise, error)
                    }
                }
            }

            // deleteToken() -> Bool
            AsyncFunction("deleteToken") { _, promise in
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                        Messaging.messaging().deleteToken { error in
                            if let error {
                                Bridge.reject(promise, error)
                                return
                            }
                            MessagingBridge.shared.clearToken()
                            Bridge.resolve(promise, true)
                        }
                    } catch {
                        Bridge.reject(promise, error)
                    }
                }
            }

            // getInitialMessage() -> PushMessage?
            AsyncFunction("getInitialMessage") { _, promise in
                Bridge.onMain {
                    Bridge.resolve(promise, MessagingBridge.shared.takeInitialMessage())
                }
            }
        }
    }
}

private final class MessagingBridge: NSObject, MessagingDelegate, UNUserNotificationCenterDelegate {
    static let shared = MessagingBridge()
    weak var module: FirebaseMessagingModule?
    private var delegateInstalled = false
    private var observingToken = false
    private var observingMessages = false
    private var observingOpened = false
    private var latestToken: String?
    private var initialMessage: [String: Any?]?
    private weak var previousNotificationDelegate: UNUserNotificationCenterDelegate?
    // Cooperates with the LocalNotification delegate when it refreshes its installation.
    @objc var masamunePreviousNotificationDelegate: AnyObject? { previousNotificationDelegate }

    private var queuedMessages: [[String: Any?]] = []

    func installNotificationDelegate() {
        Bridge.onMain {
            let center = UNUserNotificationCenter.current()
            guard center.delegate !== self else { return }
            self.previousNotificationDelegate = center.delegate
            center.delegate = self
        }
    }

    // FCM cannot return an APNs-bound token before remote registration completes.
    func requestToken(_ promise: WhiskerPromise, attemptsRemaining: Int = 20) {
        let messaging = Messaging.messaging()
        guard messaging.apnsToken != nil else {
            if attemptsRemaining <= 0 {
                Bridge.reject(promise, "APNs registration unavailable")
                return
            }
            if attemptsRemaining == 20 { UIApplication.shared.registerForRemoteNotifications() }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
                self.requestToken(promise, attemptsRemaining: attemptsRemaining - 1)
            }
            return
        }
        // The server targets the FCM registration token via HTTP v1.
        messaging.token { token, error in
            if let error { Bridge.reject(promise, error) }
            else { Bridge.resolve(promise, token) }
        }
    }

    func clearToken() {
        latestToken = nil
    }

    func ensureDelegate() {
        guard !delegateInstalled else { return }
        Messaging.messaging().delegate = self
        delegateInstalled = true
    }

    func startObservingToken() {
        observingToken = true
        if FirebaseApp.app() != nil {
            ensureDelegate()
        }
        if let token = latestToken, !token.isEmpty {
            module?.sendEvent("tokenRefresh", .string(token))
        }
    }

    func startObservingMessages() {
        observingMessages = true
        let pending = queuedMessages
        queuedMessages.removeAll()
        for payload in pending {
            module?.sendEvent("message", Bridge.event(payload))
        }
    }

    func startObservingOpened() {
        observingOpened = true
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completion: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        guard notification.request.trigger is UNPushNotificationTrigger else {
            if let previousNotificationDelegate, previousNotificationDelegate.responds(
                to: #selector(userNotificationCenter(_:willPresent:withCompletionHandler:))
            ) {
                previousNotificationDelegate.userNotificationCenter?(center, willPresent: notification, withCompletionHandler: completion)
            } else {
                completion([])
            }
            return
        }
        let payload = Self.payload(notification)
        Bridge.onMain {
            if self.observingMessages { self.module?.sendEvent("message", Bridge.event(payload)) }
            else { self.queuedMessages.append(payload) }
        }
        completion([])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completion: @escaping () -> Void
    ) {
        guard response.notification.request.trigger is UNPushNotificationTrigger else {
            if let previousNotificationDelegate, previousNotificationDelegate.responds(
                to: #selector(userNotificationCenter(_:didReceive:withCompletionHandler:))
            ) {
                previousNotificationDelegate.userNotificationCenter?(center, didReceive: response, withCompletionHandler: completion)
            } else {
                completion()
            }
            return
        }
        if response.actionIdentifier == UNNotificationDefaultActionIdentifier {
            let payload = Self.payload(response.notification)
            Bridge.onMain {
                if self.observingOpened { self.module?.sendEvent("messageOpened", Bridge.event(payload)) }
                else { self.initialMessage = payload }
            }
        }
        completion()
    }

    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || (previousNotificationDelegate?.responds(to: selector) ?? false)
    }

    override func forwardingTarget(for selector: Selector!) -> Any? {
        if let previousNotificationDelegate, previousNotificationDelegate.responds(to: selector) {
            return previousNotificationDelegate
        }
        return super.forwardingTarget(for: selector)
    }

    private static func payload(_ notification: UNNotification) -> [String: Any?] {
        let content = notification.request.content
        var data: [String: String] = [:]
        for (key, value) in content.userInfo {
            guard let key = key as? String, let value = value as? String,
                  !key.hasPrefix("gcm."), !key.hasPrefix("google."), key != "from" else { continue }
            data[key] = value
        }
        return [
            "id": content.userInfo["gcm.message_id"] as? String ?? notification.request.identifier,
            "title": content.title,
            "body": content.body,
            "data": data,
        ]
    }

    func takeInitialMessage() -> [String: Any?]? {
        defer { initialMessage = nil }
        return initialMessage
    }

    func messaging(_ messaging: Messaging, didReceiveRegistrationToken fcmToken: String?) {
        guard let token = fcmToken, !token.isEmpty else { return }
        latestToken = token
        if observingToken {
            module?.sendEvent("tokenRefresh", .string(token))
        }
    }
}
