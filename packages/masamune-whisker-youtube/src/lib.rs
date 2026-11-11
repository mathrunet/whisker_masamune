//! YouTube player element for Whisker apps: the YouTube IFrame Player API hosted in a native web
//! view (`WKWebView` on iOS, `android.webkit.WebView` on Android).
//!
//! - [`YouTubePlayer`]: the native player with props, events and an imperative
//!   [`YouTubePlayerRef`].
//! - [`YouTube`]: a 16:9 box that shows a black placeholder while the video ID is empty or the
//!   page transition is still running, then mounts the player (port of `VideoWidget` in
//!   `flutter_royaltybank_package/lib/widgets/video.dart`).
//!
//! The native element tag is `masamune-whisker-youtube:YouTubePlayer`. Native sources:
//! `android/src/main/kotlin/net/mathru/whisker/youtube/` and
//! `ios/Sources/MasamuneWhiskerYoutube/`. Hosts other than iOS and Android render the black
//! placeholder only.

mod html;

pub use html::{DEFAULT_ORIGIN, player_html, sanitize_video_id};

use std::rc::Rc;
use std::time::Duration;

use whisker::platform_module::WhiskerValue;
use whisker::prelude::*;
use whisker::{ElementRef, Signal, Style};

/// Whether this target has a native implementation of the player element.
const HAS_NATIVE_PLAYER: bool = cfg!(any(target_os = "ios", target_os = "android"));

/// Playback state reported by the YouTube IFrame Player (`YT.PlayerState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlayerState {
    /// `-1`: no video has started yet.
    Unstarted,
    /// `0`: playback ended.
    Ended,
    /// `1`: playing.
    Playing,
    /// `2`: paused.
    Paused,
    /// `3`: buffering.
    Buffering,
    /// `5`: a video is cued and ready to play.
    Cued,
}

impl PlayerState {
    /// The state for a `YT.PlayerState` code, or `None` for an unknown code.
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            -1 => Some(Self::Unstarted),
            0 => Some(Self::Ended),
            1 => Some(Self::Playing),
            2 => Some(Self::Paused),
            3 => Some(Self::Buffering),
            5 => Some(Self::Cued),
            _ => None,
        }
    }

    /// The `YT.PlayerState` code of this state.
    pub fn code(self) -> i32 {
        match self {
            Self::Unstarted => -1,
            Self::Ended => 0,
            Self::Playing => 1,
            Self::Paused => 2,
            Self::Buffering => 3,
            Self::Cued => 5,
        }
    }
}

impl TryFrom<i32> for PlayerState {
    type Error = i32;

    /// Fails with the unknown code.
    fn try_from(code: i32) -> Result<Self, Self::Error> {
        Self::from_code(code).ok_or(code)
    }
}

impl From<PlayerState> for i32 {
    fn from(state: PlayerState) -> Self {
        state.code()
    }
}

// Every native event carries its body under `detail`. Fields default so that a partial body
// still reaches the handler.

/// Body of the `state_change` event.
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct StateChangeEvent {
    #[serde(default)]
    pub detail: StateChangeDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct StateChangeDetail {
    #[serde(default)]
    pub state: i64,
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
    pub code: i64,
}

/// Imperative handle of a mounted [`YouTubePlayer`], bound through its `player_ref` prop.
///
/// Every method is a no-op while the player is not mounted, and playback commands are ignored
/// until the player reported ready (except `cue_video` / `load_video`, whose video is kept and
/// applied once ready). The handle is `Copy`.
#[derive(Copy, Clone)]
pub struct YouTubePlayerRef {
    r: ElementRef,
}

impl YouTubePlayerRef {
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

    /// Starts or resumes playback.
    pub fn play(&self) {
        let _ = self.r.command("play", WhiskerValue::Null);
    }

    /// Pauses playback.
    pub fn pause(&self) {
        let _ = self.r.command("pause", WhiskerValue::Null);
    }

    /// Shows the thumbnail of `video_id` without playing it (`cueVideoById`).
    pub fn cue_video(&self, video_id: &str) {
        let _ = self
            .r
            .command("cue", WhiskerValue::String(sanitize_video_id(video_id)));
    }

    /// Loads and plays `video_id` (`loadVideoById`).
    pub fn load_video(&self, video_id: &str) {
        let _ = self
            .r
            .command("load", WhiskerValue::String(sanitize_video_id(video_id)));
    }

    /// Seeks to `seconds` from the start.
    pub fn seek(&self, seconds: f64) {
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let _ = self.r.command("seek", WhiskerValue::Float(seconds));
    }

    /// Mutes the player.
    pub fn mute(&self) {
        let _ = self.r.command("mute", WhiskerValue::Null);
    }

    /// Unmutes the player.
    pub fn unmute(&self) {
        let _ = self.r.command("unmute", WhiskerValue::Null);
    }
}

impl Default for YouTubePlayerRef {
    fn default() -> Self {
        Self::new()
    }
}

// The page configuration (origin, html, autoplay, loop) is declared before the video so it is
// applied before the first video is cued.
#[doc(hidden)]
#[whisker::module_element(
    name = "masamune-whisker-youtube:YouTubePlayer",
    measurement = None,
    commands = [
        ("play", Null),
        ("pause", Null),
        ("cue", String),
        ("load", String),
        ("seek", Float),
        ("mute", Null),
        ("unmute", Null),
    ],
)]
pub fn native_you_tube_player(
    origin: Signal<String>,
    player_html: Signal<String>,
    autoplay: Signal<bool>,
    loop_video: Signal<bool>,
    video_id: Signal<String>,
    style: Style,
    on_ready: (),
    on_state_change: StateChangeEvent,
    on_error: ErrorEvent,
) {
}

