#![doc = include_str!("../README.md")]

pub mod cng;

mod adapter;

pub use adapter::AudioSessionMasamuneAdapter;
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Notification id of the Android media notification (and its foreground service). Other
/// foreground services can show the same notification, for example the background location
/// service of `masamune-whisker-location` (`AndroidLocationNotification::reuse_notification_id`).
pub const ANDROID_NOTIFICATION_ID: i32 = 0x4d41_5553;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "AudioSession")
}

/// How the app plays audio and how its media notification looks on Android.
///
/// [`AudioSessionConfig::music`] matches `AudioSessionConfiguration.music()` of Flutter's
/// `audio_session`: playback that keeps running with the screen locked, interrupts other apps
/// and is interrupted by calls and other media apps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSessionConfig {
    /// Play together with other apps' audio instead of interrupting it (iOS
    /// `.mixWithOthers`; Android requests no audio focus).
    pub mix_with_others: bool,
    /// Lower other apps' audio while this app plays (iOS `.duckOthers`; Android requests
    /// `AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK`). Ignored with `mix_with_others`.
    pub duck_others: bool,
    /// Android notification channel of the media notification.
    pub channel_id: String,
    /// Android notification channel name shown in the system settings.
    pub channel_name: String,
    /// Android notification channel description shown in the system settings.
    pub channel_description: String,
    /// Android small icon of the media notification as a drawable or mipmap resource name
    /// (`"mipmap/ic_launcher_notification"` or `"ic_launcher_notification"`). Empty uses the app
    /// icon.
    pub small_icon: String,
}

impl AudioSessionConfig {
    /// Music playback that interrupts other apps.
    pub fn music() -> Self {
        Self {
            mix_with_others: false,
            duck_others: false,
            channel_id: "masamune_whisker_audio_session".into(),
            channel_name: "Playback".into(),
            channel_description: String::new(),
            small_icon: String::new(),
        }
    }
}

impl Default for AudioSessionConfig {
    fn default() -> Self {
        Self::music()
    }
}

/// Configures the audio session (iOS `AVAudioSession` category `.playback`; Android audio
/// attributes for music and the notification channel). Call once at startup, before playing.
pub fn configure(config: &AudioSessionConfig) -> NativeResult<bool> {
    module().call("configure", config)
}

/// Activates the session before playback, or deactivates it when playback stops for good.
///
/// - iOS: `AVAudioSession.setActive`. Deactivating notifies other apps that they may resume.
/// - Android: requests or abandons audio focus. Returns `false` when focus was not granted (for
///   example during a phone call); do not start playback then.
pub fn set_active(active: bool) -> NativeResult<bool> {
    module().call("setActive", &json!({ "active": active }))
}

/// What kind of interruption happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InterruptionKind {
    /// Playback has to pause (a call, an alarm, another media app).
    Pause,
    /// Playback may continue at a lower volume (Android
    /// `AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK`). The system ducks automatically on Android 8 and
    /// later, so most apps ignore it.
    Duck,
    /// Another app took over the audio for good (Android `AUDIOFOCUS_LOSS`). No end event
    /// follows; resume only when the user asks.
    Unknown,
}

/// The start or end of an interruption (Flutter's `AudioInterruptionEvent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interruption {
    /// `true` when the interruption starts, `false` when it ends.
    pub begin: bool,
    pub kind: InterruptionKind,
    /// On an end event: the system suggests resuming playback (iOS `.shouldResume`; Android
    /// regained focus after a transient loss). Always `false` on begin events.
    #[serde(default)]
    pub should_resume: bool,
}

/// Receives interruptions. Events are delivered only while the returned [`Subscription`] is
/// held.
///
/// The app pauses its players itself on `begin` (iOS pauses `AVPlayer` by itself as well) and
/// decides whether to resume on the end event.
pub fn on_interruption(handler: impl Fn(Interruption) + 'static) -> Subscription {
    module().subscribe("interruption", handler)
}

/// Why the audio route changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouteChangeReason {
    /// An output was connected (headphones, Bluetooth, CarPlay / Android Auto).
    NewDeviceAvailable,
    /// The output in use went away (headphones unplugged, Bluetooth disconnected). Media apps
    /// pause here; see [`on_becoming_noisy`].
    OldDeviceUnavailable,
    /// Any other change (category change, override, wake from sleep).
    Other,
}

/// A change of the audio output.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteChange {
    pub reason: RouteChangeReason,
    /// Current outputs after the change (iOS `AVAudioSessionPortDescription.portType` such as
    /// `Speaker` or `BluetoothA2DPOutput`; Android `AudioDeviceInfo` types such as
    /// `builtinSpeaker` or `bluetoothA2dp`).
    #[serde(default)]
    pub outputs: Vec<String>,
}

