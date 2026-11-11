# masamune-whisker-webview

Native web view element for [Whisker](https://whisker.rs) apps: `WKWebView` on iOS (15 or later)
and `android.webkit.WebView` on Android. Based on `whisker-webview`, with what an app shell around
a web app needs:

- scripts injected at document start in the main frame (`user_scripts`)
- navigation restricted to a list of origins; other web links are handed to the app
- `window.open` / `target="_blank"` handed to the app instead of opening a window
- camera / microphone grants for `getUserMedia`
- `<input type="file">` on Android (iOS handles it natively)
- recovery when the web content process ends

## Usage

```rust
use masamune_whisker_webview::{MasamuneWebView, MasamuneWebViewRef};
use whisker::prelude::*;

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
            on_message: move |message: String| {
                // Page -> Rust.
                webview.post_message(&format!("echo:{message}"));
            },
            on_external_navigation: move |url: String| { /* open in the system browser */ },
            on_open_window: move |url: String| { /* open in the system browser */ },
            on_render_process_gone: move |_| { /* the page was reloaded */ },
            style: css!(flex_grow: 1.0),
        )
    }
}
```

`MasamuneWebViewRef` methods: `reload`, `go_back`, `go_forward`, `stop_loading`,
`post_message(data)`, `evaluate_javascript(script)` and `load_url(url)`. They do nothing while the
view is not mounted.

### Props

| Prop | Type | Default | Description |
|---|---|---|---|
| `url` | `Signal<String>` | — | Address to load. Changing it navigates; page navigations are not written back. |
| `html` | `Signal<String>` | — | Inline HTML shown while `url` is empty. |
| `user_agent` | `Signal<String>` | adapter, else platform | User-Agent string. |
| `javascript_enabled` | `bool` | `true` | Allow JavaScript. |
| `user_scripts` | `Vec<String>` | `[]` | Scripts run at document start in the main frame, after `window.whisker` is defined. Changes apply from the next navigation. |
| `allowed_origins` | `Vec<String>` | adapter, else `[]` | Origins such as `https://host` the main frame may load. Empty allows every origin. |
| `grant_media_permissions` | `bool` | adapter, else `false` | Grant camera / microphone requests of allowed origins. |
| `on_message` | `Fn(String)` | — | Page -> Rust message. |
| `on_load_start` / `on_load` | `Fn(String)` | — | A main-frame load started / finished. |
| `on_progress` | `Fn(f32)` | — | Load progress `0.0..=1.0`. |
| `on_error` | `Fn(WebViewError)` | — | A main-frame load failed (`url`, `code`, `description`). Cancelled loads are not reported. |
| `on_external_navigation` | `Fn(String)` | — | The main frame tried to load an `http(s)` URL outside `allowed_origins`. The navigation was cancelled. |
| `on_open_window` | `Fn(String)` | — | The page asked for a new window. No window is created. |
| `on_render_process_gone` | `Fn(())` | — | The web content process ended. The view already reloaded the last page (Android creates a new WebView). |
| `style` | `Style` | — | Whisker style. |
| `webview_ref` | `MasamuneWebViewRef` | — | Imperative handle. |

## JavaScript bridge

`window.whisker` is defined in the main frame before any page script runs:

```js
window.whisker.isMasamuneWebView === true;

// Page -> Rust: delivered to `on_message` as a string (other values are JSON-encoded).
window.whisker.postMessage("hello");

// Rust -> page: `MasamuneWebViewRef::post_message(data)` dispatches
window.addEventListener("whisker:message", (event) => {
  console.log(event.detail); // the string passed to post_message
});
```

`post_message(data)` runs
`window.dispatchEvent(new CustomEvent('whisker:message', { detail: data }))` in the main frame.
`data` is always a string, escaped as a JavaScript string literal.

Messages are accepted only from the main frame with an explicitly allowed web origin.
Empty allowlists and opaque inline HTML origins cannot send native messages. Android uses
`WebViewCompat.addWebMessageListener`; WebView versions without it have no native bridge.

## Navigation policy

Origins are compared as `scheme://host[:port]` (lowercase, default ports omitted;
`normalize_origin` / `is_allowed_url` implement the same rule in Rust).

| Navigation | Result |
|---|---|
| `http(s)` to an allowed origin (or any origin when the list is empty) | Loaded |
| `http(s)` elsewhere, main frame | Cancelled, `on_external_navigation(url)` |
| `http(s)` elsewhere, sub frame | Cancelled silently |
| `about:` (inline HTML, `srcdoc`) | Loaded |
| Any other scheme (`file:`, `javascript:`, `data:`, `blob:`, `intent:`, custom schemes) | Cancelled silently |

The policy applies to navigations started by the page. `url`, `load_url` and reloads started by the
app accept `http(s)` URLs and `about:blank` only.

## Fixed settings

Zoom disabled, DOM storage and databases enabled, default cache, mixed content blocked, file and
content access disabled, media playback without a user gesture and inline playback. iOS also
enables back / forward swipe gestures and link previews. The Android background is opaque.

## App defaults (`WebViewMasamuneAdapter`)

```rust
use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_webview::WebViewMasamuneAdapter;

MasamuneAdapters::new()
    .adapter(
        WebViewMasamuneAdapter::new()
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .allowed_origins(["https://example.com"])
            .grant_media_permissions(true)
            .max_download_bytes(28 * 1024 * 1024),
    )
    .install();
```

Props that are not set use these values. `webview_defaults()` and `max_download_bytes()` read the
installed values (built-in defaults when no adapter is installed).

## Permissions (CNG)

```rust
use masamune_whisker_webview::cng::MasamuneWebViewPlugin;

app.plugin::<MasamuneWebViewPlugin>(|c| {
    c.camera_usage("Used for video calls in the web app.")
        .microphone_usage("Used for voice input in the web app.")
        .photo_library_usage("Used to upload photos to the web app.");
});
```

- Android: adds `CAMERA`, `RECORD_AUDIO` and `MODIFY_AUDIO_SETTINGS`. The runtime permission is
  requested when an allowed page asks for the camera or microphone.
- iOS: writes `NSCameraUsageDescription`, `NSMicrophoneUsageDescription` and
  `NSPhotoLibraryUsageDescription`. Unset descriptions get short English defaults, which never
  replace a value written by another plugin.

## Native privileges

An empty `allowed_origins` list permits web navigation but denies all incoming bridge
messages and media requests. Explicitly list trusted web origins to enable these features.
Inline HTML with an opaque origin cannot send native messages. Only the main frame can
send bridge messages. Android devices without `WEB_MESSAGE_LISTENER` have no native
bridge; the insecure legacy fallback is not supported. Media auto-grant additionally
requires `grant_media_permissions`; configuring it with an empty list is an error.
