use serde::{Deserialize, Serialize};

use crate::{NativeError, NativeResult};

/// One entry of the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueItem {
    /// Unique within the queue. Events and [`PlaylistStatus`] refer to the item by this ID.
    pub id: String,
    /// `https://` URL, `file://` URL or absolute file path (for example a file in the app's
    /// cache directory). See [`validate_uri`].
    pub uri: String,
}

impl QueueItem {
    pub fn new(id: impl Into<String>, uri: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            uri: uri.into(),
        }
    }
}

/// Accepts the sources both platforms play: `https://` URLs with a host, `file:///` URLs and
/// absolute file paths. Cleartext `http://` (blocked by App Transport Security and Android's
/// cleartext policy), relative paths and other schemes are rejected.
pub fn validate_uri(uri: &str) -> NativeResult<()> {
    let lower = uri.get(..8).unwrap_or(uri).to_ascii_lowercase();
    let valid = if lower.starts_with("https://") {
        uri[8..]
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|host| !host.is_empty())
    } else if lower.starts_with("file://") {
        uri[7..].starts_with('/') && uri.len() > 8
    } else {
        uri.starts_with('/') && uri.len() > 1
    };
    if valid && !uri.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(NativeError(
            "audio source must be an https URL, a file URL or an absolute path".into(),
        ))
    }
}

/// Why an item left the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndReason {
    /// Played to the end.
    Completed,
    /// [`crate::AudioPlaylist::skip_to_next`].
    Skipped,
    /// [`crate::AudioPlaylist::remove`] or replaced by [`crate::AudioPlaylist::set_queue`].
    Removed,
    /// Could not be loaded or played. A [`PlaylistEvent::Error`] precedes it and the playlist
    /// moves on to the next item.
    Failed,
    /// [`crate::AudioPlaylist::stop`].
    Stopped,
}

/// Something that happened to the playlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlaylistEvent {
    /// The item became the current item. It is audible when the playlist is playing (and no
    /// [`InterruptMode::Pause`] announcement runs). `remaining` counts the items queued after
    /// it: append more while it is above zero to keep the playback gapless.
    ItemStarted { id: String, remaining: usize },
    /// The current item left the queue.
    ItemEnded { id: String, reason: EndReason },
    /// The last item ended and nothing follows. Append items to continue; a playing playlist
    /// starts them right away (with a gap, since the queue was empty).
    QueueDrained,
    /// A source failed. `id` is the queue item or the announcement ID.
    Error { id: Option<String>, message: String },
    /// An announcement started ([`crate::AudioPlaylist::interrupt`]).
    InterruptStarted { id: String },
    /// An announcement ended and the queue was restored. `completed` is `false` when it failed,
    /// was cancelled or was replaced by another announcement.
    InterruptEnded { id: String, completed: bool },
}

/// Snapshot of the playlist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistStatus {
    /// ID of the current item.
    pub current_id: Option<String>,
    /// Position in the current item in milliseconds.
    pub position_ms: u64,
    /// Duration of the current item in milliseconds, `None` while unknown.
    pub duration_ms: Option<u64>,
    /// Whether the playlist is in the playing state (also while an announcement pauses it).
    pub playing: bool,
    /// Whether an announcement is playing.
    pub interrupting: bool,
    /// IDs of the queue, current item first.
    pub queue: Vec<String>,
    /// Queue volume set with [`crate::AudioPlaylist::set_volume`] (before ducking).
    pub volume: f64,
}

impl Default for PlaylistStatus {
    fn default() -> Self {
        Self {
            current_id: None,
            position_ms: 0,
            duration_ms: None,
            playing: false,
            interrupting: false,
            queue: Vec::new(),
            volume: 1.0,
        }
    }
}

/// What happens to the queue while an announcement plays.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum InterruptMode {
    /// Pause the queue and resume it at the same position afterwards.
    Pause,
    /// Keep the queue playing at `volume` times its volume (`0.0..=1.0`).
    Duck {
        #[serde(rename = "duckVolume")]
        volume: f64,
    },
}

/// Options of [`crate::AudioPlaylist::interrupt`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterruptOptions {
    pub mode: InterruptMode,
    /// Volume of the announcement (`0.0..=1.0`).
    pub volume: f64,
}

impl Default for InterruptOptions {
    /// Pause the queue and play the announcement at full volume.
    fn default() -> Self {
        Self {
            mode: InterruptMode::Pause,
            volume: 1.0,
        }
    }
}

impl InterruptOptions {
    /// Keep the queue playing at `volume` times its volume.
    pub fn duck(volume: f64) -> Self {
        Self {
            mode: InterruptMode::Duck { volume },
            ..Self::default()
        }
    }
}

/// A validated announcement passed to [`crate::AudioPlaylistAdapter::interrupt`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterruptRequest {
    pub id: String,
    pub uri: String,
    #[serde(flatten)]
    pub mode: InterruptMode,
    pub volume: f64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn validates_sources() {
        for ok in [
            "https://example.com/a.m4a",
            "HTTPS://example.com",
            "file:///var/mobile/Caches/a.m4a",
            "/data/user/0/app/cache/a.mp3",
        ] {
            assert!(validate_uri(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "/",
            "http://example.com/a.m4a",
            "https://",
            "https:///a.m4a",
            "file://relative/a.m4a",
            "file:///",
            "cache/a.m4a",
            "content://media/1",
            "/cache/a\n.m4a",
        ] {
            assert!(validate_uri(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn events_use_the_wire_format() {
        let decode = |value: serde_json::Value| serde_json::from_value::<PlaylistEvent>(value);
        assert_eq!(
            decode(json!({ "type": "itemStarted", "id": "a", "remaining": 2 })).unwrap(),
            PlaylistEvent::ItemStarted {
                id: "a".into(),
                remaining: 2
            }
        );
        assert_eq!(
            decode(json!({ "type": "itemEnded", "id": "a", "reason": "completed" })).unwrap(),
            PlaylistEvent::ItemEnded {
                id: "a".into(),
                reason: EndReason::Completed
            }
        );
        assert_eq!(
            decode(json!({ "type": "queueDrained" })).unwrap(),
            PlaylistEvent::QueueDrained
        );
        assert_eq!(
            decode(json!({ "type": "error", "id": null, "message": "x" })).unwrap(),
            PlaylistEvent::Error {
                id: None,
                message: "x".into()
            }
        );
        assert_eq!(
            decode(json!({ "type": "interruptEnded", "id": "i", "completed": true })).unwrap(),
            PlaylistEvent::InterruptEnded {
                id: "i".into(),
                completed: true
            }
        );
        assert!(decode(json!({ "type": "unknown" })).is_err());
    }

    #[test]
    fn interrupt_request_uses_the_wire_format() {
        let request = InterruptRequest {
            id: "i".into(),
            uri: "/a.m4a".into(),
            mode: InterruptMode::Duck { volume: 0.2 },
            volume: 1.0,
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({ "id": "i", "uri": "/a.m4a", "mode": "duck", "duckVolume": 0.2, "volume": 1.0 })
        );
        let request = InterruptRequest {
            mode: InterruptMode::Pause,
            ..request
        };
        assert_eq!(serde_json::to_value(&request).unwrap()["mode"], "pause");
        let status: PlaylistStatus = serde_json::from_value(json!({
            "currentId": "a", "positionMs": 10, "durationMs": null, "playing": true,
            "interrupting": false, "queue": ["a", "b"], "volume": 0.5,
        }))
        .unwrap();
        assert_eq!(status.queue, vec!["a", "b"]);
    }
}
