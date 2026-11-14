//! Platform-independent state of one voice input: chunking, the noise gate, client-side turn
//! detection, the transcript order, the final commit and the timeouts. The session feeds it events
//! and carries out what it returns.
// Without the `voice` feature it is compiled for the unit tests only.
#![cfg_attr(not(feature = "voice"), allow(dead_code))]

use std::collections::{HashSet, VecDeque};

use serde_json::Value;

use super::protocol::{
    AiDebugSpeechClientConfig, IGNORED_ERROR_CODES, NoiseGate, SESSION_UPDATE_EVENT_ID,
    ServerMessage, TurnDetector, append_message, apply_replacements, chunk_bytes, commit_message,
    db_to_rms, join_transcripts, parse_server_message, session_update_message,
};

/// How long to wait for the outstanding transcripts after the final commit.
pub(crate) const STOP_TIMEOUT_MS: i64 = 3_000;
/// How long a stop before connecting waits for the connection (and then the transcripts).
pub(crate) const CONNECT_TIMEOUT_MS: i64 = 10_000;
/// The session ends when no audio was sent for this long.
pub(crate) const IDLE_TIMEOUT_MS: i64 = 15_000;
/// Messages kept while connecting (100 ms chunks, 15 seconds).
const BUFFER_LIMIT_MESSAGES: usize = 150;

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
    /// The text did not change for `auto_stop_silence_sec`: stop as if the user did.
    AutoStop,
    /// Close everything.
    Finish,
}

/// Text produced by the [`TranscriptAssembler`].
#[derive(Clone, Debug, PartialEq, Eq)]
enum Transcript {
    Partial(String),
    Committed(String),
}

struct Item {
    id: String,
    text: String,
    done: bool,
}

/// Turns (`item_id`) in the order they appeared. Completions may arrive out of order, so only the
/// leading completed turns are committed; the rest is joined into the interim transcript.
#[derive(Default)]
struct TranscriptAssembler {
    items: Vec<Item>,
    /// Turns already committed (late events do not bring them back).
    finished: HashSet<String>,
    last_partial: String,
}

impl TranscriptAssembler {
    fn find(&mut self, item_id: &str) -> &mut Item {
        let index = match self.items.iter().position(|item| item.id == item_id) {
            Some(index) => index,
            None => {
                self.items.push(Item {
                    id: item_id.to_owned(),
                    text: String::new(),
                    done: false,
                });
                self.items.len() - 1
            }
        };
        &mut self.items[index]
    }

    fn emit(&mut self) -> Vec<Transcript> {
        let mut out = Vec::new();
        while self.items.first().is_some_and(|item| item.done) {
            let item = self.items.remove(0);
            self.finished.insert(item.id);
            self.last_partial.clear();
            out.push(Transcript::Committed(item.text.trim().to_owned()));
        }
        let partial = join_transcripts(self.items.iter().map(|item| item.text.as_str()));
        if partial != self.last_partial {
            self.last_partial = partial.clone();
            out.push(Transcript::Partial(partial));
        }
        out
    }

    fn committed(&mut self, item_id: &str) {
        if !item_id.is_empty() && !self.finished.contains(item_id) {
            self.find(item_id);
        }
    }

    fn delta(&mut self, item_id: &str, text: &str) -> Vec<Transcript> {
        if self.finished.contains(item_id) {
            return Vec::new();
        }
        let item = self.find(item_id);
        if item.done {
            return Vec::new();
        }
        item.text.push_str(text);
        self.emit()
    }

    fn completed(&mut self, item_id: &str, text: &str) -> Vec<Transcript> {
        if self.finished.contains(item_id) {
            return Vec::new();
        }
        let item = self.find(item_id);
        item.text = text.to_owned();
        item.done = true;
        self.emit()
    }

    /// Turns not committed yet.
    fn pending_count(&self) -> usize {
        self.items.len()
    }
}

pub(crate) struct VoiceCore {
    sample_rate: u32,
    session_update: Option<String>,
    client: AiDebugSpeechClientConfig,
    gate: NoiseGate,
    turns: TurnDetector,
    transcripts: TranscriptAssembler,
    /// Audio not yet cut into a chunk.
    pending: Vec<u8>,
    /// Messages kept while connecting (`true` for commits).
    queued: VecDeque<(String, bool)>,
    connected: bool,
    /// Commits sent (or queued) whose `input_audio_buffer.committed` has not arrived.
    awaiting_commits: usize,
    stopped_at: Option<i64>,
    /// The session ends at this time while stopping.
    stop_deadline: Option<i64>,
    last_sent: i64,
    /// The text last shown and when it changed, for the auto stop.
    last_text: String,
    last_text_at: Option<i64>,
    auto_stopping: bool,
    done: bool,
}

