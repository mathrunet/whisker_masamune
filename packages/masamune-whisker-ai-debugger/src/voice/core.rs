//! Platform-independent state of one voice input: chunking, the first-chunk key terms, the final
//! commit and the timeouts. The session feeds it events and carries out what it returns.
// Without the `voice` feature it is compiled for the unit tests only.
#![cfg_attr(not(feature = "voice"), allow(dead_code))]

use super::protocol::{ServerMessage, audio_chunk_message, chunk_bytes, parse_server_message};

/// How long to wait for the final committed transcript after stopping.
pub(crate) const STOP_TIMEOUT_MS: i64 = 3_000;
/// The session ends when no audio was sent for this long.
pub(crate) const IDLE_TIMEOUT_MS: i64 = 15_000;
/// Audio kept while connecting (in chunks, 10 seconds).
const BUFFER_LIMIT_CHUNKS: usize = 100;

/// What the session has to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    /// Send a text frame.
    Send(String),
    /// Show the interim transcript (empty clears it).
    Partial(String),
    /// Insert a committed transcript.
    Committed(String),
    /// Show an error.
    Error(String),
    /// Close everything.
    Finish,
}

pub(crate) struct VoiceCore {
    sample_rate: u32,
    keyterms: Vec<String>,
    first: bool,
    pending: Vec<u8>,
    connected: bool,
    stopped_at: Option<i64>,
    last_sent: i64,
    done: bool,
}

impl VoiceCore {
    pub(crate) fn new(sample_rate: u32, keyterms: Vec<String>) -> Self {
        Self {
            sample_rate,
            keyterms,
            first: true,
            pending: Vec::new(),
            connected: false,
            stopped_at: None,
            last_sent: 0,
            done: false,
        }
    }

    fn chunk_bytes(&self) -> usize {
        chunk_bytes(self.sample_rate)
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stopped_at.is_some()
    }

    /// The socket opened: sends the audio buffered while connecting.
    pub(crate) fn connected(&mut self, now: i64) -> Vec<Output> {
        if self.done {
            return Vec::new();
        }
        self.connected = true;
        self.last_sent = now;
        self.flush(now)
    }

    /// Audio from the microphone.
    pub(crate) fn audio(&mut self, pcm: &[u8], now: i64) -> Vec<Output> {
        if self.done || self.stopped_at.is_some() {
            return Vec::new();
        }
        self.pending.extend_from_slice(pcm);
        if !self.connected {
            let limit = self.chunk_bytes() * BUFFER_LIMIT_CHUNKS;
            if self.pending.len() > limit {
                let excess = self.pending.len() - limit;
                self.pending.drain(..excess);
            }
            return Vec::new();
        }
        self.flush(now)
    }

    fn flush(&mut self, now: i64) -> Vec<Output> {
        let size = self.chunk_bytes();
        let mut out = Vec::new();
        while self.pending.len() >= size {
            let chunk: Vec<u8> = self.pending.drain(..size).collect();
            out.push(Output::Send(self.message(&chunk, false)));
        }
        if !out.is_empty() {
            self.last_sent = now;
        }
        out
    }

    fn message(&mut self, pcm: &[u8], commit: bool) -> String {
        let terms = self.first.then_some(self.keyterms.as_slice());
        self.first = false;
        audio_chunk_message(pcm, self.sample_rate, commit, terms)
    }

    /// The user stopped: sends the rest of the audio (`rest` is what the microphone still held)
    /// and commits it with the last chunk (100 ms of silence when nothing is left).
    pub(crate) fn stop(&mut self, rest: &[u8], now: i64) -> Vec<Output> {
        if self.done || self.stopped_at.is_some() {
            return Vec::new();
        }
        if !self.connected {
            return self.finish(Vec::new());
        }
        self.stopped_at = Some(now);
        self.pending.extend_from_slice(rest);
        let size = self.chunk_bytes();
        let mut out = Vec::new();
        while self.pending.len() > size {
            let chunk: Vec<u8> = self.pending.drain(..size).collect();
            out.push(Output::Send(self.message(&chunk, false)));
        }
        let last = if self.pending.is_empty() {
            vec![0; size]
        } else {
            std::mem::take(&mut self.pending)
        };
        out.push(Output::Send(self.message(&last, true)));
        self.last_sent = now;
        out
    }

