#![doc = include_str!("../README.md")]

use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use masamune_whisker_bridge::{NativeModule, Subscription};
use serde::Deserialize;
use serde_json::json;

/// Error message of a start that failed because the user denied the microphone.
pub const PERMISSION_DENIED: &str = "permission_denied";

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "AudioCapture")
}

/// Format of the captured audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioCaptureOptions {
    /// Samples per second of the PCM16 output (default 16000).
    pub sample_rate: u32,
    /// Length of one chunk in milliseconds (default 100).
    pub chunk_ms: u32,
}

impl Default for AudioCaptureOptions {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            chunk_ms: 100,
        }
    }
}

impl AudioCaptureOptions {
    /// Bytes of PCM16 mono in one chunk.
    pub fn chunk_bytes(&self) -> usize {
        (self.sample_rate as usize * 2 * self.chunk_ms as usize / 1000).max(2)
    }
}

/// Something the host reported while capturing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioCaptureEvent {
    /// PCM16 little-endian mono samples (about [`AudioCaptureOptions::chunk_ms`] long).
    Chunk(Vec<u8>),
    /// Capturing stopped on its own (for example an interruption or a lost input device).
    Error(String),
}

#[derive(Debug, Deserialize)]
struct NativeEvent {
    id: String,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct StopResult {
    #[serde(default)]
    data: Option<String>,
}

static NEXT_CAPTURE: AtomicU64 = AtomicU64::new(1);

/// A running microphone capture.
///
/// Only one capture runs at a time: starting another stops the previous one. Dropping the handle
/// without [`AudioCapture::stop`] stops the capture and discards the remaining audio.
pub struct AudioCapture {
    id: String,
    subscription: Option<Subscription>,
    stopped: bool,
}

impl AudioCapture {
    /// Asks for the microphone permission when needed, then starts capturing.
    ///
    /// `handler` receives the chunks on the UI thread from the moment the call is made. Fails with
    /// [`PERMISSION_DENIED`] when the user denies the microphone (see [`is_permission_denied`]).
    pub async fn start(
        options: AudioCaptureOptions,
        handler: impl Fn(AudioCaptureEvent) + 'static,
    ) -> NativeResult<Self> {
        let id = format!("capture-{}", NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed));
        // Subscribed before starting so the first chunks are not lost.
        let watched = id.clone();
        let subscription = module().subscribe("audio", move |event: NativeEvent| {
            if event.id != watched {
                return;
            }
            if let Some(error) = event.error {
                handler(AudioCaptureEvent::Error(error));
            } else if let Some(data) = event.data
                && let Some(bytes) = decode(&data)
                && !bytes.is_empty()
            {
                handler(AudioCaptureEvent::Chunk(bytes));
            }
        });
        let mut capture = Self {
            id: id.clone(),
            subscription: Some(subscription),
            stopped: false,
        };
        let started: NativeResult<bool> = module()
            .call_async(
                "start",
                &json!({
                    "id": id,
                    "sampleRate": options.sample_rate,
                    "chunkMs": options.chunk_ms,
                }),
            )
            .await;
        match started {
            Ok(_) => Ok(capture),
            Err(error) => {
                // Nothing runs on the host after a failed start.
                capture.stopped = true;
                Err(error)
            }
        }
    }

    /// Identifier of this capture in the host events.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Stops capturing and returns the audio recorded after the last chunk (possibly empty).
    pub async fn stop(mut self) -> NativeResult<Vec<u8>> {
        self.stopped = true;
        let result: NativeResult<Option<StopResult>> =
            module().call_async("stop", &json!({ "id": self.id })).await;
        self.subscription.take();
        let rest = result?.unwrap_or_default();
        Ok(rest.data.as_deref().and_then(decode).unwrap_or_default())
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = module().call::<_, Option<bool>>("cancel", &json!({ "id": self.id }));
        }
    }
}

/// Whether `error` means the user denied the microphone.
pub fn is_permission_denied(error: &NativeError) -> bool {
    error.0.starts_with(PERMISSION_DENIED)
}

fn decode(data: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_bytes() {
        assert_eq!(AudioCaptureOptions::default().chunk_bytes(), 3200);
        let options = AudioCaptureOptions {
            sample_rate: 48_000,
            chunk_ms: 20,
        };
        assert_eq!(options.chunk_bytes(), 1920);
    }

    #[test]
    fn permission_errors() {
        assert!(is_permission_denied(&NativeError(PERMISSION_DENIED.into())));
        assert!(is_permission_denied(&NativeError(
            "permission_denied: Microphone permission was denied.".into()
        )));
        assert!(!is_permission_denied(&NativeError("start_failed".into())));
    }

    #[test]
    fn decodes_events() {
        let event: NativeEvent =
            serde_json::from_str(r#"{"id":"capture-1","data":"AQID"}"#).unwrap();
        assert_eq!(event.id, "capture-1");
        assert_eq!(decode(event.data.as_deref().unwrap()), Some(vec![1, 2, 3]));
        let stop: Option<StopResult> = serde_json::from_str("null").unwrap();
        assert!(stop.is_none());
    }
}
