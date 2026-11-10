// Module definition of masamune-whisker-youtube (iOS). The codegen plugin registers the
// `masamune-whisker-youtube:YouTubePlayer` element; the view lives in YouTubePlayerView.swift.

import WhiskerModule

@WhiskerModule
public final class YouTubePlayerModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("YouTubePlayer")
            View("masamune-whisker-youtube:YouTubePlayer", YouTubePlayerView.self) {
                // Page configuration (declared before the video in the Rust schema).
                Prop("origin", clear: { (view: YouTubePlayerView) in view.setOrigin("") }) { (view: YouTubePlayerView, value: WhiskerValue) in
                    view.setOrigin(value.asString ?? "")
                }
                Prop("player-html", clear: { (view: YouTubePlayerView) in view.setPlayerHtml("") }) { (view: YouTubePlayerView, value: WhiskerValue) in
                    view.setPlayerHtml(value.asString ?? "")
                }
                Prop("autoplay", clear: { (view: YouTubePlayerView) in view.setAutoplay(false) }) { (view: YouTubePlayerView, value: WhiskerValue) in
                    view.setAutoplay(value.asBool ?? false)
                }
                Prop("loop-video", clear: { (view: YouTubePlayerView) in view.setLoopVideo(false) }) { (view: YouTubePlayerView, value: WhiskerValue) in
                    view.setLoopVideo(value.asBool ?? false)
                }
                Prop("video-id", clear: { (view: YouTubePlayerView) in view.setVideoId("") }) { (view: YouTubePlayerView, value: WhiskerValue) in
                    view.setVideoId(value.asString ?? "")
                }

                Events("ready", "state_change", "error")

                Command("play") { (view: YouTubePlayerView, _: WhiskerValue) in view.play() }
                Command("pause") { (view: YouTubePlayerView, _: WhiskerValue) in view.pause() }
                Command("cue") { (view: YouTubePlayerView, parameters: WhiskerValue) in
                    view.cueVideo(parameters.asString ?? "")
                }
                Command("load") { (view: YouTubePlayerView, parameters: WhiskerValue) in
                    view.loadVideo(parameters.asString ?? "")
                }
                Command("seek") { (view: YouTubePlayerView, parameters: WhiskerValue) in
                    view.seek(parameters.asDouble ?? 0)
                }
                Command("mute") { (view: YouTubePlayerView, _: WhiskerValue) in view.mute() }
                Command("unmute") { (view: YouTubePlayerView, _: WhiskerValue) in view.unmute() }
            }
        }
    }
}
