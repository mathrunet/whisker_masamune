// Web view element of masamune-whisker-webview (iOS), registered by WebViewModule.
//
// JS bridge: `window.whisker` is defined at document start in the main frame. Page -> Rust messages
// arrive through the `masamuneWhisker` script message handler and are accepted only from the main
// frame of an allowed origin. Rust -> page scripts are built in Rust and run with
// evaluateJavaScript.
//
// The script message handler is registered through a weak proxy: WKUserContentController retains
// its handlers, and registering `self` would keep the view alive forever.

import Foundation
import UIKit
import WebKit
import WhiskerModule

private final class WeakScriptMessageProxy: NSObject, WKScriptMessageHandler {
    weak var target: MasamuneWebView?
    init(target: MasamuneWebView) { self.target = target }

    func userContentController(
        _ userContentController: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        target?.handleScriptMessage(message)
    }
}

@objc(MasamuneWhiskerWebView)
public final class MasamuneWebView: WhiskerUI<UIView> {
    private static let handlerName = "masamuneWhisker"

    /// Defines `window.whisker` before any page script runs.
    private static let bridgeShim = """
    (function() {
      var whisker = window.whisker || {};
      whisker.postMessage = function(data) {
        var s = (typeof data === 'string') ? data : JSON.stringify(data);
        window.webkit.messageHandlers.masamuneWhisker.postMessage(String(s));
      };
      whisker.isMasamuneWebView = true;
      window.whisker = whisker;
    })();
    """

