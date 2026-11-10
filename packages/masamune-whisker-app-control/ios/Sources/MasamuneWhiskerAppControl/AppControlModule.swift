// Opens URLs in external apps (browser, store), locks the screen orientation and styles the window.
//
// The host app owns the root view controller, so the orientation lock and the status bar style are
// applied by switching that controller to a runtime subclass which overrides
// `supportedInterfaceOrientations` / `preferredStatusBarStyle` (see `RootControllerOverride`).

import ObjectiveC
import UIKit
import WhiskerModule

@WhiskerModule
public final class AppControlModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("AppControl")

            // openUrl({ url }) -> whether it was opened (false if no app can handle it)
            Function("openUrl") { args in
                guard let raw = Bridge.args(args)["url"] as? String else { return Bridge.err("No value for url") }
                guard let url = Self.url(raw), let scheme = url.scheme?.lowercased() else { return Bridge.ok(false) }
                let application = UIApplication.shared
                // `canOpenURL` only answers for schemes listed in `LSApplicationQueriesSchemes`;
                // web links always have a handler.
                guard scheme == "http" || scheme == "https" || application.canOpenURL(url) else {
                    return Bridge.ok(false)
                }
                application.open(url)
                return Bridge.ok(true)
            }

            // setOrientation({ lock }) — locks the screen orientation ("unspecified" unlocks it).
            Function("setOrientation") { args in
                guard let window = Self.keyWindow(), let root = window.rootViewController else {
                    return Bridge.err("no window")
                }
                let mask = Self.orientationMask(Bridge.args(args)["lock"] as? String ?? "")
                RootControllerOverride.orientationMask = mask
                guard RootControllerOverride.install(on: root) else {
                    return Bridge.err("the root view controller cannot be overridden")
                }
                Self.applyOrientation(mask, window: window, root: root)
                return Bridge.ok(true)
            }

            // setSystemBarsStyle({ lightStatusBar, lightNavigationBar }) — whether the status bar uses dark
            // content (for light backgrounds). iOS has no navigation bar, so `lightNavigationBar` is ignored.
            Function("setSystemBarsStyle") { args in
                guard let root = Self.keyWindow()?.rootViewController else { return Bridge.err("no window") }
                let light = Bridge.args(args)["lightStatusBar"] as? Bool ?? false
                RootControllerOverride.statusBarStyle = light ? .darkContent : .lightContent
                guard RootControllerOverride.install(on: root) else {
                    return Bridge.err("the root view controller cannot be overridden")
                }
                root.setNeedsStatusBarAppearanceUpdate()
                Self.topController(of: root).setNeedsStatusBarAppearanceUpdate()
                return Bridge.ok(true)
            }

            // setWindowBackground({ argb }) — window background color (visible right after launch and at screen edges).
            Function("setWindowBackground") { args in
                guard let number = Bridge.args(args)["argb"] as? NSNumber else { return Bridge.err("No value for argb") }
                guard let window = Self.keyWindow() else { return Bridge.err("no window") }
                let color = Self.color(argb: UInt32(truncatingIfNeeded: number.int64Value))
                window.backgroundColor = color
                // The host's root view covers the window, so it carries the same color.
                window.rootViewController?.view.backgroundColor = color
                return Bridge.ok(true)
            }

            // exitApp() — iOS apps must not quit themselves (App Store Review Guidelines), so this is a no-op.
            Function("exitApp") { _ in
                Bridge.ok(false)
            }
        }
    }

    // MARK: - Helpers

    /// Parses a URL, percent-encoding characters that iOS 15 / 16 reject (spaces, non-ASCII).
    private static func url(_ raw: String) -> URL? {
        if let url = URL(string: raw) { return url }
        let allowed = CharacterSet.urlFragmentAllowed.union(CharacterSet(charactersIn: "#%"))
        return raw.addingPercentEncoding(withAllowedCharacters: allowed).flatMap(URL.init(string:))
    }

    /// Maps the Android `screenOrientation` values onto interface orientation masks (`nil` unlocks).
    private static func orientationMask(_ lock: String) -> UIInterfaceOrientationMask? {
        switch lock {
        // Android `portrait` is the upright portrait only.
        case "portrait": return .portrait
        // Android `landscape` is the device turned counter-clockwise (home indicator on the right).
        case "landscape": return .landscapeRight
        case "sensorPortrait": return [.portrait, .portraitUpsideDown]
        case "sensorLandscape": return .landscape
        default: return nil
        }
    }

    private static func applyOrientation(_ mask: UIInterfaceOrientationMask?, window: UIWindow, root: UIViewController) {
        if #available(iOS 16.0, *) {
            root.setNeedsUpdateOfSupportedInterfaceOrientations()
            topController(of: root).setNeedsUpdateOfSupportedInterfaceOrientations()
            if let mask, let scene = window.windowScene {
                // Fails (and is ignored) when the mask is outside the app's supported orientations or
                // the scene cannot rotate (iPad multitasking).
                scene.requestGeometryUpdate(.iOS(interfaceOrientations: mask)) { _ in }
            }
        } else {
            // iOS 15 rotates only towards the current device orientation.
            UIViewController.attemptRotationToDeviceOrientation()
        }
    }

    private static func color(argb: UInt32) -> UIColor {
        UIColor(
            red: CGFloat((argb >> 16) & 0xFF) / 255,
            green: CGFloat((argb >> 8) & 0xFF) / 255,
            blue: CGFloat(argb & 0xFF) / 255,
            alpha: CGFloat((argb >> 24) & 0xFF) / 255
        )
    }

    private static func topController(of root: UIViewController) -> UIViewController {
        var top = root
        while let presented = top.presentedViewController, !presented.isBeingDismissed {
            top = presented
        }
        return top
    }

    static func keyWindow() -> UIWindow? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let ordered = scenes.filter { $0.activationState == .foregroundActive } + scenes
        for scene in ordered {
            if let window = scene.windows.first(where: \.isKeyWindow) ?? scene.windows.first {
                return window
            }
        }
        return nil
    }
}

