#![doc = include_str!("../README.md")]

pub mod cng;

mod adapter;

pub use adapter::{
    DEFAULT_IMAGE_EXTENSIONS, DEFAULT_MAX_TEXT_LEN, ShareReceiveConfig,
    ShareReceiveMasamuneAdapter, filter_payload, latest_share, take_pending,
};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::{Deserialize, Serialize};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "ShareReceive")
}

/// Content another app shared with this app.
///
/// `image_paths` are absolute paths of copies in this app's cache directory (Android:
/// `cacheDir/masamune_whisker_share_receive`, iOS: `Library/Caches/masamune_whisker_share_receive`),
/// so they stay readable after the sharing app revokes its access. The OS may clear the cache
/// later: move the files elsewhere to keep them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SharedPayload {
    /// Shared text. A shared URL arrives as its text (on iOS, several items are joined with
    /// newlines).
    pub text: Option<String>,
    /// Absolute paths of the shared images.
    pub image_paths: Vec<String>,
}

impl SharedPayload {
    /// Whether the payload has neither text nor images.
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.image_paths.is_empty()
    }
}

/// The share that launched the app, if any. It is consumed: later calls return `None`.
///
/// - Android: the `ACTION_SEND` / `ACTION_SEND_MULTIPLE` intent that started `MainActivity`
///   (ignored when the app is reopened from the recent apps). The call waits until the activity
///   is attached and copies `content://` images into the cache, hence `async`.
/// - iOS: the content the Share Extension saved to the App Group while the app was not running.
///
/// Errors (and platforms without the native module) are logged and return `None`.
pub async fn get_initial_share() -> Option<SharedPayload> {
    match module()
        .call_async::<_, Option<SharedPayload>>("getInitialShare", &())
        .await
    {
        Ok(payload) => payload.filter(|payload| !payload.is_empty()),
        Err(error) => {
            log::debug!("share_receive.initial_failed: {error}");
            None
        }
    }
}

/// Receives shares that arrive while the app is running. Shares are delivered only while the
/// returned [`Subscription`] is held.
///
/// - Android: intents delivered to `MainActivity.onNewIntent` (`launchMode="singleTask"`, which
///   the CNG plugin sets).
/// - iOS: content the Share Extension saved, read whenever the app becomes active. The app does
///   not open by itself after sharing; the user returns to it.
///
/// The share that launched the app is returned by [`get_initial_share`] instead. Handlers receive
/// the payload unfiltered; [`ShareReceiveMasamuneAdapter`] applies [`ShareReceiveConfig`].
pub fn on_share(handler: impl Fn(SharedPayload) + 'static) -> Subscription {
    module().subscribe("share", move |payload: SharedPayload| {
        if !payload.is_empty() {
            handler(payload);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_wire_payload() {
        let payload: SharedPayload =
            serde_json::from_str(r#"{"text":"hello","imagePaths":["/tmp/a.png"]}"#).unwrap();
        assert_eq!(
            payload,
            SharedPayload {
                text: Some("hello".into()),
                image_paths: vec!["/tmp/a.png".into()],
            }
        );
        let empty: SharedPayload = serde_json::from_str(r#"{"text":null}"#).unwrap();
        assert!(empty.is_empty());
    }
}
