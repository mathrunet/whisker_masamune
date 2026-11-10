// YouTube player element of masamune-whisker-youtube (iOS), registered by YouTubePlayerModule.
//
// A WKWebView loads the player page built in Rust (`player_html`) with the `origin` prop as its
// base URL. The page reports `{"event":"ready"|"state"|"error", ...}` through the
// `masamuneWhiskerYoutube` script message handler and is driven through `window.whiskerYoutube`.
//
// The video requested before the player is ready is kept and cued (or loaded) on `ready`, and
// again after the page reloads (origin change, content process termination).
//
// The script message handler is registered through a weak proxy: WKUserContentController retains
// its handlers, and registering `self` would keep the view alive forever.

import Foundation
import UIKit
import WebKit
import WhiskerModule

private final class WeakYouTubeMessageProxy: NSObject, WKScriptMessageHandler {
    weak var target: YouTubePlayerView?
    init(target: YouTubePlayerView) { self.target = target }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        target?.handleScriptMessage(message)
    }
}

@objc(MasamuneWhiskerYouTubePlayerView)
public final class YouTubePlayerView: WhiskerUI<UIView> {
    private static let handlerName = "masamuneWhiskerYoutube"
    private static let defaultOrigin = "https://www.youtube-nocookie.com"

    private lazy var containerView: UIView = {
        let view = UIView()
        view.backgroundColor = .black
        view.clipsToBounds = true
        return view
    }()

    private var webView: WKWebView?

    private var origin = YouTubePlayerView.defaultOrigin
    private var html = ""
    private var autoplay = false
    private var loopVideo = false
    /// Last value of the `video-id` prop.
    private var videoIdProp = ""
    /// Video shown by the player (re-requested after a page reload).
    private var currentVideoId = ""
    /// Video waiting for the player to become ready, and whether to load (play) it.
    private var pendingVideoId: String?
    private var pendingLoad = false
    private var ready = false

    @objc public override func createView() -> UIView {
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        config.mediaTypesRequiringUserActionForPlayback = []
        config.allowsPictureInPictureMediaPlayback = false
        config.userContentController.add(WeakYouTubeMessageProxy(target: self), name: Self.handlerName)

        let wv = WKWebView(frame: .zero, configuration: config)
        wv.navigationDelegate = self
        wv.uiDelegate = self
        wv.allowsLinkPreview = false
        // Black behind the page and before the first paint (no white flash while loading).
        wv.isOpaque = false
        wv.backgroundColor = .black
        wv.scrollView.backgroundColor = .black
        wv.scrollView.isScrollEnabled = false
        wv.scrollView.bounces = false
        wv.scrollView.contentInsetAdjustmentBehavior = .never
        webView = wv

        wv.frame = containerView.bounds
        wv.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        containerView.addSubview(wv)

        // Props can arrive before createView(); replay the page.
        loadPage()
        return containerView
    }

    deinit {
        webView?.stopLoading()
        webView?.navigationDelegate = nil
        webView?.uiDelegate = nil
        webView?.configuration.userContentController.removeScriptMessageHandler(forName: Self.handlerName)
    }

    // MARK: - Props

    func setOrigin(_ value: String) {
        origin = value.isEmpty ? Self.defaultOrigin : value
    }

    func setPlayerHtml(_ value: String) {
        guard value != html else { return }
        html = value
        loadPage()
    }

    func setAutoplay(_ value: Bool) {
        autoplay = value
    }

    func setLoopVideo(_ value: Bool) {
        loopVideo = value
        if ready { evaluate("window.whiskerYoutube.setLoop(\(value ? "true" : "false"));") }
    }

    /// The first video is cued (or loaded with autoplay); later changes load the new video.
    func setVideoId(_ value: String) {
        let id = Self.sanitize(value)
        guard !id.isEmpty, id != videoIdProp else { return }
        let first = currentVideoId.isEmpty && pendingVideoId == nil
        videoIdProp = id
        request(id, load: autoplay || !first)
    }

    // MARK: - Commands

    func play() { call("play()") }

    func pause() { call("pause()") }

    func cueVideo(_ value: String) {
        let id = Self.sanitize(value)
        guard !id.isEmpty else { return }
        request(id, load: false)
    }