impl VoiceCore {
    pub(crate) fn new(
        sample_rate: u32,
        session_update: Option<Value>,
        client: AiDebugSpeechClientConfig,
    ) -> Self {
        let gate = NoiseGate::new(client.noise_gate_db);
        let turns = TurnDetector::new(
            client.commit_silence_ms,
            gate.threshold_db().map_or(0.0, db_to_rms),
        );
        Self {
            sample_rate,
            session_update: session_update.as_ref().and_then(session_update_message),
            client,
            gate,
            turns,
            transcripts: TranscriptAssembler::default(),
            pending: Vec::new(),
            queued: VecDeque::new(),
            connected: false,
            awaiting_commits: 0,
            stopped_at: None,
            stop_deadline: None,
            last_sent: 0,
            last_text: String::new(),
            last_text_at: None,
            auto_stopping: false,
            done: false,
        }
    }

    fn chunk_bytes(&self) -> usize {
        chunk_bytes(self.sample_rate)
    }

    pub(crate) fn is_stopping(&self) -> bool {
        self.stopped_at.is_some()
    }

    /// The socket opened: sends the session update first, then what was kept while connecting.
    pub(crate) fn connected(&mut self, now: i64) -> Vec<Output> {
        if self.done || self.connected {
            return Vec::new();
        }
        self.connected = true;
        self.last_sent = now;
        self.last_text_at = Some(now);
        let mut out: Vec<Output> = self
            .session_update
            .take()
            .map(Output::Send)
            .into_iter()
            .collect();
        out.extend(self.queued.drain(..).map(|(text, _)| Output::Send(text)));
        if self.stopped_at.is_some() {
            // The final commit went out with the queue.
            let deadline = now + STOP_TIMEOUT_MS;
            self.stop_deadline = Some(self.stop_deadline.map_or(deadline, |d| d.min(deadline)));
            return self.finish_if_settled(out);
        }
        out
    }

    /// Audio from the microphone.
    pub(crate) fn audio(&mut self, pcm: &[u8], now: i64) -> Vec<Output> {
        if self.done || self.stopped_at.is_some() {
            return Vec::new();
        }
        self.pending.extend_from_slice(pcm);
        let size = self.chunk_bytes();
        let mut out = Vec::new();
        while self.pending.len() >= size {
            let chunk: Vec<u8> = self.pending.drain(..size).collect();
            self.chunk(&chunk, now, &mut out);
        }
        out
    }

    /// Gates one chunk, sends it and commits when the turn detector says so.
    fn chunk(&mut self, pcm: &[u8], now: i64, out: &mut Vec<Output>) {
        let (pcm, rms) = self.gate.apply(pcm);
        let commit = self.turns.push(rms);
        self.send(append_message(&pcm), false, now, out);
        if commit {
            self.commit(now, out);
        }
    }

    fn commit(&mut self, now: i64, out: &mut Vec<Output>) {
        self.awaiting_commits += 1;
        self.send(commit_message(), true, now, out);
    }

    /// Sends a message, or keeps it while connecting (dropping the oldest beyond the limit).
    fn send(&mut self, text: String, commit: bool, now: i64, out: &mut Vec<Output>) {
        if self.connected {
            self.last_sent = now;
            out.push(Output::Send(text));
            return;
        }
        self.queued.push_back((text, commit));
        while self.queued.len() > BUFFER_LIMIT_MESSAGES {
            if let Some((_, true)) = self.queued.pop_front() {
                self.awaiting_commits = self.awaiting_commits.saturating_sub(1);
            }
        }
    }

    /// The user stopped: sends the rest of the audio (`rest` is what the microphone still held),
    /// commits the last turn when there was speech since the previous commit, and waits up to
    /// [`STOP_TIMEOUT_MS`] for the outstanding transcripts.
    pub(crate) fn stop(&mut self, rest: &[u8], now: i64) -> Vec<Output> {
        if self.done || self.stopped_at.is_some() {
            return Vec::new();
        }
        self.stopped_at = Some(now);
        self.pending.extend_from_slice(rest);
        let size = self.chunk_bytes();
        let mut out = Vec::new();
        while !self.pending.is_empty() {
            let take = size.min(self.pending.len());
            let chunk: Vec<u8> = self.pending.drain(..take).collect();
            self.chunk(&chunk, now, &mut out);
        }
        if self.turns.has_speech() {
            self.commit(now, &mut out);
        }
        self.turns.reset();
        if !self.connected {
            if self.awaiting_commits == 0 {
                // Nothing to transcribe: no need to wait for the connection.
                return self.finish(out);
            }
            self.stop_deadline = Some(now + CONNECT_TIMEOUT_MS + STOP_TIMEOUT_MS);
            return out;
        }
        self.stop_deadline = Some(now + STOP_TIMEOUT_MS);
        self.finish_if_settled(out)
    }

