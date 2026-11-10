// Module definition of masamune-whisker-webview (iOS). The codegen plugin registers the
// `masamune-whisker-webview:WebView` element; the view lives in MasamuneWebView.swift.

import WhiskerModule

@WhiskerModule
public final class WebViewModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("WebView")
            View("masamune-whisker-webview:WebView", MasamuneWebView.self) {
                // Browser configuration (declared before the content in the Rust schema).
                Prop("user-agent", clear: { (view: MasamuneWebView) in view.setUserAgent("") }) { (view: MasamuneWebView, value: WhiskerValue) in
                    view.setUserAgent(value.asString ?? "")
                }
                Prop("javascript-enabled", clear: { (view: MasamuneWebView) in view.setJavascriptEnabled(true) }) { (view: MasamuneWebView, value: WhiskerValue) in
                    view.setJavascriptEnabled(value.asBool ?? true)
                }
                Prop("grant-media-permissions", clear: { (view: MasamuneWebView) in view.setGrantMediaPermissions(false) }) { (view: MasamuneWebView, value: WhiskerValue) in
                    view.setGrantMediaPermissions(value.asBool ?? false)
                }
                // JSON array strings.
                Prop("allowed-origins", clear: { (view: MasamuneWebView) in view.setAllowedOrigins("[]") }) { (view: MasamuneWebView, value: WhiskerValue) in
                    view.setAllowedOrigins(value.asString ?? "[]")
                }
                Prop("user-scripts", clear: { (view: MasamuneWebView) in view.setUserScripts("[]") }) { (view: MasamuneWebView, value: WhiskerValue) in
                    view.setUserScripts(value.asString ?? "[]")
                }

                // Content.
                Prop("url", clear: { (view: MasamuneWebView) in view.setUrl("") }) { (view: MasamuneWebView, value: WhiskerValue) in
                    view.setUrl(value.asString ?? "")
                }
                Prop("html", clear: { (view: MasamuneWebView) in view.setHtml("") }) { (view: MasamuneWebView, value: WhiskerValue) in
                    view.setHtml(value.asString ?? "")
                }

                Events(
                    "message",
                    "load_start",
                    "load",
                    "progress",
                    "error",
                    "external_navigation",
                    "open_window",
                    "render_process_gone"
                )

                Command("reload") { (view: MasamuneWebView, _: WhiskerValue) in view.reloadPage() }
                Command("goBack") { (view: MasamuneWebView, _: WhiskerValue) in view.goBackPage() }
                Command("goForward") { (view: MasamuneWebView, _: WhiskerValue) in view.goForwardPage() }
                Command("stopLoading") { (view: MasamuneWebView, _: WhiskerValue) in view.stopLoadingPage() }
                Command("evaluateJavaScript") { (view: MasamuneWebView, parameters: WhiskerValue) in
                    view.evaluateScript(parameters.asString ?? "")
                }
                Command("loadUrl") { (view: MasamuneWebView, parameters: WhiskerValue) in
                    view.loadUrlCommand(parameters.asString ?? "")
                }
            }
        }
    }
}
