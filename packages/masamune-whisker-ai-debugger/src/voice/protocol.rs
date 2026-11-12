//! ElevenLabs Scribe v3 Live messages and the rule for inserting committed text, shared with the
//! Flutter package (`AIDebugVoiceProtocol`) and SamuraiAI's web client (`realtimeSpeech.js`).
// Without the `voice` feature only the token and the insertion rule are used.
#![cfg_attr(not(feature = "voice"), allow(dead_code))]

use base64::Engine;
use serde_json::{Map, Value, json};

use crate::transport::AiDebugError;

/// Accessibility label of the voice input button, shared with the Flutter package.
pub const VOICE_INPUT_LABEL: &str = "AI debugger voice input";

/// Length of one audio chunk in milliseconds.
pub(crate) const CHUNK_MS: u32 = 100;

/// Server message types treated as errors.
const ERROR_TYPES: [&str; 14] = [
    "error",
    "auth_error",
    "quota_exceeded",
    "commit_throttled",
    "unaccepted_terms",
    "rate_limited",
    "queue_overflow",
    "resource_exhausted",
    "session_time_limit_exceeded",
    "input_error",
    "invalid_request",
    "chunk_size_exceeded",
    "insufficient_audio_activity",
    "transcriber_error",
];

/// Connection information returned by `POST /api/app-debug/runs/{runId}/speech-token`.
///
/// `url` carries a single-use token, so `Debug` hides it.
#[derive(Clone, PartialEq, Eq)]
pub struct AiDebugSpeechToken {
    /// ElevenLabs Scribe WebSocket URL including the single-use token.
    pub url: String,
    pub model_id: Option<String>,
    /// Sample rate of the PCM16 audio to send (16000).
    pub sample_rate: u32,
    /// Key terms attached to the first audio chunk only.
    pub keyterm_library: Vec<String>,
    /// Expiration time as sent by the server (ISO 8601).
    pub expires_at: Option<String>,
}

