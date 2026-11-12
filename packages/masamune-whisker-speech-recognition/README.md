# masamune-whisker-speech-recognition

Streaming speech recognition with partial results for Whisker apps. It prefers the on-device
recognizer, so it starts quickly and works without a network. Use it as an instant preview while a
cloud recognizer is still connecting, or as a lightweight recognizer on its own.

| Platform | Implementation |
|---|---|
| Android | `SpeechRecognizer` (`createOnDeviceSpeechRecognizer` on Android 12+ when available, otherwise the default service with `EXTRA_PREFER_OFFLINE`) |
| iOS | Stub: reports the recognizer as unavailable |

A session listens in one of two ways:

- **Microphone** (default): the recognizer service records the microphone. It listens again after
  every pause, so a session keeps going until it is stopped.
- **External audio** (Android 13+): the app passes PCM16 little-endian mono audio with
  `feed` (`RecognizerIntent.EXTRA_AUDIO_SOURCE`, one segmented session). Use it when the app already
  records the microphone, for example a WebView with `getUserMedia`, so that a second capture does
  not compete for the input.

This is a function-only native module without a CNG plugin. The app declares
`android.permission.RECORD_AUDIO` itself. The microphone mode does not request the runtime
permission; it fails with `permission_denied` when the permission is missing.

When the on-device model lacks the language, the session falls back to the default recognizer
once, which may send audio to the recognizer's servers.

## Usage

```rust,ignore
use masamune_whisker_speech_recognition::{
    SpeechRecognition, SpeechRecognitionEvent, SpeechRecognitionOptions, availability,
};

if !availability().available {
    return;
}
spawn_local(async move {
    let options = SpeechRecognitionOptions {
        language: Some("ja-JP".into()),
        ..Default::default()
    };
    let session = SpeechRecognition::start(options, |event| match event {
        // The whole text so far: replace the previous text with it.
        SpeechRecognitionEvent::Text(text) => show_preview(text),
        SpeechRecognitionEvent::Error(code) => log::debug!("speech stopped: {code}"),
    })
    .await;
    // Keep `session` while listening; dropping it stops the recognizer.
});
```

With `external_audio: true`, call `session.feed(&pcm)` (or `feed_base64`) for every chunk of
16 kHz audio (`sample_rate` sets another rate).

## Events

- `Text(text)`: the finished segments followed by the current partial result. Each event carries
  the whole text, so replace rather than append. Japanese, Chinese and Korean segments are joined
  without spaces.
- `Error(code)`: recognition stopped. Codes: `audio`, `client`, `permission_denied`, `network`,
  `no_match`, `busy`, `server`, `speech_timeout`, `error_<n>`. In the microphone mode `no_match`
  and `speech_timeout` restart listening instead.
