#![cfg(debug_assertions)]

//! Speech token requests and the availability of the voice input button.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use futures_util::FutureExt;
use futures_util::future::LocalBoxFuture;
use masamune_whisker_ai_debugger::{
    AiDebugController, AiDebugError, AiDebugTransport, AiDebuggerConfig, MemorySettingsStore,
    VOICE_INPUT_LABEL,
};
use masamune_whisker_logger::Flavor;
use serde_json::{Map, Value, json};

type Answers = VecDeque<(String, Result<Value, AiDebugError>)>;
type Calls = Rc<RefCell<Vec<(String, Vec<(String, String)>)>>>;

/// Records the request paths and answers from a queue (default `{}`).
#[derive(Clone, Default)]
struct MockTransport {
    calls: Calls,
    answers: Rc<RefCell<Answers>>,
}

impl MockTransport {
    fn answer(&self, path_suffix: &str, answer: Result<Value, AiDebugError>) {
        self.answers
            .borrow_mut()
            .push_back((path_suffix.to_owned(), answer));
    }

    fn paths(&self) -> Vec<String> {
        self.calls
            .borrow()
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }
}

impl AiDebugTransport for MockTransport {
    fn post(
        &self,
        url: String,
        headers: Vec<(String, String)>,
        _body: Value,
    ) -> LocalBoxFuture<'static, Result<Map<String, Value>, AiDebugError>> {
        let path = url
            .strip_prefix("https://samurai.test")
            .unwrap_or(&url)
            .to_owned();
        self.calls.borrow_mut().push((path.clone(), headers));
        let answer = {
            let mut answers = self.answers.borrow_mut();
            let index = answers
                .iter()
                .position(|(suffix, _)| path.ends_with(suffix));
            index.and_then(|i| answers.remove(i)).map(|(_, a)| a)
        };
        async move {
            match answer {
                Some(Ok(Value::Object(map))) => Ok(map),
                Some(Ok(_)) | None => Ok(Map::new()),
                Some(Err(error)) => Err(error),
            }
        }
        .boxed_local()
    }
}

fn config(transport: &MockTransport, flavor: Flavor) -> AiDebuggerConfig {
    AiDebuggerConfig::new(flavor)
        .project_id("Users-me-app")
        .endpoint("https://samurai.test")
        .api_key("key")
        .transport(transport.clone())
        .settings_store(MemorySettingsStore::default())
}

fn token_answer() -> Value {
    json!({
        "success": true,
        "provider": "openai",
        "url": "wss://api.openai.com/v1/realtime?intent=transcription",
        "protocols": ["realtime", "openai-insecure-api-key.ek_single_use"],
        "sessionUpdate": {
            "type": "session.update",
            "session": { "type": "transcription" },
        },
        "modelId": "gpt-live-transcribe",
        "sampleRate": 24000,
        "keytermCount": 1,
        "skippedKeyterms": [],
        "client": {
            "autoStopSilenceSec": 5,
            "sounds": true,
            "localPreview": true,
            "commitSilenceMs": 700,
            "noiseGateDb": null,
            "languageCode": "ja",
            "replacements": [{ "from": "ウィスカー", "to": "Whisker" }],
        },
        "expiresAt": "2026-11-12T10:15:00.000Z",
    })
}

#[tokio::test]
async fn issues_speech_tokens_for_the_run() {
    let transport = MockTransport::default();
    transport.answer("/speech-token", Ok(token_answer()));
    let controller = AiDebugController::new(config(&transport, Flavor::Dev)).unwrap();
    let token = controller.speech_token().await.unwrap();
    assert!(token.url.starts_with("wss://"));
    assert_eq!(
        token.protocols,
        vec!["realtime", "openai-insecure-api-key.ek_single_use"]
    );
    assert!(token.session_update.is_some());
    assert_eq!(token.model_id.as_deref(), Some("gpt-live-transcribe"));
    assert_eq!(token.sample_rate, 24_000);
    assert_eq!(token.client.commit_silence_ms, 700);
    assert_eq!(token.client.noise_gate_db, None);
    assert_eq!(
        token.client.replacements,
        vec![("ウィスカー".to_owned(), "Whisker".to_owned())]
    );
    assert!(!format!("{token:?}").contains("ek_single_use"));
    let run_id = controller.run_id();
    assert_eq!(
        transport.paths(),
        [
            "/api/app-debug/runs".to_owned(),
            format!("/api/app-debug/runs/{run_id}/speech-token"),
        ]
    );
    let headers = &transport.calls.borrow()[1].1;
    assert!(headers.contains(&("x-api-key".to_owned(), "key".to_owned())));
}

#[tokio::test]
async fn recreates_a_forgotten_run_once() {
    let transport = MockTransport::default();
    let controller = AiDebugController::new(config(&transport, Flavor::Dev)).unwrap();
    controller.register().await.unwrap();
    let first = controller.run_id();
    transport.answer(
        "/speech-token",
        Err(AiDebugError::Http {
            status: 404,
            message: "Debug run not found".into(),
        }),
    );
    transport.answer("/speech-token", Ok(token_answer()));
    controller.speech_token().await.unwrap();
    let second = controller.run_id();
    assert_ne!(first, second);
    assert_eq!(
        transport.paths(),
        [
            "/api/app-debug/runs".to_owned(),
            format!("/api/app-debug/runs/{first}/speech-token"),
            "/api/app-debug/runs".to_owned(),
            format!("/api/app-debug/runs/{second}/speech-token"),
        ]
    );
}

#[tokio::test]
async fn passes_server_errors_through() {
    let transport = MockTransport::default();
    transport.answer(
        "/speech-token",
        Err(AiDebugError::Http {
            status: 412,
            message: "OpenAI API key is not configured".into(),
        }),
    );
    let controller = AiDebugController::new(config(&transport, Flavor::Dev)).unwrap();
    let error = controller.speech_token().await.unwrap_err();
    assert_eq!(error.status(), Some(412));

    // A 2xx answer without a URL is not a token.
    transport.answer("/speech-token", Ok(json!({ "success": true })));
    assert!(matches!(
        controller.speech_token().await,
        Err(AiDebugError::Transport(_))
    ));
}

#[tokio::test]
async fn prod_never_requests_tokens() {
    let transport = MockTransport::default();
    let controller = AiDebugController::new(config(&transport, Flavor::Prod)).unwrap();
    assert!(matches!(
        controller.speech_token().await,
        Err(AiDebugError::Config(_))
    ));
    assert!(transport.paths().is_empty());
}

#[test]
fn the_button_needs_the_feature_an_enabled_debugger_and_the_setting() {
    let transport = MockTransport::default();
    let dev = AiDebugController::new(config(&transport, Flavor::Dev)).unwrap();
    // Without the `voice` feature the overlay has no voice button at all.
    assert_eq!(dev.voice_input_available(), cfg!(feature = "voice"));
    let turned_off =
        AiDebugController::new(config(&transport, Flavor::Dev).voice_input(false)).unwrap();
    assert!(!turned_off.voice_input_available());
    let prod = AiDebugController::new(config(&transport, Flavor::Prod)).unwrap();
    assert!(!prod.voice_input_available());
    assert_eq!(VOICE_INPUT_LABEL, "AI debugger voice input");
}