/// Element schema referenced by the generated Host bootstrap code (Web, desktop).
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [native_you_tube_player_schema::element_provider()],
    )
}

/// Native YouTube player.
///
/// The first non-empty `video_id` is cued (or loaded and played with `autoplay`) once the player
/// is ready; later changes load and play the new video, like `loadVideoById`. An empty
/// `video_id` keeps the current video. The player has no intrinsic size: give it one through
/// `style` (for example `width: percent(100)` with a 16:9 aspect ratio, as [`YouTube`] does).
///
/// `on_error` receives YouTube's error code (`2` invalid parameter, `5` HTML5 player error,
/// `100` not found or private, `101` / `150` embedding not allowed, `153` missing referrer),
/// or `-1` when the IFrame API script could not be loaded. Fullscreen is disabled.
#[component]
pub fn you_tube_player(
    /// YouTube video ID (only `A-Z a-z 0-9 - _` are kept).
    #[prop(default = String::new().into())]
    video_id: Signal<String>,
    /// Play the first video as soon as it is ready instead of cueing it.
    #[prop(default = false.into())]
    autoplay: Signal<bool>,
    /// Restart the video when it ends.
    #[prop(default = false.into())]
    loop_video: Signal<bool>,
    /// Page origin and `playerVars.origin` (default: [`DEFAULT_ORIGIN`]).
    #[prop(default = DEFAULT_ORIGIN.to_string().into())]
    origin: Signal<String>,
    /// The player reported ready.
    on_ready: Option<Callback<()>>,
    /// The playback state changed.
    on_state_change: Option<Callback<PlayerState>>,
    /// The player reported an error code.
    on_error: Option<Callback<i32>>,
    /// Structured Whisker style.
    style: Option<Style>,
    /// Imperative handle.
    player_ref: Option<YouTubePlayerRef>,
) -> Element {
    let page = computed(move || player_html(&origin.get()));
    let id = computed(move || sanitize_video_id(&video_id.get()));
    let mut builder = NativeYouTubePlayer::builder()
        .origin(computed(move || html::normalize_origin(&origin.get())))
        .player_html(page)
        .autoplay(autoplay)
        .loop_video(loop_video)
        .video_id(id)
        .style(style.clone().unwrap_or_default())
        .on_ready(move || {
            if let Some(cb) = &on_ready {
                cb.run(());
            }
        })
        .on_state_change(move |event: StateChangeEvent| {
            let state = i32::try_from(event.detail.state)
                .ok()
                .and_then(PlayerState::from_code);
            if let (Some(cb), Some(state)) = (&on_state_change, state) {
                cb.run(state);
            }
        })
        .on_error(move |event: ErrorEvent| {
            if let Some(cb) = &on_error {
                cb.run(i32::try_from(event.detail.code).unwrap_or(-1));
            }
        });
    if let Some(handle) = player_ref {
        builder = builder.element_ref(handle.r());
    }
    builder.build()
}

/// Prepends `base` to `style`, so declarations of `style` win.
fn merge_style(base: Css, style: Option<Style>) -> Style {
    match style {
        None => Style::Typed(base),
        Some(Style::Typed(css)) => Style::Typed(base.merge(css)),
        Some(Style::DynamicTyped(f)) => {
            Style::DynamicTyped(Rc::new(move || base.clone().merge(f())))
        }
    }
}

/// Full-width 16:9 box.
fn video_box() -> Css {
    Css::new()
        .width(percent(100))
        .aspect_ratio(16.0, 9.0)
        .background_color(Color::Named(NamedColor::Black))
        .overflow(whisker::css::Overflow::Hidden)
}

