# masamune-whisker-audio-session

Audio session, lock screen / Control Center controls and background playback for Whisker apps
(the counterpart of Flutter's `audio_session` and `audio_service`).

- **iOS**: `AVAudioSession` (`.playback`), interruption and route change notifications,
  `MPNowPlayingInfoCenter` and `MPRemoteCommandCenter`.
- **Android**: audio focus (`AudioFocusRequest`), `ACTION_AUDIO_BECOMING_NOISY` and
  `AudioDeviceCallback`, a framework `MediaSession` with a media style notification, and a
  `mediaPlayback` foreground service that keeps the process alive (with a partial wake lock while
  playing).

The crate does not play audio. It works next to any player (for example `whisker-audio`): the app
reports what plays with `set_now_playing` / `set_playback_state` and drives its players from
`on_command`.

## Setup

### iOS

Background playback needs `audio` in `UIBackgroundModes`. Add it with the CNG plugin (it merges
with the modes set by other plugins):

```rust,ignore
use masamune_whisker_audio_session::cng::MasamuneAudioSession;

app.project_plugin::<MasamuneAudioSession>(|c| {
    c.background_audio(true);
});
```

`whisker_audio::WhiskerAudio` with `enable_background_playback(true)` sets the same key.

### Android

Nothing to add to the app: the crate's `AndroidManifest.xml` declares

- `<service android:name="net.mathru.whisker.audio_session.AudioSessionService"
  android:foregroundServiceType="mediaPlayback" android:exported="false">`
- `android.permission.FOREGROUND_SERVICE`, `android.permission.FOREGROUND_SERVICE_MEDIA_PLAYBACK`,
  `android.permission.WAKE_LOCK` and `android.permission.POST_NOTIFICATIONS`.

On Android 13 and later the media notification is shown only after the user allowed
notifications (request it with `masamune-whisker-local-notification`); playback and the
foreground service run either way. Google Play requires the foreground service type declaration
for `mediaPlayback` in the Play Console.

## Usage

```rust,ignore
use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_audio_session::{
    self as audio_session, AudioSessionConfig, AudioSessionMasamuneAdapter, CustomAction,
    IosFeedbackCommand, MediaControls, NowPlaying, PlaybackState, ProcessingState, RemoteCommand,
};

// Startup: configures the session and the Android notification channel.
MasamuneAdapters::new()
    .adapter(AudioSessionMasamuneAdapter::new(AudioSessionConfig {
        channel_id: "net.mathru.segue.audio".into(),
        channel_name: "Soundtrack".into(),
        channel_description: "Keeps playing and shows the lock screen controls.".into(),
        small_icon: "mipmap/ic_launcher_notification".into(),
        ..AudioSessionConfig::music()
    }))
    .install();

// Keep the subscriptions alive as long as the player exists.
let commands = audio_session::on_command(|command| match command {
    RemoteCommand::Play => { /* resume */ }
    RemoteCommand::Pause | RemoteCommand::Stop => { /* pause */ }
    RemoteCommand::TogglePlayPause => {}
    RemoteCommand::Next => {}
    RemoteCommand::Previous => {}
    RemoteCommand::Seek { position_ms } => {}
    RemoteCommand::Custom { id } => { /* "save" */ }
});
let interruptions = audio_session::on_interruption(|event| {
    if event.begin { /* pause */ } else if event.should_resume { /* resume */ }
});
let noisy = audio_session::on_becoming_noisy(|| { /* pause */ });

// When playback starts (while the app is in the foreground on Android).
audio_session::set_active(true)?;
audio_session::set_controls(&MediaControls {
    custom: vec![CustomAction {
        id: "save".into(),
        label: "Keep this track".into(),
        android_icon: "drawable/save_track_outline".into(),
        ios_command: Some(IosFeedbackCommand::Like),
        active: false,
    }],
    ..Default::default()
})?;
audio_session::set_now_playing(&NowPlaying {
    id: "track-1".into(),
    title: "Morning Segue".into(),
    artist: "Segue".into(),
    artwork_path: Some("/path/to/artwork.png".into()),
    duration_ms: Some(180_000),
    ..Default::default()
})?;
audio_session::set_playback_state(&PlaybackState {
    playing: true,
    state: ProcessingState::Ready,
    position_ms: 0,
    speed: 1.0,
})?;

// When the user stops playback for good.
audio_session::clear_now_playing()?;
audio_session::set_active(false)?;
```

## Behavior and limits

- Interruptions: iOS reports `begin` / end with `should_resume`. Android maps
  `AUDIOFOCUS_LOSS_TRANSIENT` to `Pause`, `AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK` to `Duck`,
  `AUDIOFOCUS_LOSS` to `Unknown` (no end event) and regaining focus after a transient loss to an
  end event with `should_resume`.
- The Android foreground service starts with the first `set_now_playing` and stops with
  `clear_now_playing`. Android 12 and later refuse to start it from the background; the call then
  fails with an error and playback continues without it.
- Android Auto browsing (`getChildren`) and the iOS `MPRemoteCommandCenter` skip intervals are not
  provided.
- iOS suspends the Rust runtime in the background unless audio is playing, so commands that arrive
  while suspended are handled when the app runs again.
