#![doc = include_str!("../README.md")]

pub mod cng;

mod adapter;
mod link;

pub use adapter::{DeeplinkConfig, DeeplinkMasamuneAdapter, latest_link, take_pending};
pub use link::{Link, parse_link};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Deeplink")
}

/// The link that launched the app, if any. It is consumed: later calls return `None`.
///
/// - Android: the `ACTION_VIEW` intent that started `MainActivity` (ignored when the app is
///   reopened from the recent apps). The call waits until the activity is attached, hence
///   `async`.
/// - iOS: the URL or Universal Link in the scene's connection options.
///
/// Errors (and platforms without the native module) are logged and return `None`.
pub async fn get_initial_link() -> Option<String> {
    match module()
        .call_async::<_, Option<String>>("getInitialLink", &json!({}))
        .await
    {
        Ok(link) => link.filter(|link| !link.is_empty()),
        Err(error) => {
            log::debug!("deeplink.initial_failed: {error}");
            None
        }
    }
}

/// Receives links that open the running app (custom schemes, Universal Links, App Links) while
/// the returned [`Subscription`] is held.
///
/// - Android: `ACTION_VIEW` intents delivered to `MainActivity.onNewIntent` (`launchMode` is
///   `singleTask`, set by [`cng::MasamuneDeeplink`]).
/// - iOS: `scene(_:openURLContexts:)` and `scene(_:continue:)` of the app's scene delegate.
///   Links that arrive before the first subscription are delivered when it starts.
///
/// The link that launched the app is returned by [`get_initial_link`] instead.
pub fn on_link(handler: impl Fn(String) + 'static) -> Subscription {
    module().subscribe("link", move |link: String| {
        if !link.is_empty() {
            handler(link);
        }
    })
}
