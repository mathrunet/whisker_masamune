// AdMob (banners and interstitial ads) on iOS. Mirrors `AdmobModule.kt`: the SDK is initialized when the
// first ad is loaded (same as the Flutter version), banners are anchored adaptive banners sized from the
// requested width, and interstitials resolve once they are closed.

import AppTrackingTransparency
import GoogleMobileAds
import UIKit
import WhiskerModule

/// Initializes the Mobile Ads SDK once and queues callers until it is ready.
enum AdmobSdk {
    private static var initialized = false
    private static var initializing = false
    private static var waiters: [() -> Void] = []

    /// Runs `onReady` on the main thread once the SDK has been started.
    static func ensureInitialized(_ onReady: @escaping () -> Void) {
        Bridge.onMain {
            if initialized {
                onReady()
                return
            }
            waiters.append(onReady)
            guard !initializing else { return }
            initializing = true
            MobileAds.shared.start { _ in
                Bridge.onMain {
                    initialized = true
                    initializing = false
                    NSLog("MasamuneWhiskerAdmob: SDK initialized")
                    let pending = waiters
                    waiters.removeAll()
                    pending.forEach { $0() }
                }
            }
        }
    }

    /// Size of an anchored adaptive banner sized to `width` points (main thread only).
    static func bannerSize(width: Int) -> AdSize {
        largeAnchoredAdaptiveBanner(width: CGFloat(width))
    }

    /// The window the app is presenting in (the foreground key window when there is one).
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

    /// The top-most presented view controller of the key window.
    static func topViewController(from window: UIWindow? = keyWindow()) -> UIViewController? {
        var controller = window?.rootViewController
        while let presented = controller?.presentedViewController, !presented.isBeingDismissed {
            controller = presented
        }
        return controller
    }
}

/// Keeps a loaded interstitial alive until it is dismissed and settles the promise.
private final class InterstitialPresenter: NSObject, FullScreenContentDelegate {
    private let promise: WhiskerPromise
    private let onFinish: (InterstitialPresenter) -> Void
    var ad: InterstitialAd?

    init(promise: WhiskerPromise, onFinish: @escaping (InterstitialPresenter) -> Void) {
        self.promise = promise
        self.onFinish = onFinish
    }

    func adDidDismissFullScreenContent(_ ad: FullScreenPresentingAd) {
        Bridge.resolve(promise, true)
        onFinish(self)
    }

    func ad(_ ad: FullScreenPresentingAd, didFailToPresentFullScreenContentWithError error: Error) {
        Bridge.reject(promise, error)
        onFinish(self)
    }
}

@WhiskerModule
public final class AdmobModule: Module {
    private var presenters: [InterstitialPresenter] = []

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Admob")

            // bannerHeight({ widthDp }) -> height (points)
            Function("bannerHeight") { args in
                let width = (Bridge.args(args)["widthDp"] as? NSNumber)?.intValue ?? 360
                return Bridge.ok(Int(AdmobSdk.bannerSize(width: width).size.height))
            }

            // showInterstitial({ adUnitId }) — loads and shows the ad, resolving once it is closed.
            AsyncFunction("showInterstitial") { [weak self] args, promise in
                let adUnitId = Bridge.args(args)["adUnitId"] as? String ?? ""
                AdmobSdk.ensureInitialized {
                    self?.showInterstitial(adUnitId: adUnitId, promise: promise)
                }
            }

            // requestTrackingAuthorization() -> "notDetermined" | "restricted" | "denied" | "authorized"
            AsyncFunction("requestTrackingAuthorization") { _, promise in
                Bridge.onMain { TrackingAuthorization.request(promise) }
            }

