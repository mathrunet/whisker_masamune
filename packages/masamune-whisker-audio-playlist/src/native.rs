use std::rc::Rc;

use masamune_whisker_bridge::NativeModule;
use serde_json::json;

use crate::{
    AudioPlaylistAdapter, InterruptRequest, NativeResult, PlaylistEvent, PlaylistStatus, QueueItem,
    Subscription,
};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "AudioPlaylist")
}

/// The native queue player.
///
/// - iOS: `AVQueuePlayer` for the queue (items are enqueued ahead, so the next item follows
///   without a gap) and a separate `AVPlayer` for announcements. When the audio session category
///   is neither `.playback` nor `.playAndRecord`, `.playback` is set before playing; a category
///   configured by `masamune-whisker-audio-session` (with its options) is kept.
/// - Android: Media3 `ExoPlayer` with a playlist (gapless for formats with encoder delay and
///   padding metadata, such as AAC in MP4 and MP3 with a LAME / iTunes header) and a separate
///   `ExoPlayer` for announcements. Both use music / speech audio attributes without handling
///   audio focus themselves, and hold a wake lock and Wi-Fi lock while playing
///   (`C.WAKE_MODE_NETWORK`).
///
/// One queue player and one announcement player exist per process. Calls run on the UI thread.
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeAudioPlaylistAdapter;

impl AudioPlaylistAdapter for NativeAudioPlaylistAdapter {
    fn set_queue(&self, items: &[QueueItem]) -> NativeResult<()> {
        module().call::<_, bool>("setQueue", &json!({ "items": items }))?;
        Ok(())
    }

    fn append(&self, items: &[QueueItem]) -> NativeResult<()> {
        module().call::<_, bool>("append", &json!({ "items": items }))?;
        Ok(())
    }

    fn remove(&self, id: &str) -> NativeResult<bool> {
        module().call("remove", &json!({ "id": id }))
    }

    fn play(&self) -> NativeResult<()> {
        module().call::<_, bool>("play", &json!({}))?;
        Ok(())
    }

    fn pause(&self) -> NativeResult<()> {
        module().call::<_, bool>("pause", &json!({}))?;
        Ok(())
    }

    fn stop(&self) -> NativeResult<()> {
        module().call::<_, bool>("stop", &json!({}))?;
        Ok(())
    }

    fn skip_to_next(&self) -> NativeResult<bool> {
        module().call("skipToNext", &json!({}))
    }

    fn set_volume(&self, volume: f64) -> NativeResult<()> {
        module().call::<_, bool>("setVolume", &json!({ "volume": volume }))?;
        Ok(())
    }

    fn status(&self) -> NativeResult<PlaylistStatus> {
        module().call("status", &json!({}))
    }

    fn interrupt(&self, request: &InterruptRequest) -> NativeResult<()> {
        module().call::<_, bool>("interrupt", request)?;
        Ok(())
    }

    fn cancel_interrupt(&self) -> NativeResult<bool> {
        module().call("cancelInterrupt", &json!({}))
    }

    fn set_output_deadline(&self, id: &str, output_before_ns: u64) -> NativeResult<bool> {
        module().call(
            "setOutputDeadline",
            &json!({ "id": id, "outputBeforeNs": output_before_ns }),
        )
    }

    fn subscribe(&self, handler: Rc<dyn Fn(PlaylistEvent)>) -> Subscription {
        module().subscribe("event", move |event: PlaylistEvent| handler(event))
    }
}
