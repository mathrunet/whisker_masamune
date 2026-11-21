// Receives custom scheme URLs and Universal Links (iOS).
//
// The Whisker scene delegate does not forward URLs, so the module hooks the app's scene delegate
// class (UIApplicationSceneManifest's UISceneDelegateClassName) when it is registered from
// `application(_:didFinishLaunchingWithOptions:)`, before the first scene connects:
//
// - `scene(_:willConnectTo:options:)` is wrapped to read the launch URL or Universal Link from the
//   connection options.
// - `scene(_:openURLContexts:)` and `scene(_:continue:)` are added (or wrapped when the class
//   implements them) to receive links while the app runs.
//
// The launch link is returned once by `getInitialLink`. Later links are sent as the `link` event;
// links that arrive while nobody observes are queued and sent when observation starts.

import Foundation
import ObjectiveC
import UIKit
import WhiskerModule

@WhiskerModule
public final class DeeplinkModule: Module {
    public override func definition() -> ModuleDefinition {
        DeeplinkHooks.install()
        DeeplinkStore.shared.module = self
        return ModuleDefinition {
            Name("Deeplink")
            Events("link")

            OnStartObserving("link") {
                Bridge.onMain { DeeplinkStore.shared.startObserving() }
            }
            OnStopObserving("link") {
                Bridge.onMain { DeeplinkStore.shared.observing = false }
            }

            // getInitialLink() -> string | null. Consumes the launch link.
            AsyncFunction("getInitialLink") { _, promise in
                Bridge.onMain { Bridge.resolve(promise, DeeplinkStore.shared.takeLaunchLink()) }
            }
        }
    }

    fileprivate func send(_ link: String) {
        sendEvent("link", .string(Self.json(link)))
    }

    /// The event payload: the link as a JSON string literal.
    private static func json(_ link: String) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: [link]),
              let text = String(data: data, encoding: .utf8)
        else { return "\"\"" }
        // `["…"]` → `"…"`
        return String(text.dropFirst().dropLast())
    }
}

/// Launch link, queued links and the observing state. Main thread only.
final class DeeplinkStore {
    static let shared = DeeplinkStore()

    weak var module: DeeplinkModule?
    var observing = false
    private var launchLink: String?
    private var launchLinkTaken = false
    private var queued: [String] = []

    func setLaunchLink(_ link: String) {
        guard !launchLinkTaken, launchLink == nil else { return receive(link) }
        launchLink = link
    }

    func takeLaunchLink() -> String? {
        launchLinkTaken = true
        defer { launchLink = nil }
        return launchLink
    }

    func receive(_ link: String) {
        guard observing, let module else {
            queued.append(link)
            return
        }
        module.send(link)
    }

    func startObserving() {
        observing = true
        let pending = queued
        queued.removeAll()
        pending.forEach(receive)
    }
}

/// Installs the scene delegate hooks once.
enum DeeplinkHooks {
    private static var installed = false

    private typealias WillConnect = @convention(c) (AnyObject, Selector, UIScene, UISceneSession, UIScene.ConnectionOptions) -> Void
    private typealias OpenContexts = @convention(c) (AnyObject, Selector, UIScene, NSSet) -> Void
    private typealias ContinueActivity = @convention(c) (AnyObject, Selector, UIScene, NSUserActivity) -> Void

    static func install() {
        guard !installed else { return }
        installed = true
        guard let delegateClass = sceneDelegateClass() else {
            NSLog("[masamune-whisker-deeplink] no scene delegate class; links are not received")
            return
        }
        hookWillConnect(delegateClass)
        hookOpenURLContexts(delegateClass)
        hookContinueActivity(delegateClass)
    }

    private static func sceneDelegateClass() -> AnyClass? {
        guard let manifest = Bundle.main.object(forInfoDictionaryKey: "UIApplicationSceneManifest") as? [String: Any],
              let configurations = manifest["UISceneConfigurations"] as? [String: Any],
              let roles = configurations[UISceneSession.Role.windowApplication.rawValue] as? [[String: Any]],
              let name = roles.first?["UISceneDelegateClassName"] as? String
        else { return nil }
        let expanded = name.replacingOccurrences(
            of: "$(PRODUCT_MODULE_NAME)",
            with: Bundle.main.object(forInfoDictionaryKey: "CFBundleExecutable") as? String ?? ""
        )
        return NSClassFromString(expanded)
    }

    static func link(from activity: NSUserActivity) -> String? {
        guard activity.activityType == NSUserActivityTypeBrowsingWeb else { return nil }
        return activity.webpageURL?.absoluteString
    }

    private static func hookWillConnect(_ cls: AnyClass) {
        let selector = NSSelectorFromString("scene:willConnectToSession:options:")
        guard let method = class_getInstanceMethod(cls, selector) else { return }
        let original = unsafeBitCast(method_getImplementation(method), to: WillConnect.self)
        let block: @convention(block) (AnyObject, UIScene, UISceneSession, UIScene.ConnectionOptions) -> Void = { object, scene, session, options in
            original(object, selector, scene, session, options)
            if let url = options.urlContexts.first?.url {
                DeeplinkStore.shared.setLaunchLink(url.absoluteString)
            } else if let link = options.userActivities.lazy.compactMap(link(from:)).first {
                DeeplinkStore.shared.setLaunchLink(link)
            }
        }
        method_setImplementation(method, imp_implementationWithBlock(block))
    }

    private static func hookOpenURLContexts(_ cls: AnyClass) {
        let selector = NSSelectorFromString("scene:openURLContexts:")
        let handle: (NSSet) -> Void = { contexts in
            for case let context as UIOpenURLContext in contexts {
                DeeplinkStore.shared.receive(context.url.absoluteString)
            }
        }
        if let method = class_getInstanceMethod(cls, selector) {
            let original = unsafeBitCast(method_getImplementation(method), to: OpenContexts.self)
            let block: @convention(block) (AnyObject, UIScene, NSSet) -> Void = { object, scene, contexts in
                original(object, selector, scene, contexts)
                handle(contexts)
            }
            method_setImplementation(method, imp_implementationWithBlock(block))
        } else {
            let block: @convention(block) (AnyObject, UIScene, NSSet) -> Void = { _, _, contexts in
                handle(contexts)
            }
            class_addMethod(cls, selector, imp_implementationWithBlock(block), "v@:@@")
        }
    }

    private static func hookContinueActivity(_ cls: AnyClass) {
        let selector = NSSelectorFromString("scene:continueUserActivity:")
        let handle: (NSUserActivity) -> Void = { activity in
            if let link = link(from: activity) { DeeplinkStore.shared.receive(link) }
        }
        if let method = class_getInstanceMethod(cls, selector) {
            let original = unsafeBitCast(method_getImplementation(method), to: ContinueActivity.self)
            let block: @convention(block) (AnyObject, UIScene, NSUserActivity) -> Void = { object, scene, activity in
                original(object, selector, scene, activity)
                handle(activity)
            }
            method_setImplementation(method, imp_implementationWithBlock(block))
        } else {
            let block: @convention(block) (AnyObject, UIScene, NSUserActivity) -> Void = { _, _, activity in
                handle(activity)
            }
            class_addMethod(cls, selector, imp_implementationWithBlock(block), "v@:@@")
        }
    }
}