    /// A text frame from the server.
    pub(crate) fn server(&mut self, raw: &str) -> Vec<Output> {
        if self.done {
            return Vec::new();
        }
        match parse_server_message(raw) {
            ServerMessage::Partial(text) => vec![Output::Partial(text)],
            ServerMessage::Committed(text) => {
                let mut out = vec![Output::Partial(String::new())];
                if !text.trim().is_empty() {
                    out.push(Output::Committed(text));
                }
                if self.stopped_at.is_some() {
                    return self.finish(out);
                }
                out
            }
            // Stopping without speech is not an error.
            ServerMessage::Error { code, .. }
                if code == "insufficient_audio_activity" && self.stopped_at.is_some() =>
            {
                self.finish(Vec::new())
            }
            ServerMessage::Error { code, text } => self.finish(vec![Output::Error(format!(
                "音声入力でエラーが起きました: {text}（{code}）"
            ))]),
            ServerMessage::Started | ServerMessage::Other => Vec::new(),
        }
    }

    /// The socket closed or failed.
    pub(crate) fn closed(&mut self, reason: Option<String>) -> Vec<Output> {
        if self.done {
            return Vec::new();
        }
        if self.stopped_at.is_some() {
            return self.finish(Vec::new());
        }
        let message = match reason.filter(|reason| !reason.is_empty()) {
            Some(reason) => format!("音声認識サーバーとの接続が切れました: {reason}"),
            None => "音声認識サーバーとの接続が切れました".to_owned(),
        };
        self.finish(vec![Output::Error(message)])
    }

    /// The microphone stopped on its own.
    pub(crate) fn audio_failed(&mut self, message: &str) -> Vec<Output> {
        if self.done {
            return Vec::new();
        }
        self.finish(vec![Output::Error(format!(
            "音声入力でエラーが起きました: {message}"
        ))])
    }

    /// Periodic check of the stop and idle timeouts.
    pub(crate) fn tick(&mut self, now: i64) -> Vec<Output> {
        if self.done {
            return Vec::new();
        }
        if let Some(stopped_at) = self.stopped_at {
            if now - stopped_at >= STOP_TIMEOUT_MS {
                return self.finish(Vec::new());
            }
            return Vec::new();
        }
        if self.connected && now - self.last_sent >= IDLE_TIMEOUT_MS {
            return self.finish(vec![Output::Error(
                "音声が15秒間届かなかったため音声入力を停止しました".into(),
            )]);
        }
        Vec::new()
    }