/// Overrides the orientation mask and status bar style of the host's root view controller.
///
/// The controller is switched (`object_setClass`) to a runtime subclass of its own class whose
/// overrides return the values below, falling back to the original implementation while unset.
enum RootControllerOverride {
    static var orientationMask: UIInterfaceOrientationMask?
    static var statusBarStyle: UIStatusBarStyle?

    private static let prefix = "MasamuneWhiskerAppControl_"

    /// Returns `false` when the controller cannot be subclassed (for example while observed by KVO).
    static func install(on controller: UIViewController) -> Bool {
        guard let base = object_getClass(controller) else { return false }
        let baseName = NSStringFromClass(base)
        if baseName.hasPrefix(prefix) { return true }
        if baseName.hasPrefix("NSKVONotifying_") { return false }
        let name = prefix + baseName
        let subclass: AnyClass
        if let existing = NSClassFromString(name) {
            subclass = existing
        } else {
            guard let created = objc_allocateClassPair(base, name, 0) else { return false }
            addOverrides(to: created, base: base)
            objc_registerClassPair(created)
            subclass = created
        }
        object_setClass(controller, subclass)
        return true
    }

    private typealias Getter = @convention(c) (AnyObject, Selector) -> Int

    private static func addOverrides(to subclass: AnyClass, base: AnyClass) {
        override(
            #selector(getter: UIViewController.supportedInterfaceOrientations),
            in: subclass,
            base: base
        ) { orientationMask.map { Int(bitPattern: $0.rawValue) } }
        override(
            #selector(getter: UIViewController.preferredStatusBarStyle),
            in: subclass,
            base: base
        ) { statusBarStyle?.rawValue }
    }

    /// Adds an integer-returning getter that returns `value()` or, when `nil`, the base implementation.
    private static func override(_ selector: Selector, in subclass: AnyClass, base: AnyClass, value: @escaping () -> Int?) {
        guard let method = class_getInstanceMethod(base, selector),
              let original = class_getMethodImplementation(base, selector)
        else { return }
        let fallback = unsafeBitCast(original, to: Getter.self)
        let block: @convention(block) (AnyObject) -> Int = { object in
            value() ?? fallback(object, selector)
        }
        class_addMethod(subclass, selector, imp_implementationWithBlock(block), method_getTypeEncoding(method))
    }
}