/// Full-width 16:9 YouTube video (port of `VideoWidget`,
/// `flutter_royaltybank_package/lib/widgets/video.dart`).
///
/// Shows a black box while `content_id` is empty, during the first `defer_ms` milliseconds
/// after mounting (so the web view is not created during the page transition), and on hosts
/// without a native player. Then mounts a [`YouTubePlayer`] with [`DEFAULT_ORIGIN`] that cues
/// the video; a later change of `content_id` loads and plays the new video. `style` is applied
/// to the 16:9 box (for example a border radius).
#[component]
pub fn you_tube(
    /// YouTube video ID.
    #[prop(default = String::new().into())]
    content_id: Signal<String>,
    /// Delay in milliseconds before the player is created.
    #[prop(default = 300)]
    defer_ms: u32,
    /// Structured Whisker style of the 16:9 box.
    style: Option<Style>,
    /// Imperative handle of the player.
    player_ref: Option<YouTubePlayerRef>,
) -> Element {
    let deferred = signal(defer_ms == 0);
    if defer_ms > 0 {
        on_mount(move || {
            spawn_local(async move {
                tokio::time::sleep(Duration::from_millis(u64::from(defer_ms))).await;
                let _ = deferred.try_set(true);
            });
        });
    }
    let show_player = computed(move || {
        HAS_NATIVE_PLAYER && deferred.get() && !sanitize_video_id(&content_id.get()).is_empty()
    });
    let player_ref = player_ref.unwrap_or_default();
    let box_style = merge_style(video_box(), style.clone());
    let fill = css!(width: percent(100), height: percent(100));
    render! {
        View(style: box_style) {
            Show(when: move || show_player.get()) {
                YouTubePlayer(
                    video_id: content_id,
                    style: fill.clone(),
                    player_ref: player_ref,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_state_round_trips() {
        for (code, state) in [
            (-1, PlayerState::Unstarted),
            (0, PlayerState::Ended),
            (1, PlayerState::Playing),
            (2, PlayerState::Paused),
            (3, PlayerState::Buffering),
            (5, PlayerState::Cued),
        ] {
            assert_eq!(PlayerState::from_code(code), Some(state));
            assert_eq!(PlayerState::try_from(code), Ok(state));
            assert_eq!(i32::from(state), code);
        }
        assert_eq!(PlayerState::from_code(4), None);
        assert_eq!(PlayerState::try_from(42), Err(42));
    }

    #[test]
    fn schema_declares_commands_and_property_order() {
        let schema = native_you_tube_player_schema::schema();
        assert_eq!(schema.name, "masamune-whisker-youtube:YouTubePlayer");
        assert_eq!(
            schema
                .commands
                .iter()
                .map(|command| command.name.as_str())
                .collect::<Vec<_>>(),
            ["play", "pause", "cue", "load", "seek", "mute", "unmute"]
        );
        assert_eq!(
            schema
                .properties
                .iter()
                .map(|property| property.name.as_str())
                .collect::<Vec<_>>(),
            [
                "origin",
                "player-html",
                "autoplay",
                "loop-video",
                "video-id"
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
        assert_eq!(kind("autoplay"), whisker::ElementValueKind::Bool);
        assert_eq!(kind("loop-video"), whisker::ElementValueKind::Bool);
        assert_eq!(kind("video-id"), whisker::ElementValueKind::String);
    }

    #[test]
    fn event_bodies_deserialize() {
        let state: StateChangeEvent = WhiskerValue::map([(
            "detail",
            WhiskerValue::map([("state", WhiskerValue::Int(1))]),
        )])
        .deserialize_into()
        .unwrap();
        assert_eq!(state.detail.state, 1);

        let error: ErrorEvent = WhiskerValue::map([(
            "detail",
            WhiskerValue::map([("code", WhiskerValue::Int(150))]),
        )])
        .deserialize_into()
        .unwrap();
        assert_eq!(error.detail.code, 150);

        let empty: [(&str, WhiskerValue); 0] = [];
        let state: StateChangeEvent = WhiskerValue::map(empty).deserialize_into().unwrap();
        assert_eq!(state.detail.state, 0);
    }

    #[test]
    fn merged_style_keeps_box_and_overrides() {
        let merged = merge_style(video_box(), Some(css!(border_radius: px(8)).into()));
        let Style::Typed(css) = merged else {
            panic!("typed style expected");
        };
        let text = css.to_string();
        assert!(text.contains("aspect-ratio: 16 / 9;"), "{text}");
        assert!(text.contains("border-top-left-radius: 8px;"), "{text}");
    }
}