    /// While stopping, finishes once every commit was acknowledged and every turn completed.
    fn finish_if_settled(&mut self, out: Vec<Output>) -> Vec<Output> {
        if self.stopped_at.is_some()
            && self.connected
            && self.awaiting_commits == 0
            && self.transcripts.pending_count() == 0
        {
            return self.finish(out);
        }
        out
    }

    /// A text frame from the server.
    pub(crate) fn server(&mut self, raw: &str, now: i64) -> Vec<Output> {
        if self.done {
            return Vec::new();
        }
        let transcripts = match parse_server_message(raw) {
            ServerMessage::Delta { item_id, text } => self.transcripts.delta(&item_id, &text),
            ServerMessage::Completed { item_id, text } => {
                self.transcripts.completed(&item_id, &text)
            }
            // A failed turn is committed as empty text.
            ServerMessage::Failed { item_id, .. } => self.transcripts.completed(&item_id, ""),
            ServerMessage::Committed { item_id } => {
                self.awaiting_commits = self.awaiting_commits.saturating_sub(1);
                self.transcripts.committed(&item_id);
                Vec::new()
            }
            // The session keeps the settings of the ephemeral key.
            ServerMessage::Error { event_id, .. }
                if event_id.as_deref() == Some(SESSION_UPDATE_EVENT_ID) =>
            {
                return Vec::new();
            }
            // An empty commit (no audio since the last one) is not an error.
            ServerMessage::Error { code, .. } if IGNORED_ERROR_CODES.contains(&code.as_str()) => {
                self.awaiting_commits = self.awaiting_commits.saturating_sub(1);
                Vec::new()
            }
            ServerMessage::Error { code, text, .. } => {
                return self.finish(vec![Output::Error(format!(
                    "音声入力でエラーが起きました: {text}（{code}）"
                ))]);
            }
            ServerMessage::Other => return Vec::new(),
        };
        let mut out = Vec::new();
        for transcript in transcripts {
            match transcript {
                Transcript::Partial(text) => {
                    self.text_changed(&text, now);
                    out.push(Output::Partial(apply_replacements(
                        &text,
                        &self.client.replacements,
                    )));
                }
                Transcript::Committed(text) => {
                    self.text_changed("", now);
                    out.push(Output::Partial(String::new()));
                    let text = apply_replacements(&text, &self.client.replacements);
                    if !text.trim().is_empty() {
                        out.push(Output::Committed(text));
                    }
                }
            }
        }
        self.finish_if_settled(out)
    }

