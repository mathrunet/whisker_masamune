//! Native web view element for Whisker apps (`WKWebView` on iOS, `android.webkit.WebView` on
//! Android), based on `whisker-webview` with the settings an app shell around a web app needs:
//! main-frame document-start scripts, origin-restricted navigation, `window.open` hand-off,
//! camera / microphone grants, Android file uploads and render process recovery.
//!
//! See the crate README for usage and the JavaScript bridge contract.
//!
//! The native element tag is `masamune-whisker-webview:WebView`. Native sources:
//! `android/src/main/kotlin/net/mathru/whisker/webview/` and
//! `ios/Sources/MasamuneWhiskerWebview/`.

pub mod cng;

mod adapter;
mod js;
mod origin;

pub use adapter::{
    DEFAULT_MAX_DOWNLOAD_BYTES, WebViewDefaults, WebViewMasamuneAdapter, max_download_bytes,
    webview_defaults,
};
pub use js::{MESSAGE_EVENT, js_string_literal, post_message_script};
pub use origin::{is_allowed_url, normalize_origin, normalize_origins, origin_of};

use whisker::platform_module::WhiskerValue;
use whisker::prelude::*;
use whisker::{ElementRef, Signal, Style};

// Every native event carries its body under `detail`. Fields default so that a partial body
// still reaches the handler.

/// Body of the `message` event (`window.whisker.postMessage`).
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MessageEvent {
    #[serde(default)]
    pub detail: MessageDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct MessageDetail {
    #[serde(default)]
    pub data: String,
}

/// Body of the URL events (`load_start`, `load`, `external_navigation`, `open_window`).
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct UrlEvent {
    #[serde(default)]
    pub detail: UrlDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct UrlDetail {
    #[serde(default)]
    pub url: String,
}

/// Body of the `progress` event.
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ProgressEvent {
    #[serde(default)]
    pub detail: ProgressDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ProgressDetail {
    #[serde(default)]
    pub progress: f64,
}

/// Body of the `error` event.
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ErrorEvent {
    #[serde(default)]
    pub detail: ErrorDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ErrorDetail {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub code: i64,
    #[serde(default)]
    pub description: String,
}

/// A failed main-frame load, passed to `on_error`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebViewError {
    /// The URL that failed to load.
    pub url: String,
    /// Platform error code (`WebViewClient.ERROR_*` on Android, `NSURLError*` on iOS).
    pub code: i64,
    /// Human-readable description from the platform.
    pub description: String,
}

/// Imperative handle of a mounted [`MasamuneWebView`], bound through its `webview_ref` prop.
///
/// Every method is a no-op while the view is not mounted. The handle is `Copy`.
#[derive(Copy, Clone)]
pub struct MasamuneWebViewRef {
    r: ElementRef,
}

impl MasamuneWebViewRef {
    /// A new, unbound handle.
    pub fn new() -> Self {
        Self {
            r: ElementRef::new(),
        }
    }

    #[doc(hidden)]
    pub fn r(&self) -> ElementRef {
        self.r
    }

    /// Reloads the current page.
    pub fn reload(&self) {
        let _ = self.r.command("reload", WhiskerValue::Null);
    }

    /// Goes back one history entry, if any.
    pub fn go_back(&self) {
        let _ = self.r.command("goBack", WhiskerValue::Null);
    }

    /// Goes forward one history entry, if any.
    pub fn go_forward(&self) {
        let _ = self.r.command("goForward", WhiskerValue::Null);
    }

    /// Stops the current load.
    pub fn stop_loading(&self) {
        let _ = self.r.command("stopLoading", WhiskerValue::Null);
    }

    /// Delivers `data` to the page as
    /// `window.dispatchEvent(new CustomEvent('whisker:message', { detail: data }))`.
    pub fn post_message(&self, data: &str) {
        self.evaluate_javascript(&post_message_script(data));
    }

    /// Runs `script` in the main frame (fire-and-forget).
    pub fn evaluate_javascript(&self, script: &str) {
        let _ = self.r.command(
            "evaluateJavaScript",
            WhiskerValue::String(script.to_string()),
        );
    }

    /// Loads `url` in the main frame, even if it equals the `url` prop. The `url` prop is not
    /// updated; a later change of the prop loads that value again.
    pub fn load_url(&self, url: &str) {
        let _ = self
            .r
            .command("loadUrl", WhiskerValue::String(url.to_string()));
    }
}

impl Default for MasamuneWebViewRef {
    fn default() -> Self {
        Self::new()
    }
}

// Lists cross as JSON-array strings because module element properties are primitive values.
// Browser configuration is declared before the content so it is applied before the first load.
#[doc(hidden)]
#[whisker::module_element(
    name = "masamune-whisker-webview:WebView",
    measurement = None,
    commands = [
        ("reload", Null),
        ("goBack", Null),
        ("goForward", Null),
        ("stopLoading", Null),
        ("evaluateJavaScript", String),
        ("loadUrl", String),
    ],
)]
pub fn native_masamune_webview(
    user_agent: Signal<String>,
    javascript_enabled: Signal<bool>,
    grant_media_permissions: Signal<bool>,
    allowed_origins: Signal<String>,
    user_scripts: Signal<String>,
    url: Signal<String>,
    html: Signal<String>,
    style: Style,
    on_message: MessageEvent,
    on_load_start: UrlEvent,
    on_load: UrlEvent,
    on_progress: ProgressEvent,
    on_error: ErrorEvent,
    on_external_navigation: UrlEvent,
    on_open_window: UrlEvent,
    on_render_process_gone: (),
) {
}

/// Element schema referenced by the generated Host bootstrap code (Web, desktop).
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [native_masamune_webview_schema::element_provider()],
    )
}