            View("masamune-whisker-admob:Banner", AdBannerView.self) {
                Prop("ad-unit-id") { (view: AdBannerView, value: WhiskerValue) in
                    view.setAdUnit(value.asString ?? "")
                }
                Prop("width-dp") { (view: AdBannerView, value: WhiskerValue) in
                    view.setWidth(value.asString.flatMap { Int($0) } ?? value.asInt.map { Int($0) } ?? 0)
                }
                Events("loaded", "failed")
            }
        }
    }

    private func showInterstitial(adUnitId: String, promise: WhiskerPromise) {
        InterstitialAd.load(with: adUnitId, request: Request()) { [weak self] ad, error in
            Bridge.onMain {
                guard let ad, error == nil else {
                    // Treat no fill as an expected outcome.
                    Bridge.resolve(promise, false)
                    return
                }
                guard let self, let controller = AdmobSdk.topViewController() else {
                    Bridge.reject(promise, "no view controller")
                    return
                }
                let presenter = InterstitialPresenter(promise: promise) { [weak self] finished in
                    self?.presenters.removeAll { $0 === finished }
                }
                presenter.ad = ad
                ad.fullScreenContentDelegate = presenter
                self.presenters.append(presenter)
                ad.present(from: controller)
            }
        }
    }
}

/// App Tracking Transparency prompt. The system only shows it while the app is active, so a request made
/// during launch waits for the app to become active first.
private enum TrackingAuthorization {
    private static var observer: NSObjectProtocol?
    private static var pending: [WhiskerPromise] = []

    static func request(_ promise: WhiskerPromise) {
        guard Bundle.main.object(forInfoDictionaryKey: "NSUserTrackingUsageDescription") != nil else {
            Bridge.reject(promise, "NSUserTrackingUsageDescription is missing from Info.plist")
            return
        }
        let current = ATTrackingManager.trackingAuthorizationStatus
        if current != .notDetermined {
            Bridge.resolve(promise, name(current))
            return
        }
        pending.append(promise)
        if UIApplication.shared.applicationState == .active {
            prompt()
        } else if observer == nil {
            observer = NotificationCenter.default.addObserver(
                forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
            ) { _ in
                if let observer { NotificationCenter.default.removeObserver(observer) }
                observer = nil
                prompt()
            }
        }
    }

    private static func prompt() {
        ATTrackingManager.requestTrackingAuthorization { status in
            Bridge.onMain {
                let waiting = pending
                pending.removeAll()
                waiting.forEach { Bridge.resolve($0, name(status)) }
            }
        }
    }

    static func name(_ status: ATTrackingManager.AuthorizationStatus) -> String {
        switch status {
        case .authorized: return "authorized"
        case .denied: return "denied"
        case .restricted: return "restricted"
        case .notDetermined: return "notDetermined"
        @unknown default: return "notDetermined"
        }
    }
}

/// Banner ad element. Loads an anchored adaptive banner at `width-dp` points and emits `loaded` or
/// `failed` (`{message}`), retrying failed loads with the same back-off as Android.
public final class AdBannerView: WhiskerUI<UIView>, BannerViewDelegate {
    private let lifecycle = BannerLifecycle()
    private var banner: BannerView?
    private var retryTask: DispatchWorkItem?

    public override func createView() -> UIView {
        let view = UIView()
        view.backgroundColor = .clear
        return view
    }

    func setAdUnit(_ value: String) {
        if lifecycle.configure(adUnitId: value) { restart() }
    }

    func setWidth(_ value: Int) {
        if lifecycle.configure(widthDp: value) { restart() }
    }

