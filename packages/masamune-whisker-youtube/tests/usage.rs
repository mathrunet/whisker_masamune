//! Compile check of the README usage (not run).

use masamune_whisker_youtube::{PlayerState, YouTube, YouTubePlayer, YouTubePlayerRef};
use whisker::prelude::*;

#[allow(dead_code)]
#[component]
fn content_page(movie_id: Signal<String>) -> Element {
    let player = YouTubePlayerRef::new();
    render! {
        View(style: css!(flex_direction: FlexDirection::Column, padding: px(16))) {
            YouTube(content_id: movie_id, style: css!(border_radius: px(8)))
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

#[test]
fn usage_compiles() {}