/// Receives audio route changes while the returned [`Subscription`] is held.
///
/// - iOS: `AVAudioSession.routeChangeNotification`.
/// - Android: `AudioDeviceCallback` for added / removed outputs and
///   `ACTION_AUDIO_BECOMING_NOISY` as [`RouteChangeReason::OldDeviceUnavailable`].
pub fn on_route_change(handler: impl Fn(RouteChange) + 'static) -> Subscription {
    module().subscribe("routeChange", handler)
}

/// Calls `handler` when the output in use goes away (Flutter's `becomingNoisyEventStream`):
/// [`on_route_change`] filtered to [`RouteChangeReason::OldDeviceUnavailable`].
pub fn on_becoming_noisy(handler: impl Fn() + 'static) -> Subscription {
    on_route_change(move |change| {
        if is_becoming_noisy(&change) {
            handler();
        }
    })
}

/// Whether `change` means the output in use went away.
pub fn is_becoming_noisy(change: &RouteChange) -> bool {
    change.reason == RouteChangeReason::OldDeviceUnavailable
}

/// The item shown on the lock screen, in Control Center and in the Android media notification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// Stable ID of the item (for example the track ID).
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Absolute path of a PNG or JPEG artwork file (decoded to at most 512 px).
    pub artwork_path: Option<String>,
    /// Duration in milliseconds; `None` while unknown (no progress bar).
    pub duration_ms: Option<u64>,
}

/// Sets the Now Playing item.
///
/// On Android the first item also starts the media playback foreground service with the media
/// notification, which keeps the process alive in the background. Start it while the app is in
/// the foreground: Android 12 and later refuse to start foreground services from the background.
/// The service stays until [`clear_now_playing`], also while paused (like
/// `androidStopForegroundOnPause: false` of `audio_service`).
pub fn set_now_playing(item: &NowPlaying) -> NativeResult<bool> {
    module().call("setNowPlaying", item)
}

/// Removes the Now Playing item, the media notification and (Android) the foreground service.
pub fn clear_now_playing() -> NativeResult<bool> {
    module().call("clearNowPlaying", &json!({}))
}

/// Coarse player state for the system media controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessingState {
    #[default]
    Idle,
    Loading,
    Buffering,
    Ready,
    Completed,
    Error,
}

/// Playback progress shown with the Now Playing item.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackState {
    pub playing: bool,
    pub state: ProcessingState,
    /// Position in milliseconds at the time of the call. The system extrapolates it with `speed`.
    pub position_ms: u64,
    /// Playback rate (1.0 is normal speed).
    pub speed: f64,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            playing: false,
            state: ProcessingState::Idle,
            position_ms: 0,
            speed: 1.0,
        }
    }
}

/// Updates the playing flag and position. Call it when playback starts, pauses, seeks or
/// changes the item; the system advances the position by itself in between.
pub fn set_playback_state(state: &PlaybackState) -> NativeResult<bool> {
    let speed = if state.speed.is_finite() && state.speed >= 0.0 {
        state.speed
    } else {
        1.0
    };
    module().call("setPlaybackState", &PlaybackState { speed, ..*state })
}

/// System command an app-specific action maps to on iOS (`MPFeedbackCommand`). Android shows
/// every custom action as a notification button instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IosFeedbackCommand {
    Like,
    Dislike,
    Bookmark,
}

/// An app-specific action (for example "keep this track").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAction {
    /// Delivered back as [`RemoteCommand::Custom`].
    pub id: String,
    /// Button label (Android) and feedback title (iOS).
    pub label: String,
    /// Android drawable resource name of the button icon (`"drawable/save_track_filled"` or
    /// `"save_track_filled"`). Empty uses a generic icon.
    pub android_icon: String,
    /// iOS lock screen command to show the action as. `None` hides it on iOS.
    pub ios_command: Option<IosFeedbackCommand>,
    /// Whether the feedback is currently active (iOS `isActive`, for example a filled heart).
    pub active: bool,
}

/// Which buttons the system media controls show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaControls {
    pub previous: bool,
    pub next: bool,
    /// Allow seeking with the progress bar.
    pub seek: bool,
    /// Extra actions after previous / play-pause / next.
    pub custom: Vec<CustomAction>,
}

impl Default for MediaControls {
    fn default() -> Self {
        Self {
            previous: true,
            next: true,
            seek: true,
            custom: Vec::new(),
        }
    }
}