impl std::fmt::Debug for AiDebugSpeechToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiDebugSpeechToken")
            .field("url", &"[REDACTED]")
            .field("model_id", &self.model_id)
            .field("sample_rate", &self.sample_rate)
            .field("keyterm_library", &self.keyterm_library.len())
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
            .map_or(16_000, |rate| rate as u32);
        let keyterm_library = match json.get("keytermLibrary") {
            Some(Value::Array(terms)) => terms
                .iter()
                .filter_map(Value::as_str)
                .filter(|term| !term.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        Ok(Self {
            url,
            model_id: json
                .get("modelId")
                .and_then(Value::as_str)
                .map(str::to_owned),
            sample_rate,
            keyterm_library,
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

/// An `input_audio_chunk` message for PCM16 LE mono `pcm`. `keyterm_library` is added only when
/// it is not empty; pass it for the first chunk only.
pub(crate) fn audio_chunk_message(
    pcm: &[u8],
    sample_rate: u32,
    commit: bool,
    keyterm_library: Option<&[String]>,
) -> String {
    let mut message = json!({
        "message_type": "input_audio_chunk",
        "audio_base_64": base64::engine::general_purpose::STANDARD.encode(pcm),
        "commit": commit,
        "sample_rate": sample_rate,
    });
    if let Some(terms) = keyterm_library.filter(|terms| !terms.is_empty()) {
        message["keyterm_library"] = json!(terms);
    }
    message.to_string()
}

/// A message received from the transcription server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ServerMessage {
    Started,
    Partial(String),
    Committed(String),
    Error { code: String, text: String },
    Other,
}

pub(crate) fn parse_server_message(raw: &str) -> ServerMessage {
    let Ok(Value::Object(message)) = serde_json::from_str::<Value>(raw) else {
        return ServerMessage::Other;
    };
    let text = || {
        message
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match message.get("message_type").and_then(Value::as_str) {
        Some("session_started") => ServerMessage::Started,
        Some("partial_transcript") => ServerMessage::Partial(text()),
        Some("committed_transcript" | "committed_transcript_with_timestamps") => {
            ServerMessage::Committed(text())
        }
        Some(kind) if ERROR_TYPES.contains(&kind) => {
            let detail = message
                .get("error")
                .or_else(|| message.get("message"))
                .map(|value| match value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                });
            ServerMessage::Error {
                code: kind.to_owned(),
                text: detail.unwrap_or_else(|| kind.to_owned()),
            }
        }
        _ => ServerMessage::Other,
    }
}

fn is_ascii_word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
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

/// Message for a failed speech token request (the same texts as the Flutter package).
pub(crate) fn token_error_message(error: &AiDebugError) -> String {
    match error {
        AiDebugError::Http { status: 412, .. } => {
            "音声入力を使うには Samurai の設定 → ElevenLabs で API キーを登録してください".into()
        }
        other => format!("音声入力を開始できませんでした: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tokens() {
        let json = json!({
            "success": true,
            "url": "wss://api.elevenlabs.io/v1/speech-to-text/realtime?token=secret",
            "modelId": "scribe_v3_live",
            "sampleRate": 16000,
            "keytermCount": 2,
            "keytermLibrary": ["Whisker", "", 3, "Riverpod"],
            "skippedKeyterms": [],
            "expiresAt": "2026-11-12T10:00:00.000Z",
        });
        let token = AiDebugSpeechToken::from_json(json.as_object().unwrap()).unwrap();
        assert_eq!(token.model_id.as_deref(), Some("scribe_v3_live"));
        assert_eq!(token.sample_rate, 16_000);
        assert_eq!(token.keyterm_library, vec!["Whisker", "Riverpod"]);
        assert_eq!(chunk_bytes(token.sample_rate), 3200);
        assert!(!format!("{token:?}").contains("secret"));

        let fallback = json!({ "url": "wss://x", "sampleRate": 0 });
        let token = AiDebugSpeechToken::from_json(fallback.as_object().unwrap()).unwrap();
        assert_eq!(token.sample_rate, 16_000);
        assert!(token.keyterm_library.is_empty());
        assert!(AiDebugSpeechToken::from_json(&Map::new()).is_err());
    }

    #[test]
    fn builds_audio_chunks() {
        let terms = vec!["Whisker".to_owned()];
        let first: Value = serde_json::from_str(&audio_chunk_message(
            &[1, 2, 3],
            16_000,
            false,
            Some(&terms),
        ))
        .unwrap();
        assert_eq!(
            first,
            json!({
                "message_type": "input_audio_chunk",
                "audio_base_64": "AQID",
                "commit": false,
                "sample_rate": 16000,
                "keyterm_library": ["Whisker"],
            })
        );
        let empty: Value =
            serde_json::from_str(&audio_chunk_message(&[0, 0], 16_000, true, Some(&[]))).unwrap();
        assert!(empty.get("keyterm_library").is_none());
        assert_eq!(empty["commit"], true);
        let later: Value =
            serde_json::from_str(&audio_chunk_message(&[0, 0], 16_000, false, None)).unwrap();
        assert!(later.get("keyterm_library").is_none());
    }

    #[test]
    fn parses_server_messages() {
        assert_eq!(
            parse_server_message(r#"{"message_type":"session_started"}"#),
            ServerMessage::Started
        );
        assert_eq!(
            parse_server_message(r#"{"message_type":"partial_transcript","text":"こん"}"#),
            ServerMessage::Partial("こん".into())
        );
        assert_eq!(
            parse_server_message(r#"{"message_type":"committed_transcript","text":"こんにちは"}"#),
            ServerMessage::Committed("こんにちは".into())
        );
        assert_eq!(
            parse_server_message(
                r#"{"message_type":"committed_transcript_with_timestamps","text":"hi","words":[]}"#
            ),
            ServerMessage::Committed("hi".into())
        );
        assert_eq!(
            parse_server_message(r#"{"message_type":"quota_exceeded","error":"Quota exceeded"}"#),
            ServerMessage::Error {
                code: "quota_exceeded".into(),
                text: "Quota exceeded".into()
            }
        );
        assert_eq!(
            parse_server_message(r#"{"message_type":"auth_error"}"#),
            ServerMessage::Error {
                code: "auth_error".into(),
                text: "auth_error".into()
            }
        );
        assert_eq!(parse_server_message("not json"), ServerMessage::Other);
        assert_eq!(
            parse_server_message(r#"{"message_type":"unknown"}"#),
            ServerMessage::Other
        );
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
    fn explains_missing_api_keys() {
        let missing = AiDebugError::Http {
            status: 412,
            message: "ElevenLabs API key is not configured".into(),
        };
        assert!(token_error_message(&missing).contains("ElevenLabs"));
        assert!(
            token_error_message(&AiDebugError::Transport("offline".into())).contains("offline")
        );
    }
}
