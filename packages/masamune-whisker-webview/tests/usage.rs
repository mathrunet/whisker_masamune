//! Compile check of the README usage (not run).

use masamune_whisker_webview::{MasamuneWebView, MasamuneWebViewRef, WebViewError};
use whisker::prelude::*;

#[allow(dead_code)]
#[component]
fn browser() -> Element {
    let url = RwSignal::new("https://example.com/app/".to_string());
    let webview = MasamuneWebViewRef::new();
    render! {
        MasamuneWebView(
            url: url,
            webview_ref: webview,
            allowed_origins: vec!["https://example.com".to_string()],
            user_scripts: vec!["window.appShell = true;".to_string()],
            grant_media_permissions: true,
            on_message: move |message: String| {
                webview.post_message(&format!("echo:{message}"));
            },
            on_error: move |_error: WebViewError| {},
            on_progress: move |_progress: f32| {},
            on_external_navigation: move |_url: String| {},
            on_open_window: move |url: String| webview.load_url(&url),
            on_render_process_gone: move |_| {},
            style: css!(flex_grow: 1.0),
        )
    }
}

#[test]
fn usage_compiles() {}
