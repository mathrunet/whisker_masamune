#![doc = include_str!("../README.md")]

use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use masamune_whisker_bridge::{NativeModule, Subscription};
use serde::Deserialize;
use serde_json::json;

/// Error message of a microphone start without the `RECORD_AUDIO` permission.
pub const PERMISSION_DENIED: &str = "permission_denied";

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "SpeechRecognition")
}

/// What the device offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    /// A recognizer is installed (on-device or the default service).
    #[serde(default)]
    pub available: bool,
    /// The on-device recognizer is available (Android 12+).
    #[serde(default)]
    pub on_device: bool,
    /// Audio can be fed with [`SpeechRecognition::feed`] (Android 13+).
    #[serde(default)]
    pub external_audio: bool,
}

/// Reports what the device offers. Unsupported platforms report nothing available.
pub fn availability() -> Availability {
    module()
        .call::<_, Option<Availability>>("availability", &json!({}))
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// How a session listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechRecognitionOptions {
    /// BCP 47 language tag such as `ja-JP` (the device language when `None`).
    pub language: Option<String>,
    /// Use the on-device recognizer when the device has one (default `true`). Falls back to the
    /// default recognizer when the on-device model lacks the language.
    pub prefer_on_device: bool,
    /// Recognize the PCM16 mono audio passed to [`SpeechRecognition::feed`] instead of the
    /// microphone (Android 13+). Use it when the app already records the microphone.
    pub external_audio: bool,
    /// Samples per second of the fed audio (default 16000).
    pub sample_rate: u32,
}

impl Default for SpeechRecognitionOptions {
    fn default() -> Self {
        Self {
            language: None,
            prefer_on_device: true,
            external_audio: false,
            sample_rate: 16_000,
        }
    }
}

/// Something the recognizer reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechRecognitionEvent {
    /// The whole text so far (finished segments followed by the current partial result).
    Text(String),
    /// Recognition stopped (for example `network`, `audio` or `permission_denied`).
    Error(String),
}

#[derive(Debug, Deserialize)]
struct NativeEvent {
    id: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

/// A running recognition session.
///
/// Only one session runs at a time: starting another stops the previous one. Dropping the handle
/// stops the session.
pub struct SpeechRecognition {
    id: String,
    _subscription: Subscription,
}

impl SpeechRecognition {
    /// Starts recognizing. `handler` receives the events on the UI thread.
    pub async fn start(
        options: SpeechRecognitionOptions,
        handler: impl Fn(SpeechRecognitionEvent) + 'static,
    ) -> NativeResult<Self> {
        let id = format!("speech-{}", NEXT_SESSION.fetch_add(1, Ordering::Relaxed));
        let watched = id.clone();
        let subscription = module().subscribe("speech", move |event: NativeEvent| {
            if event.id != watched {
                return;
            }
            if let Some(error) = event.error {
                handler(SpeechRecognitionEvent::Error(error));
            } else if let Some(text) = event.text {
                handler(SpeechRecognitionEvent::Text(text));
            }
        });
        let session = Self {
            id: id.clone(),
            _subscription: subscription,
        };
        module()
            .call_async::<_, Option<bool>>(
                "start",
                &json!({
                    "id": id,
                    "language": options.language,
                    "preferOnDevice": options.prefer_on_device,
                    "externalAudio": options.external_audio,
                    "sampleRate": options.sample_rate,
                }),
            )
            .await?;
        Ok(session)
    }

    /// Identifier of this session in the host events.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Passes PCM16 little-endian mono audio to an external audio session.
    pub fn feed(&self, pcm: &[u8]) {
        if pcm.is_empty() {
            return;
        }
        let data = base64::engine::general_purpose::STANDARD.encode(pcm);
        self.feed_base64(&data);
    }

    /// Like [`SpeechRecognition::feed`] with audio that is already base64.
    pub fn feed_base64(&self, data: &str) {
        let _ = module().call::<_, Option<bool>>("feed", &json!({ "id": self.id, "data": data }));
    }
}

impl Drop for SpeechRecognition {
    fn drop(&mut self) {
        let _ = module().call::<_, Option<bool>>("stop", &json!({ "id": self.id }));
    }
}

/// Whether `error` means the microphone permission is missing.
pub fn is_permission_denied(error: &NativeError) -> bool {
    error.0.starts_with(PERMISSION_DENIED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_availability() {
        let value: Availability =
            serde_json::from_str(r#"{"available":true,"onDevice":true,"externalAudio":false}"#)
                .unwrap();
        assert_eq!(
            value,
            Availability {
                available: true,
                on_device: true,
                external_audio: false
            }
        );
        let empty: Availability = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Availability::default());
    }

    #[test]
    fn decodes_events() {
        let text: NativeEvent = serde_json::from_str(r#"{"id":"speech-1","text":"こんにちは"}"#).unwrap();
        assert_eq!(text.text.as_deref(), Some("こんにちは"));
        let error: NativeEvent = serde_json::from_str(r#"{"id":"speech-1","error":"network"}"#).unwrap();
        assert_eq!(error.error.as_deref(), Some("network"));
    }

    #[test]
    fn permission_errors() {
        assert!(is_permission_denied(&NativeError(PERMISSION_DENIED.into())));
        assert!(!is_permission_denied(&NativeError("start_failed".into())));
    }
}