    func loadVideo(_ value: String) {
        let id = Self.sanitize(value)
        guard !id.isEmpty else { return }
        request(id, load: true)
    }

    func seek(_ seconds: Double) {
        let value = seconds.isFinite ? max(0, seconds) : 0
        call("seek(\(value))")
    }

    func mute() { call("mute()") }

    func unmute() { call("unmute()") }

    // MARK: - Player

    private func loadPage() {
        guard let webView, !html.isEmpty else { return }
        ready = false
        // Re-request the shown video once the new page is ready.
        if pendingVideoId == nil, !currentVideoId.isEmpty {
            pendingVideoId = currentVideoId
            pendingLoad = autoplay
        }
        webView.loadHTMLString(html, baseURL: URL(string: origin) ?? URL(string: Self.defaultOrigin))
    }

    private func request(_ id: String, load: Bool) {
        pendingVideoId = id
        pendingLoad = load
        flushPendingVideo()
    }

    private func flushPendingVideo() {
        guard ready, let id = pendingVideoId else { return }
        pendingVideoId = nil
        currentVideoId = id
        evaluate("window.whiskerYoutube.\(pendingLoad ? "load" : "cue")('\(id)');")
    }

    private func call(_ expression: String) {
        guard ready else { return }
        evaluate("window.whiskerYoutube.\(expression);")
    }

    private func evaluate(_ script: String) {
        webView?.evaluateJavaScript(script, completionHandler: nil)
    }

    /// Keeps only `A-Z a-z 0-9 - _`, so the ID is safe inside a JavaScript string literal.
    private static func sanitize(_ value: String) -> String {
        String(value.unicodeScalars.filter { scalar in
            switch scalar {
            case "a"..."z", "A"..."Z", "0"..."9", "-", "_": return true
            default: return false
            }
        }.map(Character.init))
    }

    // MARK: - Bridge

    func handleScriptMessage(_ message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame,
              let body = message.body as? String,
              let data = body.data(using: .utf8),
              let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any],
              let event = json["event"] as? String
        else { return }
        switch event {
        case "ready":
            ready = true
            if loopVideo { evaluate("window.whiskerYoutube.setLoop(true);") }
            flushPendingVideo()
            emit("ready")
        case "state":
            emit("state_change", ["state": (json["state"] as? NSNumber)?.intValue ?? -1])
        case "error":
            emit("error", ["code": (json["code"] as? NSNumber)?.intValue ?? -1])
        default:
            break
        }
    }

    private func emit(_ name: String, _ params: [String: Any] = [:]) {
        WhiskerCustomEvent.dispatch(from: self, name: name, params: params)
    }

    /// Opens `url` in the system (YouTube app or browser).
    private func openExternally(_ url: URL?) {
        guard let url, let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else { return }
        UIApplication.shared.open(url)
    }
}

// MARK: - WKNavigationDelegate

extension YouTubePlayerView: WKNavigationDelegate {
    public func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        guard let frame = navigationAction.targetFrame else {
            // New window (`target="_blank"`, "Watch on YouTube").
            openExternally(navigationAction.request.url)
            decisionHandler(.cancel)
            return
        }
        if !frame.isMainFrame {
            // The YouTube player iframe.
            decisionHandler(.allow)
            return
        }
        let url = navigationAction.request.url
        let scheme = url?.scheme?.lowercased() ?? ""
        if scheme == "about" || (navigationAction.navigationType == .other && Self.sameOrigin(url, origin)) {
            // The player page itself (`loadHTMLString` with the origin as base URL).
            decisionHandler(.allow)
            return
        }
        openExternally(url)
        decisionHandler(.cancel)
    }

    public func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        loadPage()
    }

    private static func sameOrigin(_ url: URL?, _ origin: String) -> Bool {
        guard let url, let base = URL(string: origin) else { return false }
        return url.scheme?.lowercased() == base.scheme?.lowercased()
            && url.host?.lowercased() == base.host?.lowercased()
            && url.port == base.port
    }
}

// MARK: - WKUIDelegate

extension YouTubePlayerView: WKUIDelegate {
    /// No new web views: links that ask for a window open in the system instead.
    public func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        nil
    }
}
