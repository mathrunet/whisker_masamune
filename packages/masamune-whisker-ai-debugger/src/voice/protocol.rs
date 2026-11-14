//! OpenAI Realtime transcription (`gpt-live-transcribe`) messages, the client-side turn detection
//! and noise gate, and the rules for joining, replacing and inserting text, shared with the Flutter
//! package (`AIDebugVoiceProtocol`) and SamuraiAI's web client (`realtimeSpeech.js`).
// Without the `voice` feature only the token and the insertion rule are used.
#![cfg_attr(not(feature = "voice"), allow(dead_code))]

use base64::Engine;
use serde_json::{Map, Value, json};

use crate::transport::AiDebugError;

/// Accessibility label of the voice input button, shared with the Flutter package.
pub const VOICE_INPUT_LABEL: &str = "AI debugger voice input";

/// Length of one audio chunk in milliseconds.
pub(crate) const CHUNK_MS: u32 = 100;

/// `event_id` of the `session.update` sent after connecting. Errors for it are ignored: the
/// session keeps the settings of the ephemeral key.
pub(crate) const SESSION_UPDATE_EVENT_ID: &str = "samurai_session_update";

/// Error codes that are not shown (an empty commit, for example).
pub(crate) const IGNORED_ERROR_CODES: [&str; 1] = ["input_audio_buffer_commit_empty"];

/// The Realtime API takes 24 kHz PCM16 only.
const DEFAULT_SAMPLE_RATE: u32 = 24_000;
const DEFAULT_COMMIT_SILENCE_MS: u32 = 700;
const COMMIT_SILENCE_MIN_MS: u32 = 200;
const COMMIT_SILENCE_MAX_MS: u32 = 5_000;
const DEFAULT_AUTO_STOP_SILENCE_SEC: f64 = 5.0;

/// Client-side settings sent with the token (`client` of the answer).
#[derive(Clone, Debug, PartialEq)]
pub struct AiDebugSpeechClientConfig {
    /// Silence after speech that ends a turn (200–5000 ms, default 700).
    pub commit_silence_ms: u32,
    /// Chunks quieter than this (dBFS) are sent as silence; `None` disables the gate.
    pub noise_gate_db: Option<f64>,
    /// Stops when the text has not changed for this many seconds (default 5, 0 disables).
    pub auto_stop_silence_sec: f64,
    /// Reading → spelling replacements (`from`, `to`) applied to every transcript.
    pub replacements: Vec<(String, String)>,
}

impl Default for AiDebugSpeechClientConfig {
    fn default() -> Self {
        Self {
            commit_silence_ms: DEFAULT_COMMIT_SILENCE_MS,
            noise_gate_db: None,
            auto_stop_silence_sec: DEFAULT_AUTO_STOP_SILENCE_SEC,
            replacements: Vec::new(),
        }
    }
}