    public override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil {
            lifecycle.attach()
            restart()
        } else {
            lifecycle.detach()
            cancelRetry()
            releaseAd()
        }
    }

    private func cancelRetry() {
        retryTask?.cancel()
        retryTask = nil
    }

    private func releaseAd() {
        banner?.delegate = nil
        banner?.removeFromSuperview()
        banner = nil
    }

    private func restart() {
        cancelRetry()
        releaseAd()
        load()
    }

    private func load() {
        guard let token = lifecycle.beginLoad() else { return }
        AdmobSdk.ensureInitialized { [weak self] in
            guard let self, self.lifecycle.isCurrent(token) else { return }
            let ad = BannerView(adSize: AdmobSdk.bannerSize(width: self.lifecycle.widthDp))
            ad.adUnitID = self.lifecycle.adUnitId
            ad.rootViewController = AdmobSdk.topViewController(from: self.window ?? AdmobSdk.keyWindow())
            ad.delegate = self
            ad.frame = self.view().bounds
            ad.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            self.banner = ad
            self.view().addSubview(ad)
            ad.load(Request())
        }
    }

    public func bannerViewDidReceiveAd(_ bannerView: BannerView) {
        guard bannerView === banner, let token = lifecycle.token, lifecycle.loaded(token) else { return }
        cancelRetry()
        emitWhiskerEvent("loaded")
    }

    public func bannerView(_ bannerView: BannerView, didFailToReceiveAdWithError error: Error) {
        guard bannerView === banner, let token = lifecycle.token, lifecycle.isCurrent(token) else { return }
        failed(token, message: Bridge.message(error), code: (error as NSError).code)
    }

    private func failed(_ token: Int64, message: String, code: Int?) {
        guard lifecycle.isCurrent(token) else { return }
        let retryDelay = lifecycle.failed(token)
        releaseAd()
        NSLog("MasamuneWhiskerAdmob: banner failed: code=\(code.map(String.init) ?? "nil"), retryMs=\(retryDelay.map(String.init) ?? "nil"), message=\(message)")
        emitWhiskerEvent("failed", detail: .map(["message": .string(message)]))
        if let retryDelay, lifecycle.isCurrent(token) {
            cancelRetry()
            let task = DispatchWorkItem { [weak self] in
                guard let self else { return }
                self.retryTask = nil
                if self.lifecycle.isCurrent(token) { self.load() }
            }
            retryTask = task
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(Int(retryDelay)), execute: task)
        }
    }
}

/// Manages native View reattachment and ad load generations and retries (port of `BannerLifecycle.kt`).
final class BannerLifecycle {
    private(set) var adUnitId = ""
    private(set) var widthDp = 0
    private var attached = false
    private var generation: Int64 = 0
    private var retryCount = 0
    private var loading = false
    private var hasAd = false

    /// The generation of the load in flight (or of the loaded ad).
    var token: Int64? { attached && (loading || hasAd) ? generation : nil }

    func configure(adUnitId: String? = nil, widthDp: Int? = nil) -> Bool {
        let unit = adUnitId ?? self.adUnitId
        let width = widthDp ?? self.widthDp
        if self.adUnitId == unit && self.widthDp == width { return false }
        self.adUnitId = unit
        self.widthDp = width
        invalidate()
        return true
    }

    func attach() {
        attached = true
        invalidate()
    }

    func detach() {
        attached = false
        invalidate()
    }

    private func invalidate() {
        generation += 1
        retryCount = 0
        loading = false
        hasAd = false
    }

    func beginLoad() -> Int64? {
        if !attached || adUnitId.trimmingCharacters(in: .whitespaces).isEmpty || widthDp <= 0 || loading { return nil }
        loading = true
        hasAd = false
        generation += 1
        return generation
    }

    func isCurrent(_ token: Int64) -> Bool { attached && generation == token }

    func loaded(_ token: Int64) -> Bool {
        if !isCurrent(token) || !loading { return false }
        loading = false
        hasAd = true
        retryCount = 0
        return true
    }

    /// Returns the retry delay in milliseconds, or `nil` when no retry should be scheduled.
    func failed(_ token: Int64) -> Int64? {
        if !isCurrent(token) || (!loading && !hasAd) { return nil }
        loading = false
        hasAd = false
        if retryCount >= 3 { return nil }
        let delay = Int64(5_000) << Int64(retryCount)
        retryCount += 1
        return delay
    }
}