    fn finish(&mut self, mut out: Vec<Output>) -> Vec<Output> {
        self.done = true;
        out.push(Output::Finish);
        out
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn sent(outputs: &[Output]) -> Vec<Value> {
        outputs
            .iter()
            .filter_map(|output| match output {
                Output::Send(text) => Some(serde_json::from_str(text).unwrap()),
                _ => None,
            })
            .collect()
    }

    fn pcm_len(message: &Value) -> usize {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(message["audio_base_64"].as_str().unwrap())
            .unwrap()
            .len()
    }

    #[test]
    fn buffers_until_connected_and_sends_key_terms_once() {
        let mut core = VoiceCore::new(16_000, vec!["Whisker".into()]);
        assert!(core.audio(&[1; 5000], 0).is_empty());
        let messages = sent(&core.connected(10));
        assert_eq!(messages.len(), 1);
        assert_eq!(pcm_len(&messages[0]), 3200);
        assert_eq!(messages[0]["keyterm_library"][0], "Whisker");
        assert_eq!(messages[0]["commit"], false);
        let messages = sent(&core.audio(&[1; 1400], 20));
        assert_eq!(messages.len(), 1);
        assert!(messages[0].get("keyterm_library").is_none());
    }

    #[test]
    fn keeps_ten_seconds_while_connecting() {
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.audio(&vec![0; 3200 * 150], 0);
        assert_eq!(sent(&core.connected(0)).len(), 100);
    }

    #[test]
    fn commits_the_rest_and_waits_for_the_transcript() {
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        core.audio(&[1; 1000], 0);
        let messages = sent(&core.stop(&[2; 3000], 100));
        // 4000 bytes: one full chunk, then the rest with commit.
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["commit"], false);
        assert_eq!(messages[1]["commit"], true);
        assert_eq!(pcm_len(&messages[1]), 800);
        assert!(core.audio(&[1; 3200], 120).is_empty());
        assert_eq!(
            core.server(r#"{"message_type":"partial_transcript","text":"こん"}"#),
            vec![Output::Partial("こん".into())]
        );
        assert_eq!(
            core.server(r#"{"message_type":"committed_transcript","text":"こんにちは"}"#),
            vec![
                Output::Partial(String::new()),
                Output::Committed("こんにちは".into()),
                Output::Finish
            ]
        );
        assert!(core.tick(10_000).is_empty());
    }

    #[test]
    fn commits_silence_when_nothing_is_left() {
        let mut core = VoiceCore::new(16_000, vec!["Term".into()]);
        core.connected(0);
        let messages = sent(&core.stop(&[], 0));
        assert_eq!(messages.len(), 1);
        assert_eq!(pcm_len(&messages[0]), 3200);
        assert_eq!(messages[0]["commit"], true);
        // The first chunk still carries the key terms.
        assert_eq!(messages[0]["keyterm_library"][0], "Term");
        assert!(core.tick(STOP_TIMEOUT_MS - 1).is_empty());
        assert_eq!(core.tick(STOP_TIMEOUT_MS), vec![Output::Finish]);
    }

    #[test]
    fn committed_text_while_recording_keeps_going() {
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        assert_eq!(
            core.server(r#"{"message_type":"committed_transcript","text":"one"}"#),
            vec![
                Output::Partial(String::new()),
                Output::Committed("one".into())
            ]
        );
        assert_eq!(
            core.server(r#"{"message_type":"committed_transcript","text":" "}"#),
            vec![Output::Partial(String::new())]
        );
    }

    #[test]
    fn errors() {
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        let out = core.server(r#"{"message_type":"quota_exceeded","error":"Quota"}"#);
        assert!(matches!(&out[0], Output::Error(text) if text.contains("Quota")));
        assert_eq!(out.last(), Some(&Output::Finish));
        assert!(
            core.server(r#"{"message_type":"partial_transcript","text":"x"}"#)
                .is_empty()
        );

        // No speech before stopping is not an error.
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        core.stop(&[], 0);
        assert_eq!(
            core.server(r#"{"message_type":"insufficient_audio_activity"}"#),
            vec![Output::Finish]
        );

        // ... but it is while recording.
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        assert!(matches!(
            &core.server(r#"{"message_type":"insufficient_audio_activity"}"#)[0],
            Output::Error(_)
        ));

        // Closing while recording is an error, after stopping it is not.
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        assert!(matches!(&core.closed(None)[0], Output::Error(_)));
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        core.stop(&[], 0);
        assert_eq!(core.closed(None), vec![Output::Finish]);
    }

    #[test]
    fn stops_after_fifteen_idle_seconds() {
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.connected(0);
        core.audio(&[0; 3200], 1_000);
        assert!(core.tick(1_000 + IDLE_TIMEOUT_MS - 1).is_empty());
        let out = core.tick(1_000 + IDLE_TIMEOUT_MS);
        assert!(matches!(&out[0], Output::Error(_)));
        assert_eq!(out.last(), Some(&Output::Finish));
    }

    #[test]
    fn stopping_before_connecting_just_finishes() {
        let mut core = VoiceCore::new(16_000, Vec::new());
        core.audio(&[0; 6400], 0);
        assert_eq!(core.stop(&[], 0), vec![Output::Finish]);
        assert!(core.connected(0).is_empty());
    }
}