impl AiDebugSpeechClientConfig {
    /// Reads the `client` object; missing or invalid values fall back to the defaults.
    pub fn from_json(json: &Map<String, Value>) -> Self {
        let number = |key: &str| {
            json.get(key)
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite())
        };
        let replacements = match json.get("replacements") {
            Some(Value::Array(entries)) => entries
                .iter()
                .filter_map(|entry| {
                    let from = entry.get("from").and_then(Value::as_str)?;
                    let to = entry.get("to").and_then(Value::as_str)?;
                    (!from.is_empty()).then(|| (from.to_owned(), to.to_owned()))
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            commit_silence_ms: number("commitSilenceMs").map_or(DEFAULT_COMMIT_SILENCE_MS, |ms| {
                (ms.round() as i64)
                    .clamp(COMMIT_SILENCE_MIN_MS as i64, COMMIT_SILENCE_MAX_MS as i64)
                    as u32
            }),
            noise_gate_db: number("noiseGateDb"),
            auto_stop_silence_sec: number("autoStopSilenceSec")
                .map_or(DEFAULT_AUTO_STOP_SILENCE_SEC, |sec| sec.max(0.0)),
            replacements,
        }
    }
}

/// Connection information returned by `POST /api/app-debug/runs/{runId}/speech-token`.
///
/// `protocols` carries the ephemeral key, so `Debug` hides it together with `url`.
#[derive(Clone, PartialEq)]
pub struct AiDebugSpeechToken {
    /// OpenAI Realtime WebSocket URL (`wss://api.openai.com/v1/realtime?intent=transcription`).
    pub url: String,
    /// WebSocket subprotocols, including `openai-insecure-api-key.<ephemeral key>`.
    pub protocols: Vec<String>,
    /// `session.update` message sent again after connecting.
    pub session_update: Option<Value>,
    pub model_id: Option<String>,
    /// Sample rate of the PCM16 audio to send (24000).
    pub sample_rate: u32,
    /// Client-side settings (turn detection, noise gate, auto stop, replacements).
    pub client: AiDebugSpeechClientConfig,
    /// Expiration time of the ephemeral key as sent by the server (ISO 8601).
    pub expires_at: Option<String>,
}

impl std::fmt::Debug for AiDebugSpeechToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiDebugSpeechToken")
            .field("url", &"[REDACTED]")
            .field("protocols", &"[REDACTED]")
            .field("session_update", &self.session_update.is_some())
            .field("model_id", &self.model_id)
            .field("sample_rate", &self.sample_rate)
            .field("client", &self.client)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl AiDebugSpeechToken {
    /// Reads the JSON object of a 2xx answer.
    pub fn from_json(json: &Map<String, Value>) -> Result<Self, AiDebugError> {
        let url = match json.get("url") {
            Some(Value::String(url)) if !url.is_empty() => url.clone(),
            _ => {
                return Err(AiDebugError::Transport(
                    "The speech token response has no url".into(),
                ));
            }
        };
        let sample_rate = json
            .get("sampleRate")
            .and_then(Value::as_f64)
            .filter(|rate| *rate > 0.0)
            .map_or(DEFAULT_SAMPLE_RATE, |rate| rate as u32);
        let protocols = match json.get("protocols") {
            Some(Value::Array(protocols)) => protocols
                .iter()
                .filter_map(Value::as_str)
                .filter(|protocol| !protocol.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        let session_update = json
            .get("sessionUpdate")
            .filter(|value| value.is_object())
            .cloned();
        let client = match json.get("client") {
            Some(Value::Object(client)) => AiDebugSpeechClientConfig::from_json(client),
            _ => AiDebugSpeechClientConfig::default(),
        };
        Ok(Self {
            url,
            protocols,
            session_update,
            model_id: json
                .get("modelId")
                .and_then(Value::as_str)
                .map(str::to_owned),
            sample_rate,
            client,
            expires_at: json
                .get("expiresAt")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
    }
}

pub(crate) fn chunk_bytes(sample_rate: u32) -> usize {
    (sample_rate as usize * 2 * CHUNK_MS as usize / 1000).max(2)
}

/// An `input_audio_buffer.append` message for PCM16 LE mono `pcm`.
pub(crate) fn append_message(pcm: &[u8]) -> String {
    json!({
        "type": "input_audio_buffer.append",
        "audio": base64::engine::general_purpose::STANDARD.encode(pcm),
    })
    .to_string()
}

/// The `input_audio_buffer.commit` message that ends a turn.
pub(crate) fn commit_message() -> String {
    json!({ "type": "input_audio_buffer.commit" }).to_string()
}

/// The token's `sessionUpdate` with [`SESSION_UPDATE_EVENT_ID`]; `None` unless it is an object.
pub(crate) fn session_update_message(session_update: &Value) -> Option<String> {
    let mut message = session_update.as_object()?.clone();
    message.insert("event_id".into(), SESSION_UPDATE_EVENT_ID.into());
    Some(Value::Object(message).to_string())
}

/// A message received from the transcription server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ServerMessage {
    /// More text of an unfinished turn.
    Delta {
        item_id: String,
        text: String,
    },
    /// The final text of a turn.
    Completed {
        item_id: String,
        text: String,
    },
    /// The turn could not be transcribed.
    Failed {
        item_id: String,
        code: String,
        text: String,
    },
    /// The server accepted a commit as the turn `item_id`.
    Committed {
        item_id: String,
    },
    Error {
        code: String,
        text: String,
        event_id: Option<String>,
    },
    Other,
}

pub(crate) fn parse_server_message(raw: &str) -> ServerMessage {
    let Ok(Value::Object(message)) = serde_json::from_str::<Value>(raw) else {
        return ServerMessage::Other;
    };
    let string = |value: Option<&Value>| {
        value
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    let item_id = || string(message.get("item_id")).unwrap_or_default();
    let error = message.get("error");
    let error_field = |key: &str| string(error.and_then(|error| error.get(key)));
    match message.get("type").and_then(Value::as_str) {
        Some("conversation.item.input_audio_transcription.delta") => ServerMessage::Delta {
            item_id: item_id(),
            text: string(message.get("delta")).unwrap_or_default(),
        },
        Some("conversation.item.input_audio_transcription.completed") => ServerMessage::Completed {
            item_id: item_id(),
            text: string(message.get("transcript")).unwrap_or_default(),
        },
        Some("conversation.item.input_audio_transcription.failed") => ServerMessage::Failed {
            item_id: item_id(),
            code: error_field("code").unwrap_or_else(|| "transcription_failed".into()),
            text: error_field("message").unwrap_or_else(|| "文字起こしに失敗しました".into()),
        },
        Some("input_audio_buffer.committed") => ServerMessage::Committed { item_id: item_id() },
        Some("error") => ServerMessage::Error {
            code: error_field("code")
                .or_else(|| error_field("type"))
                .unwrap_or_else(|| "error".into()),
            text: error_field("message").unwrap_or_else(|| "音声認識でエラーが起きました".into()),
            event_id: error_field("event_id"),
        },
        _ => ServerMessage::Other,
    }
}

/// Loudness (RMS, 0–1) of PCM16 LE mono bytes.
pub(crate) fn compute_rms(pcm: &[u8]) -> f64 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for sample in pcm.chunks_exact(2) {
        let value = i16::from_le_bytes([sample[0], sample[1]]) as f64 / 32768.0;
        sum += value * value;
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    (sum / count as f64).sqrt()
}

fn rms_to_db(rms: f64) -> f64 {
    if rms > 0.0 {
        20.0 * rms.log10()
    } else {
        f64::NEG_INFINITY
    }
}

pub(crate) fn db_to_rms(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The gate stays open this long after the level drops, so word endings are kept.
const NOISE_GATE_HOLD_MS: i64 = 300;

/// Minimum level to pass (noise gate). Chunks below the threshold are sent as silence of the same
/// length, so distant voices do not reach OpenAI; the audio time (and the billing) stays the same.
pub(crate) struct NoiseGate {
    threshold_db: Option<f64>,
    hold_left_ms: i64,
}

impl NoiseGate {
    /// `threshold_db` of `None` (or not finite) disables the gate.
    pub(crate) fn new(threshold_db: Option<f64>) -> Self {
        Self {
            threshold_db: threshold_db.filter(|db| db.is_finite()),
            hold_left_ms: 0,
        }
    }

    pub(crate) fn threshold_db(&self) -> Option<f64> {
        self.threshold_db
    }

    /// Whether the chunk with this level passes.
    pub(crate) fn push(&mut self, rms: f64) -> bool {
        let Some(threshold) = self.threshold_db else {
            return true;
        };
        if rms_to_db(rms) >= threshold {
            self.hold_left_ms = NOISE_GATE_HOLD_MS;
            return true;
        }
        if self.hold_left_ms <= 0 {
            return false;
        }
        self.hold_left_ms -= CHUNK_MS as i64;
        true
    }

    /// The chunk to send and its level: a closed gate yields zeros of the same length (level 0).
    pub(crate) fn apply(&mut self, pcm: &[u8]) -> (Vec<u8>, f64) {
        let rms = compute_rms(pcm);
        if self.push(rms) {
            (pcm.to_vec(), rms)
        } else {
            (vec![0; pcm.len()], 0.0)
        }
    }
}

/// Minimum level of speech; it must also be three times the noise floor.
const MIN_SPEECH_RMS: f64 = 0.02;
const NOISE_FLOOR_FACTOR: f64 = 3.0;
/// Speech shorter than this (a click, for example) does not end a turn.
const MIN_SPEECH_MS: u32 = 200;

/// Client-side turn detection (`gpt-live-transcribe` has no server VAD): after speech, a silence of
/// `commit_silence_ms` ends the turn.
pub(crate) struct TurnDetector {
    commit_silence_ms: u32,
    min_rms: f64,
    noise_floor: f64,
    speech_ms: u32,
    silence_ms: u32,
}

impl TurnDetector {
    /// `min_speech_rms` raises the speech threshold (the noise gate threshold when it is used).
    pub(crate) fn new(commit_silence_ms: u32, min_speech_rms: f64) -> Self {
        Self {
            commit_silence_ms,
            min_rms: MIN_SPEECH_RMS.max(min_speech_rms),
            noise_floor: 0.005,
            speech_ms: 0,
            silence_ms: 0,
        }
    }

    /// Feeds the level of one chunk; true when a commit should be sent now.
    pub(crate) fn push(&mut self, rms: f64) -> bool {
        let speaking = rms >= self.min_rms.max(self.noise_floor * NOISE_FLOOR_FACTOR);
        if speaking {
            self.speech_ms += CHUNK_MS;
            self.silence_ms = 0;
            return false;
        }
        // Follow the background level slowly, only while it is quiet.
        self.noise_floor = self.noise_floor * 0.95 + rms * 0.05;
        if self.speech_ms == 0 {
            return false;
        }
        self.silence_ms += CHUNK_MS;
        if self.silence_ms < self.commit_silence_ms {
            return false;
        }
        let enough = self.speech_ms >= MIN_SPEECH_MS;
        self.speech_ms = 0;
        self.silence_ms = 0;
        enough
    }

    /// Whether there was speech since the last commit (the final commit on stop).
    pub(crate) fn has_speech(&self) -> bool {
        self.speech_ms >= MIN_SPEECH_MS
    }

    pub(crate) fn reset(&mut self) {
        self.speech_ms = 0;
        self.silence_ms = 0;
    }
}

fn is_ascii_word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Joins transcripts, with a half-width space only between ASCII words.
pub(crate) fn join_transcripts<'a>(pieces: impl IntoIterator<Item = &'a str>) -> String {
    let mut result = String::new();
    for piece in pieces {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if is_ascii_word(result.chars().last()) && is_ascii_word(piece.chars().next()) {
            result.push(' ');
        }
        result.push_str(piece);
    }
    result
}

fn eq_ignore_case(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Applies reading → spelling replacements (`client.replacements`) to `text`.
///
/// Longer readings win; ASCII readings ignore case and never match inside an ASCII word. Where a
/// replacement makes ASCII words touch, a half-width space is added (`フラッターダート` becomes
/// `Flutter Dart`, `Flutterの` stays as is).
pub(crate) fn apply_replacements(text: &str, replacements: &[(String, String)]) -> String {
    if text.is_empty() || replacements.is_empty() {
        return text.to_owned();
    }
    let mut entries: Vec<(Vec<char>, &str)> = replacements
        .iter()
        .filter(|(from, _)| !from.is_empty())
        .map(|(from, to)| (from.chars().collect(), to.as_str()))
        .collect();
    if entries.is_empty() {
        return text.to_owned();
    }
    entries.sort_by_key(|(from, _)| std::cmp::Reverse(from.len()));
    let chars: Vec<char> = text.chars().collect();
    let find = |index: usize| {
        entries.iter().find(|(from, _)| {
            let end = index + from.len();
            end <= chars.len()
                && from
                    .iter()
                    .zip(&chars[index..end])
                    .all(|(a, b)| eq_ignore_case(*a, *b))
                && !(is_ascii_word(from.first().copied())
                    && is_ascii_word(index.checked_sub(1).map(|i| chars[i])))
                && !(is_ascii_word(from.last().copied()) && is_ascii_word(chars.get(end).copied()))
        })
    };
    let mut result = String::new();
    let mut last = 0;
    let mut index = 0;
    while index < chars.len() {
        let Some((from, to)) = find(index) else {
            index += 1;
            continue;
        };
        result.extend(&chars[last..index]);
        if is_ascii_word(result.chars().last()) && is_ascii_word(to.chars().next()) {
            result.push(' ');
        }
        result.push_str(to);
        index += from.len();
        last = index;
        if is_ascii_word(to.chars().last()) && is_ascii_word(chars.get(index).copied()) {
            result.push(' ');
        }
    }
    result.extend(&chars[last..]);
    result
}

/// Replaces the characters `[start, end)` of `value` with the trimmed `text` and returns the new
/// value and the cursor (in characters) after the insertion.
///
/// A half-width space is added only where ASCII letters or digits would touch (`use` + `Effect`
/// becomes `use Effect`, Japanese is joined as is). Missing or out-of-range positions fall back to
/// the end.
pub fn insert_transcript(
    value: &str,
    start: Option<usize>,
    end: Option<usize>,
    text: &str,
) -> (String, usize) {
    let chars: Vec<char> = value.chars().collect();
    let length = chars.len();
    let from = start.unwrap_or(length).min(length);
    let to = end.unwrap_or(from).clamp(from, length);
    let piece = text.trim();
    if piece.is_empty() {
        return (value.to_owned(), to);
    }
    let before: String = chars[..from].iter().collect();
    let after: String = chars[to..].iter().collect();
    let lead = if is_ascii_word(before.chars().last()) && is_ascii_word(piece.chars().next()) {
        " "
    } else {
        ""
    };
    let trail = if is_ascii_word(piece.chars().last()) && is_ascii_word(after.chars().next()) {
        " "
    } else {
        ""
    };
    let inserted = format!("{lead}{piece}{trail}");
    let cursor = from + inserted.chars().count();
    (format!("{before}{inserted}{after}"), cursor)
}

/// Message for a failed speech token request (the same texts as the Flutter package). Other
/// failures carry the server's message (a rejected key or exhausted credits answer 502).
pub(crate) fn token_error_message(error: &AiDebugError) -> String {
    match error {
        AiDebugError::Http { status: 412, .. } => {
            "音声入力を使うには Samurai の設定 → 音声入力 で OpenAI の API キーを登録してください"
                .into()
        }
        other => format!("音声入力を開始できませんでした: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    fn token_json() -> Value {
        json!({
            "success": true,
            "provider": "openai",
            "url": "wss://api.openai.com/v1/realtime?intent=transcription",
            "protocols": ["realtime", "openai-insecure-api-key.ek_secret", "", 3],
            "sessionUpdate": {
                "type": "session.update",
                "session": { "type": "transcription" },
            },
            "modelId": "gpt-live-transcribe",
            "sampleRate": 24000,
            "keytermCount": 2,
            "skippedKeyterms": [],
            "client": {
                "autoStopSilenceSec": 8,
                "sounds": true,
                "localPreview": true,
                "commitSilenceMs": 9000,
                "noiseGateDb": -45,
                "languageCode": "ja",
                "replacements": [
                    { "from": "フラッター", "to": "Flutter" },
                    { "from": "", "to": "x" },
                    { "from": "y" },
                ],
            },
            "expiresAt": "2026-11-12T10:00:00.000Z",
        })
    }

    #[test]
    fn parses_tokens() {
        let token = AiDebugSpeechToken::from_json(token_json().as_object().unwrap()).unwrap();
        assert_eq!(
            token.protocols,
            vec!["realtime", "openai-insecure-api-key.ek_secret"]
        );
        assert_eq!(
            token.session_update.as_ref().unwrap()["type"],
            "session.update"
        );
        assert_eq!(token.model_id.as_deref(), Some("gpt-live-transcribe"));
        assert_eq!(token.sample_rate, 24_000);
        assert_eq!(chunk_bytes(token.sample_rate), 4800);
        assert_eq!(
            token.client,
            AiDebugSpeechClientConfig {
                commit_silence_ms: 5000,
                noise_gate_db: Some(-45.0),
                auto_stop_silence_sec: 8.0,
                replacements: vec![("フラッター".into(), "Flutter".into())],
            }
        );
        let debug = format!("{token:?}");
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("api.openai.com"));

        let fallback =
            json!({ "url": "wss://x", "sampleRate": 0, "client": { "commitSilenceMs": 1 } });
        let token = AiDebugSpeechToken::from_json(fallback.as_object().unwrap()).unwrap();
        assert_eq!(token.sample_rate, 24_000);
        assert!(token.protocols.is_empty());
        assert!(token.session_update.is_none());
        assert_eq!(token.client.commit_silence_ms, 200);
        assert_eq!(token.client.auto_stop_silence_sec, 5.0);
        assert_eq!(token.client.noise_gate_db, None);
        let bare = json!({ "url": "wss://x" });
        let token = AiDebugSpeechToken::from_json(bare.as_object().unwrap()).unwrap();
        assert_eq!(token.client, AiDebugSpeechClientConfig::default());
        assert!(AiDebugSpeechToken::from_json(&Map::new()).is_err());
    }

    #[test]
    fn builds_messages() {
        let append: Value = serde_json::from_str(&append_message(&[1, 2, 3])).unwrap();
        assert_eq!(
            append,
            json!({ "type": "input_audio_buffer.append", "audio": "AQID" })
        );
        let commit: Value = serde_json::from_str(&commit_message()).unwrap();
        assert_eq!(commit, json!({ "type": "input_audio_buffer.commit" }));
        let update = json!({ "type": "session.update", "session": { "type": "transcription" } });
        let message: Value =
            serde_json::from_str(&session_update_message(&update).unwrap()).unwrap();
        assert_eq!(
            message,
            json!({
                "type": "session.update",
                "session": { "type": "transcription" },
                "event_id": "samurai_session_update",
            })
        );
        assert!(session_update_message(&json!("x")).is_none());
    }

    #[test]
    fn parses_server_messages() {
        assert_eq!(
            parse_server_message(
                r#"{"type":"conversation.item.input_audio_transcription.delta","item_id":"a","delta":"こん"}"#
            ),
            ServerMessage::Delta {
                item_id: "a".into(),
                text: "こん".into()
            }
        );
        assert_eq!(
            parse_server_message(
                r#"{"type":"conversation.item.input_audio_transcription.completed","item_id":"a","transcript":"こんにちは"}"#
            ),
            ServerMessage::Completed {
                item_id: "a".into(),
                text: "こんにちは".into()
            }
        );
        assert_eq!(
            parse_server_message(
                r#"{"type":"conversation.item.input_audio_transcription.failed","item_id":"b"}"#
            ),
            ServerMessage::Failed {
                item_id: "b".into(),
                code: "transcription_failed".into(),
                text: "文字起こしに失敗しました".into()
            }
        );
        assert_eq!(
            parse_server_message(
                r#"{"type":"input_audio_buffer.committed","item_id":"c","previous_item_id":"b"}"#
            ),
            ServerMessage::Committed {
                item_id: "c".into()
            }
        );
        assert_eq!(
            parse_server_message(
                r#"{"type":"error","error":{"type":"invalid_request_error","code":"bad","message":"Bad","event_id":"samurai_session_update"}}"#
            ),
            ServerMessage::Error {
                code: "bad".into(),
                text: "Bad".into(),
                event_id: Some("samurai_session_update".into())
            }
        );
        assert_eq!(
            parse_server_message(r#"{"type":"error","error":{"type":"invalid_request_error"}}"#),
            ServerMessage::Error {
                code: "invalid_request_error".into(),
                text: "音声認識でエラーが起きました".into(),
                event_id: None
            }
        );
        assert_eq!(parse_server_message("not json"), ServerMessage::Other);
        assert_eq!(
            parse_server_message(r#"{"type":"session.updated"}"#),
            ServerMessage::Other
        );
    }

    #[test]
    fn computes_levels() {
        assert_eq!(compute_rms(&[]), 0.0);
        assert_eq!(compute_rms(&pcm(&[0, 0])), 0.0);
        assert!((compute_rms(&pcm(&[16384, -16384])) - 0.5).abs() < 1e-9);
        assert!((db_to_rms(-20.0) - 0.1).abs() < 1e-9);
        assert!((rms_to_db(0.1) + 20.0).abs() < 1e-9);
        assert_eq!(rms_to_db(0.0), f64::NEG_INFINITY);
    }

    #[test]
    fn gates_quiet_chunks_after_a_hold() {
        let mut open = NoiseGate::new(None);
        assert!(open.push(0.0));
        let loud = pcm(&[16384; 4]);
        let quiet = pcm(&[10; 4]);
        let mut gate = NoiseGate::new(Some(-20.0));
        // Quiet before any speech: closed.
        assert_eq!(gate.apply(&quiet), (vec![0; 8], 0.0));
        assert_eq!(gate.apply(&loud).0, loud);
        // Held open for 300 ms (three chunks), then closed.
        for _ in 0..3 {
            assert_eq!(gate.apply(&quiet).0, quiet);
        }
        assert_eq!(gate.apply(&quiet), (vec![0; 8], 0.0));
        assert_eq!(NoiseGate::new(Some(f64::NAN)).threshold_db(), None);
    }

    #[test]
    fn detects_turns() {
        let mut turns = TurnDetector::new(700, 0.0);
        // Silence before speech never commits.
        for _ in 0..20 {
            assert!(!turns.push(0.0));
        }
        // 100 ms of speech is too short to commit, even after a long silence.
        assert!(!turns.push(0.2));
        assert!(!turns.has_speech());
        let commits = (0..7).filter(|_| turns.push(0.0)).count();
        assert_eq!(commits, 0);
        // 300 ms of speech then 700 ms of silence commits once.
        for _ in 0..3 {
            assert!(!turns.push(0.2));
        }
        assert!(turns.has_speech());
        let pushes: Vec<bool> = (0..8).map(|_| turns.push(0.0)).collect();
        assert_eq!(pushes.iter().filter(|commit| **commit).count(), 1);
        assert!(pushes[6]);
        assert!(!turns.has_speech());
        // reset forgets the speech.
        turns.push(0.2);
        turns.push(0.2);
        assert!(turns.has_speech());
        turns.reset();
        assert!(!turns.has_speech());
        // The gate threshold raises the speech level.
        let mut gated = TurnDetector::new(700, 0.1);
        gated.push(0.05);
        gated.push(0.05);
        assert!(!gated.has_speech());
    }

    #[test]
    fn noise_floor_follows_quiet_background() {
        let mut turns = TurnDetector::new(700, 0.0);
        // 0.03 is speech over a silent background ...
        turns.push(0.03);
        turns.push(0.03);
        assert!(turns.has_speech());
        // ... but not after the floor followed a steady 0.015 (threshold 0.045).
        let mut turns = TurnDetector::new(700, 0.0);
        for _ in 0..200 {
            turns.push(0.015);
        }
        turns.push(0.03);
        turns.push(0.03);
        assert!(!turns.has_speech());
        turns.push(0.05);
        turns.push(0.05);
        assert!(turns.has_speech());
    }

    #[test]
    fn joins_transcripts() {
        assert_eq!(
            join_transcripts(["use", " Effect ", "", "の話"]),
            "use Effectの話"
        );
        assert_eq!(
            join_transcripts(["こんにちは", "Flutter"]),
            "こんにちはFlutter"
        );
        assert_eq!(join_transcripts(Vec::<&str>::new()), "");
    }

    #[test]
    fn applies_replacements_like_the_web_client() {
        let table = vec![
            ("フラッター".to_owned(), "Flutter".to_owned()),
            ("フラッターフロー".to_owned(), "FlutterFlow".to_owned()),
            ("ダート".to_owned(), "Dart".to_owned()),
            ("flutter".to_owned(), "Flutter".to_owned()),
        ];
        // Longer readings first.
        assert_eq!(
            apply_replacements("フラッターフローとフラッター", &table),
            "FlutterFlowとFlutter"
        );
        // ASCII words that touch get a space; Japanese stays joined.
        assert_eq!(
            apply_replacements("フラッターダート", &table),
            "Flutter Dart"
        );
        assert_eq!(
            apply_replacements("フラッターの画面", &table),
            "Flutterの画面"
        );
        assert_eq!(apply_replacements("useフラッター", &table), "use Flutter");
        // ASCII readings ignore case but never match inside a word.
        assert_eq!(apply_replacements("FLUTTER app", &table), "Flutter app");
        assert_eq!(apply_replacements("fluttering", &table), "fluttering");
        assert_eq!(apply_replacements("", &table), "");
        assert_eq!(apply_replacements("フラッター", &[]), "フラッター");
    }

    #[test]
    fn inserts_transcripts_like_the_web_client() {
        // Appends at the end without a cursor.
        assert_eq!(
            insert_transcript("", None, None, " こんにちは "),
            ("こんにちは".into(), 5)
        );
        // Japanese next to ASCII is joined as is.
        assert_eq!(
            insert_transcript("Flutter", None, None, "の画面"),
            ("Flutterの画面".into(), 10)
        );
        // ASCII words are separated by a space on both sides.
        assert_eq!(
            insert_transcript("use", None, None, "Effect"),
            ("use Effect".into(), 10)
        );
        assert_eq!(
            insert_transcript("ab", Some(1), Some(1), "X"),
            ("a X b".into(), 4)
        );
        // Replaces the selection, counting characters (not bytes).
        assert_eq!(
            insert_transcript("あいうえお", Some(1), Some(3), "か"),
            ("あかえお".into(), 2)
        );
        // Out-of-range positions fall back to the end; empty text keeps the value.
        assert_eq!(
            insert_transcript("abc", Some(99), None, "。"),
            ("abc。".into(), 4)
        );
        assert_eq!(
            insert_transcript("abc", Some(1), Some(2), "  "),
            ("abc".into(), 2)
        );
    }

    #[test]
    fn explains_token_errors() {
        let missing = AiDebugError::Http {
            status: 412,
            message: "OpenAI の API キーが未設定です".into(),
        };
        assert!(token_error_message(&missing).contains("OpenAI の API キー"));
        let quota = AiDebugError::Http {
            status: 502,
            message: "OpenAI の API クレジットを使い切りました".into(),
        };
        assert!(token_error_message(&quota).contains("クレジットを使い切りました"));
        assert!(
            token_error_message(&AiDebugError::Transport("offline".into())).contains("offline")
        );
    }
}
