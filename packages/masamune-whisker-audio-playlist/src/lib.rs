#![doc = include_str!("../README.md")]

mod native;
mod runtime;
mod types;

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
pub use native::NativeAudioPlaylistAdapter;
pub use runtime::RuntimeAudioPlaylistAdapter;
pub use types::{
    EndReason, InterruptMode, InterruptOptions, InterruptRequest, PlaylistEvent, PlaylistStatus,
    QueueItem, validate_uri,
};

/// A gapless audio queue with announcement interrupts.
///
/// Implemented by [`NativeAudioPlaylistAdapter`] (iOS `AVQueuePlayer`, Android Media3
/// `ExoPlayer`) and [`RuntimeAudioPlaylistAdapter`] (in memory, for tests and previews). Use it
/// through an [`AudioPlaylist`] handle, which validates the arguments before they reach the
/// adapter.
///
/// The queue holds the current item first and the items that follow it. Finished, skipped,
/// failed and removed items leave the queue. Every adapter reports the same
/// [`PlaylistEvent`]s in the same order for the same calls; see each method.
pub trait AudioPlaylistAdapter {
    /// Replaces the whole queue. The current item ends with [`EndReason::Removed`], and the
    /// first new item becomes current ([`PlaylistEvent::ItemStarted`]) and starts from the
    /// beginning when the playlist is playing. An empty list clears the queue without
    /// [`PlaylistEvent::QueueDrained`]. Fails when two items share an ID.
    fn set_queue(&self, items: &[QueueItem]) -> NativeResult<()>;

    /// Adds items after the last one without interrupting the current item. When the queue was
    /// empty, the first new item becomes current. Fails when an ID is already queued.
    fn append(&self, items: &[QueueItem]) -> NativeResult<()>;

    /// Removes the item with `id`. Removing the current item ends it with
    /// [`EndReason::Removed`] and moves to the next one. Returns whether the item was queued.
    fn remove(&self, id: &str) -> NativeResult<bool>;

    /// Starts or resumes playback. While a [`InterruptMode::Pause`] announcement plays, the
    /// queue resumes once the announcement ends. With an empty queue the playlist stays in the
    /// playing state and starts the next appended item.
    fn play(&self) -> NativeResult<()>;

    /// Pauses at the current position. During an announcement the announcement keeps playing
    /// and the queue no longer resumes after it.
    fn pause(&self) -> NativeResult<()>;

    /// Stops playback: cancels the announcement, ends the current item with
    /// [`EndReason::Stopped`] and clears the queue.
    fn stop(&self) -> NativeResult<()>;

    /// Ends the current item with [`EndReason::Skipped`] and moves to the next one. Returns
    /// whether a next item exists; [`PlaylistEvent::QueueDrained`] follows when it does not.
    fn skip_to_next(&self) -> NativeResult<bool>;

    /// Volume of the queue from `0.0` to `1.0` (already clamped by [`AudioPlaylist`]).
    fn set_volume(&self, volume: f64) -> NativeResult<()>;

    /// Current item, position and queue.
    fn status(&self) -> NativeResult<PlaylistStatus>;

    /// Plays an announcement once on a separate player, pausing or ducking the queue, then
    /// restores the queue at the same position. A running announcement is cancelled first
    /// ([`PlaylistEvent::InterruptEnded`] with `completed: false`) without resuming the queue in
    /// between.
    fn interrupt(&self, request: &InterruptRequest) -> NativeResult<()>;

    /// Stops the running announcement and restores the queue. Returns whether one was playing.
    fn cancel_interrupt(&self) -> NativeResult<bool>;

    /// Receives events while the returned [`Subscription`] is held.
    fn subscribe(&self, handler: Rc<dyn Fn(PlaylistEvent)>) -> Subscription;
}

static NEXT_INTERRUPT: AtomicU64 = AtomicU64::new(1);

/// Handle to an [`AudioPlaylistAdapter`]. Cheap to clone; clones share the adapter.
///
/// The native adapter drives one process-wide queue player and one announcement player, so
/// every [`AudioPlaylist::native`] handle controls the same playlist.
///
/// ```no_run
/// use masamune_whisker_audio_playlist::{AudioPlaylist, PlaylistEvent, QueueItem};
///
/// let playlist = AudioPlaylist::native();
/// let feeder = playlist.clone();
/// // Keep the subscription as long as the playlist is used.
/// let _events = playlist.on_event(move |event| {
///     if let PlaylistEvent::ItemStarted { remaining: 0, .. } = event {
///         // Top up before the queue runs dry so the next track follows without a gap.
///         let _ = feeder.append(&[QueueItem::new("t3", "https://example.com/t3.m4a")]);
///     }
/// });
/// playlist.set_queue(&[
///     QueueItem::new("t1", "/path/to/cache/t1.m4a"),
///     QueueItem::new("t2", "https://example.com/t2.m4a"),
/// ])?;
/// playlist.play()?;
/// # Ok::<(), masamune_whisker_audio_playlist::NativeError>(())
/// ```
#[derive(Clone)]
pub struct AudioPlaylist {
    adapter: Rc<dyn AudioPlaylistAdapter>,
}

impl AudioPlaylist {
    /// A handle to `adapter`.
    pub fn new(adapter: impl AudioPlaylistAdapter + 'static) -> Self {
        Self {
            adapter: Rc::new(adapter),
        }
    }

