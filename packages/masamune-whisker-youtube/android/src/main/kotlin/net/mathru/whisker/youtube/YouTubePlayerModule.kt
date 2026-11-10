// Module definition of masamune-whisker-youtube (Android). KSP registers the
// `masamune-whisker-youtube:YouTubePlayer` element; the view lives in YouTubePlayerView.kt.

package net.mathru.whisker.youtube

import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class YouTubePlayerModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("YouTubePlayer")
        View("masamune-whisker-youtube:YouTubePlayer", YouTubePlayerView::class.java) {
            // Page configuration (declared before the video in the Rust schema).
            Prop("origin", clear = { view: YouTubePlayerView -> view.setOrigin("") }) { view: YouTubePlayerView, value ->
                view.setOrigin(value.asString() ?: "")
            }
            Prop("player-html", clear = { view: YouTubePlayerView -> view.setPlayerHtml("") }) { view: YouTubePlayerView, value ->
                view.setPlayerHtml(value.asString() ?: "")
            }
            Prop("autoplay", clear = { view: YouTubePlayerView -> view.setAutoplay(false) }) { view: YouTubePlayerView, value ->
                view.setAutoplay(value.asBool() ?: false)
            }
            Prop("loop-video", clear = { view: YouTubePlayerView -> view.setLoopVideo(false) }) { view: YouTubePlayerView, value ->
                view.setLoopVideo(value.asBool() ?: false)
            }
            Prop("video-id", clear = { view: YouTubePlayerView -> view.setVideoId("") }) { view: YouTubePlayerView, value ->
                view.setVideoId(value.asString() ?: "")
            }

            Events("ready", "state_change", "error")

            Command("play") { view: YouTubePlayerView, _ -> view.play() }
            Command("pause") { view: YouTubePlayerView, _ -> view.pause() }
            Command("cue") { view: YouTubePlayerView, parameters ->
                view.cueVideo(parameters.asString() ?: "")
            }
            Command("load") { view: YouTubePlayerView, parameters ->
                view.loadVideo(parameters.asString() ?: "")
            }
            Command("seek") { view: YouTubePlayerView, parameters ->
                view.seek(parameters.asDouble() ?: 0.0)
            }
            Command("mute") { view: YouTubePlayerView, _ -> view.mute() }
            Command("unmute") { view: YouTubePlayerView, _ -> view.unmute() }
        }
    }
}