/// Native web view.
///
/// `url` is one-way: changing the signal loads the new address, while navigations inside the
/// page are reported through `on_load_start` / `on_load` and never written back. `html` is shown
/// while `url` is empty. Unset `user_agent`, `allowed_origins` and `grant_media_permissions`
/// use the installed [`WebViewMasamuneAdapter`].
///
/// `user_scripts` run at document start in the main frame only, after `window.whisker` is
/// defined. Changes to `user_scripts` apply from the next navigation.
#[allow(clippy::too_many_arguments)]
#[component]
pub fn masamune_web_view(
    /// Address to load. Changing it navigates the view.
    url: Option<Signal<String>>,
    /// Inline HTML shown while `url` is empty.
    html: Option<Signal<String>>,
    /// User-Agent string (default: the adapter's, else the platform's).
    user_agent: Option<Signal<String>>,
    /// Allow JavaScript.
    #[prop(default = true)]
    javascript_enabled: bool,
    /// Scripts injected at document start in the main frame.
    #[prop(default = Vec::new())]
    user_scripts: Vec<String>,
    /// Origins the main frame may navigate to, such as `https://example.com` (default: the
    /// adapter's; empty allows every origin). Other `http(s)` navigations of the main frame are
    /// cancelled and reported through `on_external_navigation`.
    allowed_origins: Option<Vec<String>>,
    /// Grant camera / microphone requests of allowed origins (default: the adapter's).
    grant_media_permissions: Option<bool>,
    /// Page → Rust message (`window.whisker.postMessage(string)`).
    on_message: Option<Callback<String>>,
    /// A main-frame load started.
    on_load_start: Option<Callback<String>>,
    /// A main-frame load finished.
    on_load: Option<Callback<String>>,
    /// Load progress `0.0..=1.0`.
    on_progress: Option<Callback<f32>>,
    /// A main-frame load failed.
    on_error: Option<Callback<WebViewError>>,
    /// The main frame tried to open an `http(s)` URL outside `allowed_origins` (cancelled).
    on_external_navigation: Option<Callback<String>>,
    /// The page asked for a new window (`window.open`, `target="_blank"`). No window is created.
    on_open_window: Option<Callback<String>>,
    /// The web content process ended. The view has already reloaded the last page.
    on_render_process_gone: Option<Callback<()>>,
    /// Structured Whisker style.
    style: Option<Style>,
    /// Imperative handle.
    webview_ref: Option<MasamuneWebViewRef>,
) -> Element {
    let defaults = webview_defaults();
    let user_agent = user_agent.unwrap_or_else(|| Signal::from(defaults.user_agent.clone()));
    let allowed_origins = allowed_origins
        .as_deref()
        .map(normalize_origins)
        .unwrap_or(defaults.allowed_origins);
    let grant_media_permissions =
        grant_media_permissions.unwrap_or(defaults.grant_media_permissions);

    let mut builder = NativeMasamuneWebview::builder()
        .user_agent(user_agent)
        .javascript_enabled(javascript_enabled)
        .grant_media_permissions(grant_media_permissions)
        .allowed_origins(json_array(&allowed_origins))
        .user_scripts(json_array(&user_scripts))
        .url(url.unwrap_or_default())
        .html(html.unwrap_or_default())
        .style(style.clone().unwrap_or_default())
        .on_message(move |event: MessageEvent| {
            if let Some(cb) = &on_message {
                cb.run(event.detail.data);
            }
        })
        .on_load_start(move |event: UrlEvent| {
            if let Some(cb) = &on_load_start {
                cb.run(event.detail.url);
            }
        })
        .on_load(move |event: UrlEvent| {
            if let Some(cb) = &on_load {
                cb.run(event.detail.url);
            }
        })
        .on_progress(move |event: ProgressEvent| {
            if let Some(cb) = &on_progress {
                cb.run(event.detail.progress.clamp(0.0, 1.0) as f32);
            }
        })
        .on_error(move |event: ErrorEvent| {
            if let Some(cb) = &on_error {
                cb.run(WebViewError {
                    url: event.detail.url,
                    code: event.detail.code,
                    description: event.detail.description,
                });
            }
        })
        .on_external_navigation(move |event: UrlEvent| {
            if let Some(cb) = &on_external_navigation {
                cb.run(event.detail.url);
            }
        })
        .on_open_window(move |event: UrlEvent| {
            if let Some(cb) = &on_open_window {
                cb.run(event.detail.url);
            }
        })
        .on_render_process_gone(move || {
            if let Some(cb) = &on_render_process_gone {
                cb.run(());
            }
        });
    if let Some(handle) = webview_ref {
        builder = builder.element_ref(handle.r());
    }
    builder.build()
}

