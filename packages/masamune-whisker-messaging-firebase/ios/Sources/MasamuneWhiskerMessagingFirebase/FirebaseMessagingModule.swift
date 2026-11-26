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
                UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
                    if let error {
                        Bridge.reject(promise, error)
                        return
                    }
                    if granted {
                        Bridge.onMain {
                            UIApplication.shared.registerForRemoteNotifications()
                        }
                    }
                    Bridge.resolve(promise, granted)
                }
            }

            // getToken() -> String?
            AsyncFunction("getToken") { _, promise in
                Bridge.onMain {
                    do {
                        try FirebaseBoot.ensure()
                        MessagingBridge.shared.ensureDelegate()
                        Messaging.messaging().register { error in
                            if let error {
                                Bridge.reject(promise, error)
                                return
                            }
                            Bridge.resolve(promise, MessagingBridge.shared.currentToken)
                        }
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
                        Messaging.messaging().unregister { error in
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

private final class MessagingBridge: NSObject, MessagingDelegate {
    static let shared = MessagingBridge()
    weak var module: FirebaseMessagingModule?
    private var delegateInstalled = false
    private var observingToken = false
    private var observingMessages = false
    private var observingOpened = false
    private var latestToken: String?
    private var initialMessage: [String: Any?]?
    private var queuedMessages: [[String: Any?]] = []
    private var queuedOpened: [[String: Any?]] = []

    var currentToken: String? { latestToken }

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
        let pending = queuedOpened
        queuedOpened.removeAll()
        for payload in pending {
            module?.sendEvent("messageOpened", Bridge.event(payload))
        }
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