/// Sets the buttons of the lock screen, Control Center and the Android media notification.
/// Play and pause are always available.
pub fn set_controls(controls: &MediaControls) -> NativeResult<bool> {
    module().call("setControls", controls)
}

/// A command from the lock screen, Control Center, headphones, a car or the media notification.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "command", rename_all = "camelCase")]
pub enum RemoteCommand {
    Play,
    Pause,
    TogglePlayPause,
    Next,
    Previous,
    Stop,
    /// Seek to a position in milliseconds.
    Seek {
        #[serde(rename = "positionMs")]
        position_ms: u64,
    },
    /// A [`CustomAction`] by its ID.
    Custom {
        id: String,
    },
}

/// Receives remote commands while the returned [`Subscription`] is held. The module does not
/// control any player: the handler drives the app's players and then reports the result with
/// [`set_playback_state`].
///
/// iOS suspends the Rust runtime while the app is in the background without playing audio, so
/// commands arrive once audio keeps the app running (background audio mode) or the app returns.
pub fn on_command(handler: impl Fn(RemoteCommand) + 'static) -> Subscription {
    module().subscribe("command", handler)
}

/// Calls `handler` when the app is opened from the media notification (Android only; iOS has no
/// media notification). The launch from a tap is reported once the subscription exists.
pub fn on_notification_tap(handler: impl Fn() + 'static) -> Subscription {
    module().subscribe("notificationTap", move |_: serde_json::Value| handler())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_id_matches_the_android_host() {
        let kotlin = include_str!(
            "../android/src/main/kotlin/net/mathru/whisker/audio_session/AudioSessions.kt"
        );
        assert_eq!(ANDROID_NOTIFICATION_ID, 0x4d415553);
        assert!(kotlin.contains("const val NOTIFICATION_ID = 0x4d415553"));
    }

    #[test]
    fn decodes_remote_commands() {
        let decode = |json: &str| serde_json::from_str::<RemoteCommand>(json).unwrap();
        assert_eq!(decode(r#"{"command":"play"}"#), RemoteCommand::Play);
        assert_eq!(
            decode(r#"{"command":"togglePlayPause"}"#),
            RemoteCommand::TogglePlayPause
        );
        assert_eq!(
            decode(r#"{"command":"seek","positionMs":1500}"#),
            RemoteCommand::Seek { position_ms: 1500 }
        );
        assert_eq!(
            decode(r#"{"command":"custom","id":"save"}"#),
            RemoteCommand::Custom { id: "save".into() }
        );
        assert!(serde_json::from_str::<RemoteCommand>(r#"{"command":"rewind"}"#).is_err());
    }

    #[test]
    fn decodes_interruptions_and_routes() {
        let begin: Interruption = serde_json::from_str(r#"{"begin":true,"kind":"pause"}"#).unwrap();
        assert_eq!(
            begin,
            Interruption {
                begin: true,
                kind: InterruptionKind::Pause,
                should_resume: false
            }
        );
        let end: Interruption =
            serde_json::from_str(r#"{"begin":false,"kind":"duck","shouldResume":true}"#).unwrap();
        assert!(end.should_resume);
        let change: RouteChange =
            serde_json::from_str(r#"{"reason":"oldDeviceUnavailable","outputs":["Speaker"]}"#)
                .unwrap();
        assert!(is_becoming_noisy(&change));
        let change: RouteChange = serde_json::from_str(r#"{"reason":"other"}"#).unwrap();
        assert!(!is_becoming_noisy(&change));
        assert!(change.outputs.is_empty());
    }

    #[test]
    fn encodes_the_wire_format() {
        let item = NowPlaying {
            id: "t1".into(),
            title: "Song".into(),
            duration_ms: Some(1000),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&item).unwrap(),
            json!({
                "id": "t1", "title": "Song", "artist": "", "album": "",
                "artworkPath": null, "durationMs": 1000,
            })
        );
        let controls = MediaControls {
            custom: vec![CustomAction {
                id: "save".into(),
                label: "Keep".into(),
                android_icon: "drawable/save".into(),
                ios_command: Some(IosFeedbackCommand::Like),
                active: true,
            }],
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&controls).unwrap()["custom"][0],
            json!({
                "id": "save", "label": "Keep", "androidIcon": "drawable/save",
                "iosCommand": "like", "active": true,
            })
        );
        assert_eq!(
            serde_json::to_value(PlaybackState::default()).unwrap(),
            json!({ "playing": false, "state": "idle", "positionMs": 0, "speed": 1.0 })
        );
        assert_eq!(
            serde_json::to_value(AudioSessionConfig::music()).unwrap()["mixWithOthers"],
            json!(false)
        );
    }
}
