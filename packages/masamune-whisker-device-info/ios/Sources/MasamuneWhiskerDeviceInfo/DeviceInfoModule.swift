// Returns the time zone and app info, and reports window size and appearance changes
// (rotation, iPad multitasking, dark mode, Dynamic Type) as events.

import UIKit
import WhiskerModule

@WhiskerModule
public final class DeviceInfoModule: Module {
    private var metricsObserved = false
    private var configurationObserved = false
    private var probe: WindowProbe?
    private var tokens: [NSObjectProtocol] = []

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("DeviceInfo")
            Events("metricsChanged", "configurationChanged")

            OnStartObserving("metricsChanged") { [weak self] in
                Bridge.onMain {
                    self?.metricsObserved = true
                    self?.startObserving()
                    self?.sendMetrics()
                }
            }
            OnStopObserving("metricsChanged") { [weak self] in
                Bridge.onMain {
                    self?.metricsObserved = false
                    self?.stopObservingIfIdle()
                }
            }
            OnStartObserving("configurationChanged") { [weak self] in
                Bridge.onMain {
                    self?.configurationObserved = true
                    self?.startObserving()
                    self?.sendConfiguration()
                }
            }
            OnStopObserving("configurationChanged") { [weak self] in
                Bridge.onMain {
                    self?.configurationObserved = false
                    self?.stopObservingIfIdle()
                }
            }

            // info() -> { timezone, offsetMinutes, packageName,
            //             versionName, versionCode, debuggable, widthDp, heightDp, density, ... }
            Function("info") { [weak self] _ in
                guard let self else { return Bridge.err("DeviceInfo module released") }
                return Bridge.ok(self.info())
            }
        }
    }

    // MARK: - Observation

    private func startObserving() {
        attachProbe()
        guard tokens.isEmpty else { return }
        let center = NotificationCenter.default
        // The key window may not exist yet while the app launches.
        tokens.append(center.addObserver(forName: UIWindow.didBecomeKeyNotification, object: nil, queue: .main) { [weak self] _ in
            self?.attachProbe()
        })
        tokens.append(center.addObserver(forName: UIContentSizeCategory.didChangeNotification, object: nil, queue: .main) { [weak self] _ in
            self?.sendConfiguration()
        })
    }

    private func stopObservingIfIdle() {
        guard !metricsObserved, !configurationObserved else { return }
        tokens.forEach(NotificationCenter.default.removeObserver)
        tokens.removeAll()
        probe?.removeFromSuperview()
        probe = nil
    }

    /// Adds an invisible view that fills the key window, so window resizes and trait changes reach us.
    private func attachProbe() {
        guard let window = Self.keyWindow() else { return }
        if let probe, probe.window === window { return }
        probe?.removeFromSuperview()
        let view = WindowProbe(frame: window.bounds)
        view.onResize = { [weak self] in
            self?.sendMetrics()
            self?.sendConfiguration()
        }
        view.onTraitsChange = { [weak self] in self?.sendConfiguration() }
        window.insertSubview(view, at: 0)
        probe = view
    }

    private func sendMetrics() {
        guard metricsObserved, let window = Self.keyWindow() else { return }
        sendEvent("metricsChanged", Bridge.event(Self.metrics(window)))
    }

    private func sendConfiguration() {
        guard configurationObserved else { return }
        let window = Self.keyWindow()
        var payload: [String: Any?] = window.map(Self.metrics) ?? [:]
        payload.merge(Self.configuration(window), uniquingKeysWith: { _, new in new })
        sendEvent("configurationChanged", Bridge.event(payload))
    }

    // MARK: - Values

    private func info() -> [String: Any?] {
        let bundle = Bundle.main
        let zone = TimeZone.current
        let window = Self.keyWindow()
        var result: [String: Any?] = window.map(Self.metrics) ?? [
            "widthDp": Double(UIScreen.main.bounds.width),
            "heightDp": Double(UIScreen.main.bounds.height),
            "density": Double(UIScreen.main.scale),
        ]
        result.merge(Self.configuration(window), uniquingKeysWith: { _, new in new })
        result["timezone"] = zone.identifier
        result["offsetMinutes"] = zone.secondsFromGMT() / 60
        result["packageName"] = bundle.bundleIdentifier
        result["versionName"] = bundle.infoDictionary?["CFBundleShortVersionString"] as? String
        result["versionCode"] = (bundle.infoDictionary?["CFBundleVersion"] as? String).flatMap { Int64($0) }
        result["sdkInt"] = 0
        #if DEBUG
        result["debuggable"] = true
        #else
        result["debuggable"] = false
        #endif
        return result
    }

    /// Window size in points (the iOS counterpart of dp) and the screen scale.
    private static func metrics(_ window: UIWindow) -> [String: Any?] {
        [
            "widthDp": Double(window.bounds.width),
            "heightDp": Double(window.bounds.height),
            "density": Double(window.screen.scale),
        ]
    }

    /// Dark mode, orientation and font scale.
    private static func configuration(_ window: UIWindow?) -> [String: Any?] {
        let traits = window?.traitCollection ?? UIScreen.main.traitCollection
        let bounds = window?.bounds ?? UIScreen.main.bounds
        let body = UIFont.preferredFont(forTextStyle: .body, compatibleWith: traits).pointSize
        return [
            "nightMode": traits.userInterfaceStyle == .dark,
            "orientation": bounds.width > bounds.height ? "landscape" : "portrait",
            // 17 pt is the body size at the default content size category.
            "fontScale": Double(body / 17),
        ]
    }

    static func keyWindow() -> UIWindow? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let active = scenes.filter { $0.activationState == .foregroundActive } + scenes
        for scene in active {
            if let window = scene.windows.first(where: \.isKeyWindow) ?? scene.windows.first {
                return window
            }
        }
        return nil
    }
}

/// Invisible, non-interactive view that tracks its window's size and trait changes.
private final class WindowProbe: UIView {
    var onResize: (() -> Void)?
    var onTraitsChange: (() -> Void)?
    private var lastSize: CGSize = .zero

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        backgroundColor = .clear
        autoresizingMask = [.flexibleWidth, .flexibleHeight]
    }

    required init?(coder: NSCoder) { nil }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.size != lastSize else { return }
        lastSize = bounds.size
        onResize?()
    }

    override func traitCollectionDidChange(_ previous: UITraitCollection?) {
        super.traitCollectionDidChange(previous)
        guard let previous else { return }
        if previous.userInterfaceStyle != traitCollection.userInterfaceStyle
            || previous.preferredContentSizeCategory != traitCollection.preferredContentSizeCategory {
            onTraitsChange?()
        }
    }
}
