# masamune-whisker-audio-capture

Microphone capture for Whisker apps. Audio arrives as PCM16 little-endian mono chunks at the
requested sample rate (default 16 kHz, 100 ms per chunk), ready for streaming speech recognition.

| Platform | Implementation |
|---|---|
| iOS | `AVAudioEngine` input tap → `AVAudioConverter`, `AVAudioSession` `playAndRecord` while capturing |
| Android | `AudioRecord` (`VOICE_RECOGNITION`) on a background thread, runtime `RECORD_AUDIO` request |
| Web | `getUserMedia` → `ScriptProcessorNode`, resampled with linear interpolation |

This is a function-only native module without a CNG plugin. The app declares the microphone
permission itself:

- iOS: `NSMicrophoneUsageDescription` in `Info.plist` (iOS terminates the app without it).
- Android: `android.permission.RECORD_AUDIO` in the manifest.

`masamune-whisker-ai-debugger` (feature `voice`) depends on this crate and writes both with its own
CNG plugin. Because whisker-cng only links native modules of crates in the app's dependency graph,
an app that enables that feature for development builds only ships no recording code in
production.

## Usage

```rust,ignore
use masamune_whisker_audio_capture::{
    AudioCapture, AudioCaptureEvent, AudioCaptureOptions, is_permission_denied,
};

spawn_local(async move {
    let capture = AudioCapture::start(AudioCaptureOptions::default(), |event| match event {
        AudioCaptureEvent::Chunk(pcm) => send_to_recognizer(pcm),
        AudioCaptureEvent::Error(message) => show_error(message),
    })
    .await;
    let capture = match capture {
        Ok(capture) => capture,
        Err(error) if is_permission_denied(&error) => return ask_for_microphone(),
        Err(error) => return show_error(error.to_string()),
    };
    // ... later
    let rest = capture.stop().await.unwrap_or_default(); // audio after the last chunk
});
```

- Only one capture runs at a time; starting another stops the previous one.
- Dropping the handle without `stop` stops the capture and discards the remaining audio.
- `AudioCaptureEvent::Error` reports a capture that stopped on its own (an iOS audio session
  interruption or a failed read).

## Host protocol

Module `masamune-whisker-audio-capture:AudioCapture`, arguments and results are JSON strings
(`masamune-whisker-bridge`).

| Call | Arguments | Result |
|---|---|---|
| `start` (async) | `{ id, sampleRate, chunkMs }` | `true`, or the error `permission_denied` |
| `stop` (async) | `{ id }` | `{ data }` (base64 of the remaining PCM) or `null` when not running |
| `cancel` | `{ id }` | `true` |
| event `audio` | | `{ id, data }` (base64 PCM chunk) or `{ id, error }` |
