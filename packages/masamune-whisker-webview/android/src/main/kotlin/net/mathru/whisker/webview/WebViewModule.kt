// Module definition of masamune-whisker-webview (Android). KSP registers the
// `masamune-whisker-webview:WebView` element; the view lives in MasamuneWebView.kt.

package net.mathru.whisker.webview

import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class WebViewModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("WebView")
        View("masamune-whisker-webview:WebView", MasamuneWebView::class.java) {
            // Browser configuration (declared before the content in the Rust schema).
            Prop("user-agent", clear = { view: MasamuneWebView -> view.setUserAgent("") }) { view: MasamuneWebView, value ->
                view.setUserAgent(value.asString() ?: "")
            }
            Prop("javascript-enabled", clear = { view: MasamuneWebView -> view.setJavascriptEnabled(true) }) { view: MasamuneWebView, value ->
                view.setJavascriptEnabled(value.asBool() ?: true)
            }
            Prop("grant-media-permissions", clear = { view: MasamuneWebView -> view.setGrantMediaPermissions(false) }) { view: MasamuneWebView, value ->
                view.setGrantMediaPermissions(value.asBool() ?: false)
            }
            // JSON array strings.
            Prop("allowed-origins", clear = { view: MasamuneWebView -> view.setAllowedOrigins("[]") }) { view: MasamuneWebView, value ->
                view.setAllowedOrigins(value.asString() ?: "[]")
            }
            Prop("user-scripts", clear = { view: MasamuneWebView -> view.setUserScripts("[]") }) { view: MasamuneWebView, value ->
                view.setUserScripts(value.asString() ?: "[]")
            }

            // Content.
            Prop("url", clear = { view: MasamuneWebView -> view.setUrl("") }) { view: MasamuneWebView, value ->
                view.setUrl(value.asString() ?: "")
            }
            Prop("html", clear = { view: MasamuneWebView -> view.setHtml("") }) { view: MasamuneWebView, value ->
                view.setHtml(value.asString() ?: "")
            }

            Events(
                "message",
                "load_start",
                "load",
                "progress",
                "error",
                "external_navigation",
                "open_window",
                "render_process_gone",
            )

            Command("reload") { view: MasamuneWebView, _ -> view.reload() }
            Command("goBack") { view: MasamuneWebView, _ -> view.goBack() }
            Command("goForward") { view: MasamuneWebView, _ -> view.goForward() }
            Command("stopLoading") { view: MasamuneWebView, _ -> view.stopLoading() }
            Command("evaluateJavaScript") { view: MasamuneWebView, parameters ->
                view.evaluateJavascript(parameters.asString() ?: "")
            }
            Command("loadUrl") { view: MasamuneWebView, parameters ->
                view.loadUrlCommand(parameters.asString() ?: "")
            }
        }
    }
}