    /// Restarts the auto stop countdown when the shown text changes.
    fn text_changed(&mut self, text: &str, now: i64) {
        if text != self.last_text {
            self.last_text = text.to_owned();
            self.last_text_at = Some(now);
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

    /// Periodic check of the stop, auto stop and idle timeouts.
    pub(crate) fn tick(&mut self, now: i64) -> Vec<Output> {
        if self.done {
            return Vec::new();
        }
        if self.stopped_at.is_some() {
            if self.stop_deadline.is_some_and(|deadline| now >= deadline) {
                return self.finish(Vec::new());
            }
            return Vec::new();
        }
        let auto_stop_ms = (self.client.auto_stop_silence_sec * 1000.0) as i64;
        if let Some(changed_at) = self.last_text_at
            && auto_stop_ms > 0
            && !self.auto_stopping
            && now - changed_at >= auto_stop_ms
        {
            self.auto_stopping = true;
            return vec![Output::AutoStop];
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
    use serde_json::{Value, json};

    use super::*;

    /// 24 kHz: 4800 bytes per 100 ms chunk.
    const CHUNK: usize = 4800;

    fn loud() -> Vec<u8> {
        (0..CHUNK / 2).flat_map(|_| 8000i16.to_le_bytes()).collect()
    }

    fn quiet() -> Vec<u8> {
        vec![0; CHUNK]
    }

    fn client() -> AiDebugSpeechClientConfig {
        AiDebugSpeechClientConfig {
            auto_stop_silence_sec: 0.0,
            ..Default::default()
        }
    }

    fn core() -> VoiceCore {
        VoiceCore::new(24_000, None, client())
    }

    fn sent(outputs: &[Output]) -> Vec<Value> {
        outputs
            .iter()
            .filter_map(|output| match output {
                Output::Send(text) => Some(serde_json::from_str(text).unwrap()),
                _ => None,
            })
            .collect()
    }

    fn types(outputs: &[Output]) -> Vec<String> {
        sent(outputs)
            .iter()
            .map(|message| message["type"].as_str().unwrap().to_owned())
            .collect()
    }

    fn pcm_len(message: &Value) -> usize {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(message["audio"].as_str().unwrap())
            .unwrap()
            .len()
    }

    fn delta(item: &str, text: &str) -> String {
        json!({
            "type": "conversation.item.input_audio_transcription.delta",
            "item_id": item,
            "delta": text,
        })
        .to_string()
    }

    fn completed(item: &str, text: &str) -> String {
        json!({
            "type": "conversation.item.input_audio_transcription.completed",
            "item_id": item,
            "transcript": text,
        })
        .to_string()
    }

    fn committed(item: &str) -> String {
        json!({ "type": "input_audio_buffer.committed", "item_id": item }).to_string()
    }

    /// Speaks for 300 ms and stays quiet until the turn detector commits.
    fn speak_turn(core: &mut VoiceCore, now: i64) -> Vec<Output> {
        let mut out = Vec::new();
        for _ in 0..3 {
            out.extend(core.audio(&loud(), now));
        }
        for _ in 0..7 {
            out.extend(core.audio(&quiet(), now));
        }
        out
    }

    #[test]
    fn sends_the_session_update_first_then_the_buffered_audio() {
        let update = json!({ "type": "session.update", "session": { "type": "transcription" } });
        let mut core = VoiceCore::new(24_000, Some(update), client());
        assert!(core.audio(&[1; CHUNK + 200], 0).is_empty());
        let messages = sent(&core.connected(10));
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["type"], "session.update");
        assert_eq!(messages[0]["event_id"], "samurai_session_update");
        assert_eq!(messages[1]["type"], "input_audio_buffer.append");
        assert_eq!(pcm_len(&messages[1]), CHUNK);
        let messages = sent(&core.audio(&[1; CHUNK - 200], 20));
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["type"], "input_audio_buffer.append");
        // Connecting again sends nothing.
        assert!(core.connected(30).is_empty());
    }

    #[test]
    fn keeps_fifteen_seconds_while_connecting() {
        let mut core = self::core();
        core.audio(&vec![0; CHUNK * 200], 0);
        assert_eq!(sent(&core.connected(0)).len(), 150);
    }

    #[test]
    fn commits_after_speech_and_silence() {
        let mut core = self::core();
        core.connected(0);
        let out = speak_turn(&mut core, 0);
        let kinds = types(&out);
        assert_eq!(kinds.len(), 11);
        assert_eq!(kinds[9], "input_audio_buffer.append");
        assert_eq!(kinds[10], "input_audio_buffer.commit");
        // Silence alone never commits.
        let out = core.audio(&vec![0; CHUNK * 20], 0);
        assert!(
            types(&out)
                .iter()
                .all(|kind| kind == "input_audio_buffer.append")
        );
    }

    #[test]
    fn the_noise_gate_silences_quiet_chunks() {
        let mut core = VoiceCore::new(
            24_000,
            None,
            AiDebugSpeechClientConfig {
                noise_gate_db: Some(-10.0),
                ..client()
            },
        );
        core.connected(0);
        // 8000 / 32768 is about -12 dBFS: below the gate, sent as zeros and never a turn.
        let out = speak_turn(&mut core, 0);
        assert!(!types(&out).contains(&"input_audio_buffer.commit".to_owned()));
        let first = sent(&out)[0]["audio"].as_str().unwrap().to_owned();
        assert_eq!(first, sent(&core.audio(&quiet(), 0))[0]["audio"]);
    }

    #[test]
    fn commits_turns_in_order_when_completions_arrive_out_of_order() {
        let mut core = self::core();
        core.connected(0);
        assert!(core.server(&committed("a"), 0).is_empty());
        assert!(core.server(&committed("b"), 0).is_empty());
        assert_eq!(
            core.server(&delta("a", "こん"), 0),
            vec![Output::Partial("こん".into())]
        );
        assert_eq!(
            core.server(&delta("b", "Flutter"), 0),
            vec![Output::Partial("こんFlutter".into())]
        );
        // "b" completes first: it waits behind "a".
        assert_eq!(
            core.server(&completed("b", "Flutterです"), 0),
            vec![Output::Partial("こんFlutterです".into())]
        );
        assert_eq!(
            core.server(&completed("a", " こんにちは "), 0),
            vec![
                Output::Partial(String::new()),
                Output::Committed("こんにちは".into()),
                Output::Partial(String::new()),
                Output::Committed("Flutterです".into()),
            ]
        );
        // Late events of a committed turn are ignored.
        assert!(core.server(&delta("a", "x"), 0).is_empty());
        // An empty or failed turn only clears the interim text.
        core.server(&committed("c"), 0);
        core.server(&delta("c", "えー"), 0);
        assert_eq!(
            core.server(
                r#"{"type":"conversation.item.input_audio_transcription.failed","item_id":"c"}"#,
                0
            ),
            vec![Output::Partial(String::new())]
        );
    }

    #[test]
    fn applies_replacements() {
        let mut core = VoiceCore::new(
            24_000,
            None,
            AiDebugSpeechClientConfig {
                replacements: vec![("フラッター".into(), "Flutter".into())],
                ..client()
            },
        );
        core.connected(0);
        core.server(&committed("a"), 0);
        assert_eq!(
            core.server(&delta("a", "フラッター"), 0),
            vec![Output::Partial("Flutter".into())]
        );
        assert_eq!(
            core.server(&completed("a", "フラッターの画面"), 0),
            vec![
                Output::Partial(String::new()),
                Output::Committed("Flutterの画面".into())
            ]
        );
    }

    #[test]
    fn stop_sends_the_rest_commits_and_waits_for_completion() {
        let mut core = self::core();
        core.connected(0);
        for _ in 0..3 {
            core.audio(&loud(), 0);
        }
        core.audio(&loud()[..1000], 0);
        let out = core.stop(&loud()[..2000], 100);
        let messages = sent(&out);
        // 3000 bytes left: one short append, then the final commit.
        assert_eq!(messages.len(), 2);
        assert_eq!(pcm_len(&messages[0]), 3000);
        assert_eq!(messages[1]["type"], "input_audio_buffer.commit");
        assert!(!out.contains(&Output::Finish));
        // Audio after stopping is dropped.
        assert!(core.audio(&loud(), 120).is_empty());
        assert!(core.server(&committed("a"), 200).is_empty());
        assert_eq!(
            core.server(&delta("a", "こん"), 300),
            vec![Output::Partial("こん".into())]
        );
        assert_eq!(
            core.server(&completed("a", "こんにちは"), 400),
            vec![
                Output::Partial(String::new()),
                Output::Committed("こんにちは".into()),
                Output::Finish
            ]
        );
        assert!(core.tick(10_000).is_empty());
    }

    #[test]
    fn stop_waits_for_turns_committed_while_recording() {
        let mut core = self::core();
        core.connected(0);
        speak_turn(&mut core, 0);
        // No speech since the commit: no final commit, but the earlier turn is outstanding.
        let out = core.stop(&[], 1_000);
        assert!(sent(&out).is_empty());
        assert!(!out.contains(&Output::Finish));
        assert!(core.tick(1_000 + STOP_TIMEOUT_MS - 1).is_empty());
        assert_eq!(core.tick(1_000 + STOP_TIMEOUT_MS), vec![Output::Finish]);
    }

    #[test]
    fn stop_without_anything_outstanding_finishes_at_once() {
        let mut core = self::core();
        core.connected(0);
        core.audio(&quiet(), 0);
        let out = core.stop(&quiet(), 0);
        assert_eq!(types(&out), vec!["input_audio_buffer.append"]);
        assert_eq!(out.last(), Some(&Output::Finish));
    }

    #[test]
    fn committed_text_while_recording_keeps_going() {
        let mut core = self::core();
        core.connected(0);
        core.server(&committed("a"), 0);
        assert_eq!(
            core.server(&completed("a", "one"), 0),
            vec![
                Output::Partial(String::new()),
                Output::Committed("one".into())
            ]
        );
        assert_eq!(
            core.server(&completed("b", " "), 0),
            vec![Output::Partial(String::new())]
        );
        assert!(core.tick(1_000).is_empty());
    }

    #[test]
    fn errors() {
        let mut core = self::core();
        core.connected(0);
        let out = core.server(
            r#"{"type":"error","error":{"type":"invalid_request_error","code":"bad","message":"Bad request"}}"#,
            0,
        );
        assert_eq!(
            out[0],
            Output::Error("音声入力でエラーが起きました: Bad request（bad）".into())
        );
        assert_eq!(out.last(), Some(&Output::Finish));
        assert!(core.server(&delta("a", "x"), 0).is_empty());

        // Closing while recording is an error, after stopping it is not.
        let mut core = self::core();
        core.connected(0);
        assert!(matches!(&core.closed(None)[0], Output::Error(_)));
        let mut core = self::core();
        core.connected(0);
        speak_turn(&mut core, 0);
        core.stop(&[], 0);
        assert_eq!(core.closed(None), vec![Output::Finish]);
    }

    #[test]
    fn ignores_session_update_errors_and_empty_commits() {
        let mut core = self::core();
        core.connected(0);
        assert!(
            core.server(
                r#"{"type":"error","error":{"code":"unknown_parameter","message":"x","event_id":"samurai_session_update"}}"#,
                0
            )
            .is_empty()
        );
        // The final commit turns out empty: the error only settles the stop.
        core.audio(&loud(), 0);
        core.audio(&loud(), 0);
        let out = core.stop(&[], 0);
        assert_eq!(types(&out), vec!["input_audio_buffer.commit"]);
        assert!(!out.contains(&Output::Finish));
        assert_eq!(
            core.server(
                r#"{"type":"error","error":{"code":"input_audio_buffer_commit_empty","message":"empty"}}"#,
                0
            ),
            vec![Output::Finish]
        );
    }

    #[test]
    fn stops_automatically_when_the_text_stays_the_same() {
        let mut core = VoiceCore::new(
            24_000,
            None,
            AiDebugSpeechClientConfig {
                auto_stop_silence_sec: 5.0,
                ..Default::default()
            },
        );
        core.connected(1_000);
        assert!(core.tick(5_999).is_empty());
        // New text restarts the countdown.
        core.server(&delta("a", "こん"), 5_000);
        assert!(core.tick(9_999).is_empty());
        assert_eq!(core.tick(10_000), vec![Output::AutoStop]);
        // Asked once; the session then stops like the user did.
        assert!(core.tick(10_250).is_empty());
        let out = core.stop(&[], 10_300);
        assert!(!out.iter().any(|output| matches!(output, Output::Error(_))));

        // 0 disables it.
        let mut core = self::core();
        core.connected(0);
        core.audio(&quiet(), 14_000);
        assert!(core.tick(14_500).is_empty());
    }

    #[test]
    fn stops_after_fifteen_idle_seconds() {
        let mut core = self::core();
        core.connected(0);
        core.audio(&quiet(), 1_000);
        assert!(core.tick(1_000 + IDLE_TIMEOUT_MS - 1).is_empty());
        let out = core.tick(1_000 + IDLE_TIMEOUT_MS);
        assert!(matches!(&out[0], Output::Error(_)));
        assert_eq!(out.last(), Some(&Output::Finish));
    }

    #[test]
    fn stopping_before_connecting() {
        // Nothing to transcribe: finishes at once.
        let mut core = self::core();
        core.audio(&vec![0; CHUNK * 2], 0);
        assert_eq!(core.stop(&[], 0), vec![Output::Finish]);
        assert!(core.connected(0).is_empty());

        // Speech: the audio and the final commit go out once connected.
        let mut core = self::core();
        for _ in 0..3 {
            core.audio(&loud(), 0);
        }
        assert!(core.stop(&[], 0).is_empty());
        let out = core.connected(500);
        assert_eq!(
            types(&out),
            vec![
                "input_audio_buffer.append",
                "input_audio_buffer.append",
                "input_audio_buffer.append",
                "input_audio_buffer.commit",
            ]
        );
        assert!(core.tick(500 + STOP_TIMEOUT_MS - 1).is_empty());
        assert_eq!(core.tick(500 + STOP_TIMEOUT_MS), vec![Output::Finish]);

        // Never connecting gives up after the connect and stop timeouts.
        let mut core = self::core();
        for _ in 0..3 {
            core.audio(&loud(), 0);
        }
        core.stop(&[], 0);
        assert!(
            core.tick(CONNECT_TIMEOUT_MS + STOP_TIMEOUT_MS - 1)
                .is_empty()
        );
        assert_eq!(
            core.tick(CONNECT_TIMEOUT_MS + STOP_TIMEOUT_MS),
            vec![Output::Finish]
        );
    }
}
