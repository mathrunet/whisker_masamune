# masamune-whisker-youtube

YouTube player for Whisker apps: the [YouTube IFrame Player API](https://developers.google.com/youtube/iframe_api_reference)
hosted in a native web view (`WKWebView` on iOS, `android.webkit.WebView` on Android). No
third-party SDK is used.

- `YouTube`: a full-width 16:9 box. It stays black while the video ID is empty and for
  `defer_ms` (300 ms by default) after mounting, so the web view is not created during the page
  transition, then cues the video. A later change of the ID loads and plays the new video.
  Port of `VideoWidget` (`flutter_royaltybank_package/lib/widgets/video.dart`).
- `YouTubePlayer`: the native player with props, events and an imperative `YouTubePlayerRef`.

Hosts other than iOS and Android (desktop, Web, host tests) show the black box only.

## Usage

```toml
[dependencies]
masamune-whisker-youtube.workspace = true
```

```rust
// Compiled by tests/usage.rs.
use masamune_whisker_youtube::{PlayerState, YouTube, YouTubePlayer, YouTubePlayerRef};
use whisker::prelude::*;

#[component]
pub fn content_page(movie_id: Signal<String>) -> Element {
    let player = YouTubePlayerRef::new();
    render! {
        View(style: css!(flex_direction: FlexDirection::Column, padding: px(16))) {
            // 16:9 box with rounded corners; black until the ID is set.
            YouTube(content_id: movie_id, style: css!(border_radius: px(8)))

            // Lower level: your own size, events and commands.
            YouTubePlayer(
                video_id: "dQw4w9WgXcQ",
                loop_video: true,
                player_ref: player,
                on_ready: move |_| player.mute(),
                on_state_change: move |_state: PlayerState| {},
                on_error: move |_code: i32| {},
                style: css!(width: percent(100), height: px(200)),
            )
            Text(value: "Pause", on_tap: move |_| player.pause())
        }
    }
}
```

## `YouTubePlayer`

| Prop | Default | Description |
|---|---|---|
| `video_id` | `""` | Video ID. The first one is cued (or loaded with `autoplay`) when the player is ready; later changes load and play the new video. Empty keeps the current video. |
| `autoplay` | `false` | Play the first video as soon as it is ready. |
| `loop_video` | `false` | Restart the video when it ends. |
| `origin` | `DEFAULT_ORIGIN` | Base URL of the page and `playerVars.origin` (`https://www.youtube-nocookie.com`). |
| `on_ready` | | The player is ready. |
| `on_state_change` | | `PlayerState` (`Unstarted`, `Ended`, `Playing`, `Paused`, `Buffering`, `Cued`). |
| `on_error` | | YouTube error code (`2`, `5`, `100`, `101`, `150`, `153`), or `-1` when the IFrame API could not be loaded. |
| `style` | | Size it yourself; the element has no intrinsic size. |
| `player_ref` | | `YouTubePlayerRef`: `play`, `pause`, `cue_video`, `load_video`, `seek`, `mute`, `unmute`. |

`YouTubePlayerRef` methods are no-ops while the player is not mounted. Playback commands sent
before the player is ready are ignored; `cue_video` / `load_video` are kept and applied once it
is ready. Video IDs keep only `A-Z a-z 0-9 - _` (`sanitize_video_id`).

## Behavior

- The page is built in Rust (`player_html(origin)`) and loaded with `origin` as its base URL.
  It loads `https://www.youtube.com/iframe_api` and creates the player with `playsinline`,
  `rel=0`, `controls`, `fs=0` and `enablejsapi`.
- Inline playback without a user gesture is allowed. Fullscreen is not supported: the
  fullscreen button is hidden and Android fullscreen requests are dismissed.
- Links that leave the player ("Watch on YouTube", the channel name) open in the YouTube app or
  the browser instead of the player.
- Android: the web view is destroyed when the element leaves the window and created again
  (re-cueing the current video) when it comes back or after its render process ended. The
  library manifest adds the `INTERNET` permission.
- iOS: the page reloads after the web content process ended.

Native sources: `ios/Sources/MasamuneWhiskerYoutube/` and
`android/src/main/kotlin/net/mathru/whisker/youtube/`.