    /// A handle to the native player ([`NativeAudioPlaylistAdapter`]).
    pub fn native() -> Self {
        Self::new(NativeAudioPlaylistAdapter)
    }

    /// Replaces the queue. See [`AudioPlaylistAdapter::set_queue`].
    pub fn set_queue(&self, items: &[QueueItem]) -> NativeResult<()> {
        validate_items(items)?;
        self.adapter.set_queue(items)
    }

    /// Adds items at the end. See [`AudioPlaylistAdapter::append`].
    pub fn append(&self, items: &[QueueItem]) -> NativeResult<()> {
        validate_items(items)?;
        if items.is_empty() {
            return Ok(());
        }
        self.adapter.append(items)
    }

    /// Removes an item by ID. See [`AudioPlaylistAdapter::remove`].
    pub fn remove(&self, id: &str) -> NativeResult<bool> {
        self.adapter.remove(id)
    }

    /// Starts or resumes playback. See [`AudioPlaylistAdapter::play`].
    pub fn play(&self) -> NativeResult<()> {
        self.adapter.play()
    }

    /// Pauses. See [`AudioPlaylistAdapter::pause`].
    pub fn pause(&self) -> NativeResult<()> {
        self.adapter.pause()
    }

    /// Stops and clears the queue. See [`AudioPlaylistAdapter::stop`].
    pub fn stop(&self) -> NativeResult<()> {
        self.adapter.stop()
    }

    /// Skips to the next item. See [`AudioPlaylistAdapter::skip_to_next`].
    pub fn skip_to_next(&self) -> NativeResult<bool> {
        self.adapter.skip_to_next()
    }

    /// Sets the queue volume, clamped to `0.0..=1.0`. Fails for NaN or infinite values.
    pub fn set_volume(&self, volume: f64) -> NativeResult<()> {
        self.adapter.set_volume(clamp_volume(volume)?)
    }

    /// Current item, position and queue.
    pub fn status(&self) -> NativeResult<PlaylistStatus> {
        self.adapter.status()
    }

    /// ID of the current item.
    pub fn current_id(&self) -> NativeResult<Option<String>> {
        Ok(self.status()?.current_id)
    }

    /// Position in the current item in milliseconds (`0` without a current item).
    pub fn position_ms(&self) -> NativeResult<u64> {
        Ok(self.status()?.position_ms)
    }

    /// Plays the announcement at `uri` once and restores the queue afterwards. Returns the ID
    /// reported by [`PlaylistEvent::InterruptStarted`] and [`PlaylistEvent::InterruptEnded`].
    /// See [`AudioPlaylistAdapter::interrupt`].
    pub fn interrupt(&self, uri: &str, options: InterruptOptions) -> NativeResult<String> {
        validate_uri(uri)?;
        let volume = clamp_volume(options.volume)?;
        let mode = match options.mode {
            InterruptMode::Pause => InterruptMode::Pause,
            InterruptMode::Duck { volume } => InterruptMode::Duck {
                volume: clamp_volume(volume)?,
            },
        };
        let id = format!(
            "interrupt-{}",
            NEXT_INTERRUPT.fetch_add(1, Ordering::Relaxed)
        );
        self.adapter.interrupt(&InterruptRequest {
            id: id.clone(),
            uri: uri.to_owned(),
            mode,
            volume,
        })?;
        Ok(id)
    }

    /// Stops the running announcement. See [`AudioPlaylistAdapter::cancel_interrupt`].
    pub fn cancel_interrupt(&self) -> NativeResult<bool> {
        self.adapter.cancel_interrupt()
    }

    /// Receives [`PlaylistEvent`]s while the returned [`Subscription`] is held.
    pub fn on_event(&self, handler: impl Fn(PlaylistEvent) + 'static) -> Subscription {
        self.adapter.subscribe(Rc::new(handler))
    }
}

impl std::fmt::Debug for AudioPlaylist {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioPlaylist").finish_non_exhaustive()
    }
}

pub(crate) fn validate_items(items: &[QueueItem]) -> NativeResult<()> {
    for (index, item) in items.iter().enumerate() {
        if item.id.is_empty() {
            return Err(NativeError("queue item id is empty".into()));
        }
        validate_uri(&item.uri)?;
        if items[..index].iter().any(|other| other.id == item.id) {
            return Err(NativeError(format!("duplicate queue item id: {}", item.id)));
        }
    }
    Ok(())
}

fn clamp_volume(volume: f64) -> NativeResult<f64> {
    if !volume.is_finite() {
        return Err(NativeError("volume must be a finite number".into()));
    }
    Ok(volume.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_items_and_volumes() {
        assert!(validate_items(&[QueueItem::new("a", "https://example.com/a.m4a")]).is_ok());
        assert!(validate_items(&[QueueItem::new("", "https://example.com/a.m4a")]).is_err());
        assert!(
            validate_items(&[
                QueueItem::new("a", "/cache/a.m4a"),
                QueueItem::new("a", "/cache/b.m4a"),
            ])
            .is_err()
        );
        assert!(validate_items(&[QueueItem::new("a", "http://example.com/a.m4a")]).is_err());
        assert_eq!(clamp_volume(1.5).unwrap(), 1.0);
        assert_eq!(clamp_volume(-1.0).unwrap(), 0.0);
        assert!(clamp_volume(f64::NAN).is_err());
    }
}