fn json_array(values: &[String]) -> String {
    serde_json::to_string(values).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_declares_commands_and_property_order() {
        let schema = native_masamune_webview_schema::schema();
        assert_eq!(schema.name, "masamune-whisker-webview:WebView");
        assert_eq!(
            schema
                .commands
                .iter()
                .map(|command| command.name.as_str())
                .collect::<Vec<_>>(),
            [
                "reload",
                "goBack",
                "goForward",
                "stopLoading",
                "evaluateJavaScript",
                "loadUrl",
            ]
        );
        assert_eq!(
            schema
                .properties
                .iter()
                .map(|property| property.name.as_str())
                .collect::<Vec<_>>(),
            [
                "user-agent",
                "javascript-enabled",
                "grant-media-permissions",
                "allowed-origins",
                "user-scripts",
                "url",
                "html",
            ]
        );
        let kind = |name: &str| {
            schema
                .properties
                .iter()
                .find(|property| property.name == name)
                .unwrap()
                .value
        };
        assert_eq!(kind("javascript-enabled"), whisker::ElementValueKind::Bool);
        assert_eq!(
            kind("grant-media-permissions"),
            whisker::ElementValueKind::Bool
        );
    }

    #[test]
    fn lists_cross_as_json_arrays() {
        assert_eq!(json_array(&[]), "[]");
        assert_eq!(
            json_array(&["a\"b".to_string(), "c\\d\n".to_string()]),
            r#"["a\"b","c\\d\n"]"#
        );
    }

    #[test]
    fn event_bodies_deserialize() {
        let message: MessageEvent = WhiskerValue::map([(
            "detail",
            WhiskerValue::map([("data", WhiskerValue::String("hi".into()))]),
        )])
        .deserialize_into()
        .unwrap();
        assert_eq!(message.detail.data, "hi");

        let error: ErrorEvent = WhiskerValue::map([(
            "detail",
            WhiskerValue::map([
                ("url", WhiskerValue::String("https://x".into())),
                ("code", WhiskerValue::Int(-1009)),
                ("description", WhiskerValue::String("offline".into())),
            ]),
        )])
        .deserialize_into()
        .unwrap();
        assert_eq!(error.detail.code, -1009);

        let empty: [(&str, WhiskerValue); 0] = [];
        let url: UrlEvent = WhiskerValue::map(empty).deserialize_into().unwrap();
        assert_eq!(url.detail.url, "");
    }
}