    /// Disables pinch zoom through the viewport (honoured because `ignoresViewportScaleLimits`
    /// stays false). Pages without a viewport get the device width, like Android's default.
    private static let disableZoomScript = #"""
    (function() {
      var meta = document.querySelector('meta[name="viewport"]');
      if (!meta) {
        meta = document.createElement('meta');
        meta.name = 'viewport';
        meta.content = 'width=device-width, initial-scale=1';
        (document.head || document.documentElement).appendChild(meta);
      }
      var content = meta.content || '';
      if (!/user-scalable\s*=\s*(no|0)/i.test(content)) {
        meta.content = content + (content ? ', ' : '') + 'maximum-scale=1, user-scalable=no';
      }
    })();
    """#

    private lazy var containerView: UIView = {
        let view = UIView()
        view.backgroundColor = .clear
        return view
    }()

    private var webView: WKWebView?
    private var progressObservation: NSKeyValueObservation?

    private var userAgent = ""
    private var javascriptEnabled = true
    private var grantMediaPermissions = false
    private var allowedOrigins: [String] = []
    private var userScripts: [String] = []

    /// Last value of the `url` prop that was loaded (empty while showing `html`).
    private var requestedUrl = ""
    private var html = ""
    /// Last committed main-frame URL, reloaded after the content process ends.
    private var currentUrl: URL?

    @objc public override func createView() -> UIView {
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        config.mediaTypesRequiringUserActionForPlayback = []
        config.preferences.javaScriptCanOpenWindowsAutomatically = true
        config.defaultWebpagePreferences.allowsContentJavaScript = javascriptEnabled
        config.userContentController.add(WeakScriptMessageProxy(target: self), name: Self.handlerName)

        let wv = WKWebView(frame: .zero, configuration: config)
        wv.navigationDelegate = self
        wv.uiDelegate = self
        wv.allowsBackForwardNavigationGestures = true
        wv.allowsLinkPreview = true
        wv.customUserAgent = userAgent.isEmpty ? nil : userAgent
        wv.scrollView.bouncesZoom = false
        webView = wv
        installUserScripts()

        progressObservation = wv.observe(\.estimatedProgress, options: [.new]) { [weak self] _, change in
            guard let self, let progress = change.newValue else { return }
            self.emit("progress", ["progress": progress])
        }

        wv.frame = containerView.bounds
        wv.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        containerView.addSubview(wv)

        // Props can arrive before createView(); replay the content.
        if !requestedUrl.isEmpty {
            load(requestedUrl)
        } else if !html.isEmpty {
            loadInline()
        }
        return containerView
    }

    deinit {
        progressObservation = nil
        webView?.stopLoading()
        webView?.navigationDelegate = nil
        webView?.uiDelegate = nil
        webView?.configuration.userContentController.removeScriptMessageHandler(forName: Self.handlerName)
        webView?.configuration.userContentController.removeAllUserScripts()
    }

    // MARK: - Props

    func setUserAgent(_ value: String) {
        userAgent = value
        webView?.customUserAgent = value.isEmpty ? nil : value
    }

    func setJavascriptEnabled(_ enabled: Bool) {
        javascriptEnabled = enabled
        webView?.configuration.defaultWebpagePreferences.allowsContentJavaScript = enabled
    }

    func setGrantMediaPermissions(_ grant: Bool) {
        grantMediaPermissions = grant
    }

    /// Malformed JSON keeps the previous list, so a bad value never opens navigation up.
    func setAllowedOrigins(_ json: String) {
        guard let origins = Self.parseStringArray(json) else { return }
        allowedOrigins = origins
    }

    func setUserScripts(_ json: String) {
        guard let scripts = Self.parseStringArray(json), scripts != userScripts else { return }
        userScripts = scripts
        installUserScripts()
    }

    func setUrl(_ value: String) {
        if value.isEmpty {
            if !requestedUrl.isEmpty {
                requestedUrl = ""
                loadInline()
            }
            return
        }
        guard value != requestedUrl else { return }
        requestedUrl = value
        load(value)
    }

    func setHtml(_ value: String) {
        html = value
        if requestedUrl.isEmpty { loadInline() }
    }

    // MARK: - Commands

    func reloadPage() { webView?.reload() }

    func goBackPage() { webView?.goBack() }

    func goForwardPage() { webView?.goForward() }

    func stopLoadingPage() { webView?.stopLoading() }

    func evaluateScript(_ script: String) {
        guard !script.isEmpty else { return }
        webView?.evaluateJavaScript(script, completionHandler: nil)
    }

    func loadUrlCommand(_ url: String) { load(url) }

    // MARK: - Loading

    private func load(_ string: String) {
        guard let url = URL(string: string) else { return }
        let scheme = url.scheme?.lowercased() ?? ""
        guard scheme == "http" || scheme == "https" || string == "about:blank" else { return }
        webView?.load(URLRequest(url: url))
    }

    private func loadInline() {
        webView?.loadHTMLString(html, baseURL: nil)
    }

    /// Scripts apply from the next navigation.
    private func installUserScripts() {
        guard let controller = webView?.configuration.userContentController else { return }
        controller.removeAllUserScripts()
        controller.addUserScript(WKUserScript(source: Self.bridgeShim, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        controller.addUserScript(WKUserScript(source: Self.disableZoomScript, injectionTime: .atDocumentEnd, forMainFrameOnly: true))
        for script in userScripts {
            controller.addUserScript(WKUserScript(source: script, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        }
    }

    // MARK: - Policy

    /// `scheme://host[:port]` with default ports omitted, or nil for non-web URLs.
    private static func origin(scheme: String?, host: String?, port: Int?) -> String? {
        let scheme = scheme?.lowercased() ?? ""
        let defaultPort: Int
        switch scheme {
        case "http": defaultPort = 80
        case "https": defaultPort = 443
        default: return nil
        }
        guard var host = host?.lowercased(), !host.isEmpty else { return nil }
        // Foundation drops the brackets of IPv6 literals; the origin keeps them.
        if host.contains(":") && !host.hasPrefix("[") { host = "[\(host)]" }
        if let port, port != 0, port != defaultPort { return "\(scheme)://\(host):\(port)" }
        return "\(scheme)://\(host)"
    }

    private static func origin(of url: URL?) -> String? {
        guard let url else { return nil }
        return origin(scheme: url.scheme, host: url.host, port: url.port)
    }

    private static func origin(of securityOrigin: WKSecurityOrigin) -> String? {
        origin(scheme: securityOrigin.protocol, host: securityOrigin.host, port: securityOrigin.port)
    }

    private func isAllowedOrigin(_ origin: String?) -> Bool {
        guard let origin else { return false }
        return allowedOrigins.isEmpty || allowedOrigins.contains(origin)
    }

    private static func parseStringArray(_ json: String) -> [String]? {
        guard let data = json.data(using: .utf8),
              let array = try? JSONSerialization.jsonObject(with: data) as? [String]
        else { return nil }
        return array
    }

    // MARK: - Bridge

    func handleScriptMessage(_ message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame else { return }
        let origin = message.frameInfo.securityOrigin
        let accepted: Bool
        if let webOrigin = Self.origin(of: origin) {
            accepted = isAllowedOrigin(webOrigin)
        } else {
            // Inline `html` page (opaque origin).
            accepted = requestedUrl.isEmpty
        }
        guard accepted else { return }
        let data = message.body as? String ?? ""
        emit("message", ["data": data])
    }

    private func emit(_ name: String, _ params: [String: Any] = [:]) {
        WhiskerCustomEvent.dispatch(from: self, name: name, params: params)
    }
}

// MARK: - WKNavigationDelegate

extension MasamuneWebView: WKNavigationDelegate {
    public func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        // No target frame: a new window, handed to Rust by createWebViewWith.
        guard let frame = navigationAction.targetFrame else {
            decisionHandler(.allow)
            return
        }
        let url = navigationAction.request.url
        switch url?.scheme?.lowercased() ?? "" {
        case "http", "https":
            if isAllowedOrigin(Self.origin(of: url)) {
                decisionHandler(.allow)
                return
            }
            if frame.isMainFrame, let url {
                emit("external_navigation", ["url": url.absoluteString])
            }
            decisionHandler(.cancel)
        case "about":
            // `loadHTMLString` and `srcdoc` documents.
            decisionHandler(.allow)
        default:
            // file:, javascript:, data:, blob:, custom schemes.
            decisionHandler(.cancel)
        }
    }

    public func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
        emit("load_start", ["url": webView.url?.absoluteString ?? ""])
    }

    public func webView(_ webView: WKWebView, didCommit navigation: WKNavigation!) {
        if let url = webView.url { currentUrl = url }
    }

    public func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        if let url = webView.url { currentUrl = url }
        emit("load", ["url": webView.url?.absoluteString ?? ""])
    }

    public func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        emitError(webView, error)
    }

    public func webView(
        _ webView: WKWebView,
        didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: Error
    ) {
        emitError(webView, error)
    }

    public func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        if let url = currentUrl ?? URL(string: requestedUrl), url.scheme?.lowercased().hasPrefix("http") == true {
            webView.load(URLRequest(url: url))
        } else if !html.isEmpty {
            loadInline()
        } else {
            webView.reload()
        }
        emit("render_process_gone")
    }

    private func emitError(_ webView: WKWebView, _ error: Error) {
        let nsError = error as NSError
        // A newer navigation replaced this one, or a policy decision cancelled it.
        if nsError.domain == NSURLErrorDomain && nsError.code == NSURLErrorCancelled { return }
        if nsError.domain == "WebKitErrorDomain" && nsError.code == 102 { return }
        let failingUrl = (nsError.userInfo[NSURLErrorFailingURLStringErrorKey] as? String)
            ?? webView.url?.absoluteString
            ?? ""
        emit("error", [
            "url": failingUrl,
            "code": nsError.code,
            "description": nsError.localizedDescription,
        ])
    }
}

// MARK: - WKUIDelegate

extension MasamuneWebView: WKUIDelegate {
    /// `window.open` / `target="_blank"`: reported to Rust, no window is created.
    public func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        if let url = navigationAction.request.url?.absoluteString, !url.isEmpty, url != "about:blank" {
            emit("open_window", ["url": url])
        }
        return nil
    }

    public func webView(
        _ webView: WKWebView,
        requestMediaCapturePermissionFor origin: WKSecurityOrigin,
        initiatedByFrame frame: WKFrameInfo,
        type: WKMediaCaptureType,
        decisionHandler: @escaping (WKPermissionDecision) -> Void
    ) {
        guard isAllowedOrigin(Self.origin(of: origin)) else {
            decisionHandler(.deny)
            return
        }
        decisionHandler(grantMediaPermissions ? .grant : .prompt)
    }
}
