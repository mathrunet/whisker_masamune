# masamune-whisker-audio-playlist

Gapless queue playback with announcement interrupts for Whisker apps, for example background
music that runs for hours and is interrupted by announcements.

- **iOS**: `AVQueuePlayer` for the queue (the next item is enqueued ahead, so it follows without
  a gap) and a separate `AVPlayer` for announcements.
- **Android**: a Media3 `ExoPlayer` playlist (`androidx.media3:media3-exoplayer` from Google
  Maven) and a separate `ExoPlayer` for announcements. Gapless for formats with encoder delay /
  padding metadata, such as AAC in MP4 (`.m4a`) and MP3 with a LAME or iTunes header.

Sources are `https://` URLs, `file://` URLs and absolute file paths (for example files
downloaded into the app's cache directory). Cleartext `http://` is rejected.

## Usage

```rust,ignore
use masamune_whisker_audio_playlist::{
    AudioPlaylist, EndReason, InterruptOptions, PlaylistEvent, QueueItem,
};

let playlist = AudioPlaylist::native();

// Keep the subscription as long as the playlist is used.
let feeder = playlist.clone();
let events = playlist.on_event(move |event| match event {
    // Top up while items still follow, so the handover stays gapless.
    PlaylistEvent::ItemStarted { id, remaining } if remaining < 2 => {
        let _ = feeder.append(&next_tracks());
    }
    PlaylistEvent::ItemEnded { id, reason: EndReason::Completed } => { /* log the play */ }
    // Nothing followed in time: appended items start right away while playing.
    PlaylistEvent::QueueDrained => { let _ = feeder.append(&next_tracks()); }
    PlaylistEvent::Error { id, message } => { /* the playlist already moved on */ }
    PlaylistEvent::InterruptEnded { id, completed } => { /* the queue is restored */ }
    _ => {}
});

playlist.set_queue(&[
    QueueItem::new("track-1", "/data/user/0/app/cache/bgm/track-1.m4a"),
    QueueItem::new("track-2", "https://example.com/bgm/track-2.m4a"),
])?;
playlist.set_volume(0.8)?;
playlist.play()?;

// Announcement: pause the music, play the announcement once, resume at the same position.
let id = playlist.interrupt("/data/user/0/app/cache/announce/open.m4a", InterruptOptions::default())?;
// Or keep the music at 20 % of its volume underneath.
playlist.interrupt("https://example.com/announce/sale.m4a", InterruptOptions::duck(0.2))?;

let status = playlist.status()?; // current_id, position_ms, duration_ms, queue, ...
playlist.skip_to_next()?;
playlist.remove("track-2")?;
playlist.pause()?;
playlist.stop()?; // also clears the queue
```

### API

- `AudioPlaylistAdapter`: the queue operations and events. Implemented by
  `NativeAudioPlaylistAdapter` (the OS players) and `RuntimeAudioPlaylistAdapter` (in memory).
- `AudioPlaylist`: cheap-to-clone handle that validates the arguments (non-empty unique IDs,
  sources, finite volumes clamped to `0.0..=1.0`). `AudioPlaylist::native()` controls the one
  process-wide native playlist; `AudioPlaylist::new(adapter)` takes any adapter.
  - Queue: `set_queue`, `append`, `remove`, `skip_to_next`.
  - Playback: `play`, `pause`, `stop`, `set_volume`.
  - State: `status` (`PlaylistStatus`), `current_id`, `position_ms`.
  - Announcements: `interrupt(uri, InterruptOptions)` returns the announcement ID,
    `cancel_interrupt`.
  - Events: `on_event` returns a `Subscription`; events stop when it is dropped.
- `PlaylistEvent`: `ItemStarted { id, remaining }`, `ItemEnded { id, reason }`
  (`Completed`, `Skipped`, `Removed`, `Failed`, `Stopped`, `Expired`), `QueueDrained`,
  `Error { id, message }`, `InterruptStarted { id }`, `InterruptEnded { id, completed }`,
  `OutputStarted { id, first_frame_ns, earliest_ns, latest_ns }`,
  `OutputEnded { id, last_frame_ns }`.
- Output deadlines: `QueueItem::with_output_before_ns`, `InterruptOptions::with_output_before_ns`
  and `AudioPlaylist::set_output_deadline(id, ns)` (only shortens). Values are on the
  `output_clock_ns()` clock, a monotonic clock that keeps counting while the device sleeps
  (`mach_continuous_time` on Apple platforms, `CLOCK_BOOTTIME` /
  `SystemClock.elapsedRealtimeNanos` on Android).

### Behavior

- **Output deadlines**: no frame of an item or announcement is presented at or after its
  deadline. The native players stop it early enough to cover the output latency (iOS: the audio
  session's `outputLatency` + `ioBufferDuration`; Android: a 150 ms bound) plus two periods of
  the 40 ms monitor, end it with `ItemEnded { reason: Expired }` (announcements:
  `InterruptEnded { completed: false }`), and never start an item whose deadline is closer than
  that margin. This also holds with the screen locked, as long as the app keeps running in the
  background audio mode.
- **Output events**: `OutputStarted` reports that the first frame of an item or announcement
  reached the output (between `earliest_ns` and `latest_ns`, detected from the advancing
  playback position, so the interval is at most one monitor period plus the output latency
  wide); `OutputEnded` reports that its audio stopped, with an upper bound of the last presented
  frame. They are reported again when the output resumes (for example after a pausing
  announcement). `RuntimeAudioPlaylistAdapter` reports them only after
  `set_output_reporting(true)`.

- The queue holds the current item first. Finished, skipped, failed and removed items leave it.
- `set_queue` replaces the queue (the current item ends with `Removed`); use `append` to extend
  it without a gap. With an empty queue, a playing playlist starts the next appended item.
- A source that cannot be loaded reports `Error` and `ItemEnded { reason: Failed }`, and the
  playlist moves on, so background music keeps going.
- `QueueDrained` follows when the last item ends and nothing follows.
- Only one announcement plays at a time: a new `interrupt` cancels the running one
  (`InterruptEnded { completed: false }`) without resuming the queue in between. `pause` during
  an announcement keeps the queue paused afterwards; `stop` cancels the announcement.
- Neither player requests audio focus or reacts to interruptions (calls, other apps); see
  below.

## Background playback

The crate only plays audio. Background playback, audio focus, interruptions and the lock screen
come from [`masamune-whisker-audio-session`](../masamune-whisker-audio-session/README.md), which
is required for music that keeps playing with the screen off:

- **iOS**: add the `audio` background mode with the audio-session CNG plugin
  (`c.background_audio(true)`). The playlist sets the `.playback` category only when the session
  has another category, so the category and options configured by audio-session are kept.
- **Android**: the process is kept alive by audio-session's `mediaPlayback` foreground service,
  which starts with the first `set_now_playing` call. Call it while the app is in the foreground
  (Android 12 and later refuse to start the service from the background). The playlist players
  hold a wake lock and Wi-Fi lock while playing, so streaming continues with the screen off; this
  crate's manifest declares `WAKE_LOCK`, `ACCESS_WIFI_STATE` and `INTERNET`.

```rust,ignore
use masamune_whisker_audio_session::{
    self as audio_session, NowPlaying, PlaybackState, ProcessingState, RemoteCommand,
};

// Startup: AudioSessionMasamuneAdapter configures the session (see its README).
audio_session::set_active(true)?;

// Mirror the playlist on the lock screen / media notification.
let events = playlist.on_event(move |event| {
    if let PlaylistEvent::ItemStarted { id, .. } = event {
        let _ = audio_session::set_now_playing(&NowPlaying {
            id: id.clone(),
            title: title_of(&id),
            ..Default::default()
        });
        let _ = audio_session::set_playback_state(&PlaybackState {
            playing: true,
            state: ProcessingState::Ready,
            ..Default::default()
        });
    }
});

// Drive the playlist from the lock screen, headphones and interruptions.
let player = playlist.clone();
let commands = audio_session::on_command(move |command| match command {
    RemoteCommand::Play => { let _ = player.play(); }
    RemoteCommand::Pause | RemoteCommand::Stop => { let _ = player.pause(); }
    RemoteCommand::Next => { let _ = player.skip_to_next(); }
    _ => {}
});
let player = playlist.clone();
let interruptions = audio_session::on_interruption(move |event| {
    if event.begin { let _ = player.pause(); } else if event.should_resume { let _ = player.play(); }
});
```

## Tests

`RuntimeAudioPlaylistAdapter` behaves like the native players without audio: time moves only
with `advance(ms)`, every source lasts three minutes unless set with `set_duration(uri, ms)`,
and `fail(uri)` makes a source fail. Events are delivered synchronously and recorded
(`events`, `take_events`); handlers may call the playlist again.

```rust
use masamune_whisker_audio_playlist::{
    AudioPlaylist, EndReason, InterruptOptions, PlaylistEvent, QueueItem,
    RuntimeAudioPlaylistAdapter,
};

let adapter = RuntimeAudioPlaylistAdapter::new();
adapter.set_default_duration(1_000);
adapter.set_duration("/announce.m4a", 300);
let playlist = AudioPlaylist::new(adapter.clone());

playlist.set_queue(&[QueueItem::new("a", "/a.m4a"), QueueItem::new("b", "/b.m4a")])?;
playlist.play()?;
adapter.advance(400);
playlist.interrupt("/announce.m4a", InterruptOptions::default())?;
adapter.advance(300); // the announcement; the queue stays at 400 ms
adapter.advance(700); // "a" finishes and "b" starts without a gap
assert_eq!(playlist.current_id()?.as_deref(), Some("b"));
assert!(adapter.events().contains(&PlaylistEvent::ItemEnded {
    id: "a".into(),
    reason: EndReason::Completed,
}));
# Ok::<(), masamune_whisker_audio_playlist::NativeError>(())
```

Use the same adapter in the app's `test` flavor to run E2E flows without audio.
