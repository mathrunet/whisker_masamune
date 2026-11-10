//! Controller behavior, mirroring `masamune_ai_debugger_test.dart`.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::LocalBoxFuture;
use masamune_whisker_ai_debugger::{
    AiDebugAccount, AiDebugContextSnapshot, AiDebugController, AiDebugError, AiDebugModel,
    AiDebugPermissionMode, AiDebugSettings, AiDebugSettingsStore, AiDebugTransport,
    AiDebuggerConfig, MemorySettingsStore,
};
use masamune_whisker_logger::{Flavor, LoggerAdapter};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug)]
struct Call {
    path: String,
    headers: Vec<(String, String)>,
    body: Value,
}

type Answers = VecDeque<(String, Result<Value, AiDebugError>)>;

/// Records every request and answers from a queue (default `{}`).
#[derive(Clone, Default)]
struct MockTransport {
    calls: Rc<RefCell<Vec<Call>>>,
    answers: Rc<RefCell<Answers>>,
}

impl MockTransport {
    fn answer(&self, path_suffix: &str, answer: Result<Value, AiDebugError>) {
        self.answers
            .borrow_mut()
            .push_back((path_suffix.to_owned(), answer));
    }

    fn paths(&self) -> Vec<String> {
        self.calls.borrow().iter().map(|c| c.path.clone()).collect()
    }

    fn bodies(&self, suffix: &str) -> Vec<Value> {
        self.calls
            .borrow()
            .iter()
            .filter(|c| c.path.ends_with(suffix))
            .map(|c| c.body.clone())
            .collect()
    }
}

impl AiDebugTransport for MockTransport {
    fn post(
        &self,
        url: String,
        headers: Vec<(String, String)>,
        body: Value,
    ) -> LocalBoxFuture<'static, Result<Map<String, Value>, AiDebugError>> {
        let path = url
            .strip_prefix("https://samurai.test/base")
            .unwrap_or(&url)
            .to_owned();
        self.calls.borrow_mut().push(Call {
            path: path.clone(),
            headers,
            body,
        });
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

fn config(transport: &MockTransport) -> AiDebuggerConfig {
    AiDebuggerConfig::new(Flavor::Dev)
        .project_id("Users-me-app")
        .endpoint("https://samurai.test/base/")
        .api_key("key")
        .transport(transport.clone())
        .settings_store(MemorySettingsStore::default())
}

fn attach(controller: &AiDebugController) {
    controller.attach(Rc::new(|future| {
        tokio::task::spawn_local(future);
    }));
}

async fn run<F: std::future::Future<Output = ()>>(f: F) {
    tokio::task::LocalSet::new().run_until(f).await;
}

/// Lets spawned tasks run.
async fn settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn registers_once_and_sends_requests() {
    run(async {
        let transport = MockTransport::default();
        transport.answer(
            "/screenshots",
            Ok(json!({ "screenshot": { "name": "stored.png" } })),
        );
        transport.answer("/request", Ok(json!({ "sessionId": "session-1" })));
        let controller = AiDebugController::new(
            config(&transport)
                .settings(AiDebugSettings {
                    provider: Some("claude".into()),
                    ..Default::default()
                })
                .context_provider(|| async {
                    let mut values = Map::new();
                    values.insert("apiToken".into(), "secret".into());
                    values.insert("email".into(), "user@example.com".into());
                    Some(AiDebugContextSnapshot {
                        route: Some("/home".into()),
                        values,
                        ..Default::default()
                    })
                }),
        )
        .unwrap();
        attach(&controller);
        let session = controller
            .send(
                "fix it password=hunter2",
                &[vec![1, 2, 3]],
                Some(AiDebugModel::Astra),
                None,
            )
            .await
            .unwrap();
        assert_eq!(session.as_deref(), Some("session-1"));
        let run_id = controller.run_id();
        assert_eq!(
            transport.paths(),
            [
                "/api/app-debug/runs".to_owned(),
                format!("/api/app-debug/runs/{run_id}/screenshots"),
                format!("/api/app-debug/runs/{run_id}/request"),
            ]
        );
        let call = transport.calls.borrow()[0].clone();
        assert!(
            call.headers
                .contains(&("x-api-key".to_owned(), "key".to_owned()))
        );
        assert_eq!(call.body["projectId"], "Users-me-app");
        assert_eq!(call.body["flavor"], "debug");
        assert_eq!(call.body["maxSessionsPerHour"], 6);
        let screenshot = &transport.bodies("/screenshots")[0];
        assert_eq!(screenshot["data"], "data:image/png;base64,AQID");
        let request = &transport.bodies("/request")[0];
        assert_eq!(request["instruction"], "fix it password=[REDACTED]");
        assert_eq!(request["screenshotNames"], json!(["stored.png"]));
        assert_eq!(request["provider"], "claude");
        assert_eq!(request["model"], "astra");
        assert_eq!(request["permissionMode"], "plan");
        assert_eq!(request["context"]["route"], "/home");
        assert_eq!(request["context"]["values"]["apiToken"], "[REDACTED]");
        assert_eq!(request["context"]["values"]["email"], "[REDACTED_EMAIL]");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn automatic_provider_is_omitted() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        controller.send("hello", &[], None, None).await.unwrap();
        let request = &transport.bodies("/request")[0];
        assert!(request.get("provider").is_none());
        assert!(request.get("context").is_none());
        assert_eq!(request["model"], "sol");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn missing_configuration_fails() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport).project_id("")).unwrap();
        assert_eq!(
            controller.register().await,
            Err(AiDebugError::Config(
                "MASAMUNE_AI_DEBUGGER_PROJECT_ID is not configured".into()
            ))
        );
        let controller = AiDebugController::new(config(&transport).api_key("")).unwrap();
        assert_eq!(
            controller.register().await.unwrap_err().to_string(),
            "MASAMUNE_AI_DEBUGGER_API_KEY is not configured"
        );
        assert!(transport.paths().is_empty());
    })
    .await;
}

#[test]
fn initial_provider_must_be_listed() {
    let transport = MockTransport::default();
    let result = AiDebugController::new(
        config(&transport)
            .providers([" codex ", "codex", "", "claude"])
            .settings(AiDebugSettings {
                provider: Some("gemini".into()),
                ..Default::default()
            }),
    );
    assert!(result.is_err());
    let controller =
        AiDebugController::new(config(&transport).providers([" codex ", "codex", "", "claude"]))
            .unwrap();
    assert_eq!(controller.providers(), ["codex", "claude"]);
}

#[tokio::test(start_paused = true)]
async fn failed_registration_is_retried() {
    run(async {
        let transport = MockTransport::default();
        transport.answer(
            "/runs",
            Err(AiDebugError::Http {
                status: 500,
                message: "down".into(),
            }),
        );
        let controller = AiDebugController::new(config(&transport)).unwrap();
        assert!(controller.register().await.is_err());
        controller.register().await.unwrap();
        controller.register().await.unwrap();
        assert_eq!(transport.paths().len(), 2);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn expired_run_is_rotated() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        controller.register().await.unwrap();
        let first = controller.run_id();
        transport.answer(
            "/heartbeat",
            Err(AiDebugError::Http {
                status: 410,
                message: "gone".into(),
            }),
        );
        controller.heartbeat().await.unwrap();
        let second = controller.run_id();
        assert_ne!(first, second);
        assert_eq!(
            transport.paths(),
            [
                "/api/app-debug/runs".to_owned(),
                format!("/api/app-debug/runs/{first}/heartbeat"),
                "/api/app-debug/runs".to_owned(),
                format!("/api/app-debug/runs/{second}/heartbeat"),
            ]
        );
        assert_eq!(transport.bodies("/runs")[1]["runId"], second);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn heartbeat_repeats_until_paused_and_end_runs_once() {
    run(async {
        let transport = MockTransport::default();
        let controller =
            AiDebugController::new(config(&transport).heartbeat_interval(Duration::from_secs(30)))
                .unwrap();
        attach(&controller);
        controller.resume().await.unwrap();
        tokio::time::sleep(Duration::from_secs(61)).await;
        settle().await;
        assert_eq!(transport.bodies("/heartbeat").len(), 3);
        controller.pause();
        tokio::time::sleep(Duration::from_secs(120)).await;
        settle().await;
        assert_eq!(transport.bodies("/heartbeat").len(), 3);
        controller.end().await;
        controller.end().await;
        assert_eq!(transport.bodies("/end").len(), 1);
        let run_id = controller.run_id();
        assert_eq!(
            transport.paths().last().unwrap(),
            &format!("/api/app-debug/runs/{run_id}/end")
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn end_without_registration_sends_nothing() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        controller.end().await;
        assert!(transport.paths().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn incidents_are_deduplicated_and_throttled() {
    run(async {
        let transport = MockTransport::default();
        transport.answer(
            "/incidents",
            Ok(json!({ "sessionCreated": true, "sessionId": "s1" })),
        );
        let controller = AiDebugController::new(config(&transport)).unwrap();
        let notified = Rc::new(RefCell::new(Vec::new()));
        controller.set_incident_listener({
            let notified = notified.clone();
            move |kind, session| {
                notified
                    .borrow_mut()
                    .push((kind.to_owned(), session.to_owned()))
            }
        });
        controller.report_error("failed at item 12", "stack").await;
        // Right after another incident: throttled, but not remembered.
        controller.report_error("other failure", "").await;
        tokio::time::sleep(Duration::from_secs(2)).await;
        // Same message with other digits: duplicate.
        controller.report_error("failed at item 99", "stack").await;
        controller.report_error("other failure", "").await;
        controller.report_error("third failure", "").await;
        let incidents = transport.bodies("/incidents");
        assert_eq!(incidents.len(), 2);
        assert_eq!(incidents[0]["kind"], "exception");
        assert_eq!(incidents[0]["message"], "failed at item 12");
        assert_eq!(incidents[0]["model"], "sol");
        assert!(incidents[0].get("metadata").is_none());
        assert_eq!(incidents[1]["message"], "other failure");
        assert_eq!(
            *notified.borrow(),
            [("exception".to_owned(), "s1".to_owned())]
        );
        // Only registration and incidents were sent.
        assert!(
            transport
                .paths()
                .iter()
                .all(|p| p.ends_with("/runs") || p.ends_with("/incidents"))
        );

        // A new controller (app launch) forgets the fingerprints.
        let controller = AiDebugController::new(config(&transport)).unwrap();
        controller.report_error("failed at item 12", "stack").await;
        assert_eq!(transport.bodies("/incidents").len(), 3);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn incident_attaches_screenshot() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        controller.set_capture(|| async { Some(vec![9]) });
        controller.report_error("boom", "").await;
        let screenshots = transport.bodies("/screenshots");
        assert_eq!(screenshots.len(), 1);
        assert!(
            screenshots[0]["name"]
                .as_str()
                .unwrap()
                .starts_with("exception-")
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn logs_are_batched() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        attach(&controller);
        controller.add_log("tap", json!({ "token": "abc" }), "info");
        settle().await;
        assert!(transport.bodies("/events").is_empty());
        tokio::time::sleep(Duration::from_secs(2)).await;
        settle().await;
        let events = transport.bodies("/events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["events"][0]["message"], r#"tap {"token":"abc"}"#);
        assert_eq!(events[0]["events"][0]["severity"], "info");

        for index in 0..20 {
            controller.add_log("tap", json!({ "index": index }), "info");
        }
        settle().await;
        assert_eq!(transport.bodies("/events").len(), 2);
        assert_eq!(
            transport.bodies("/events")[1]["events"]
                .as_array()
                .unwrap()
                .len(),
            20
        );

        // A failed upload keeps the batch.
        transport.answer(
            "/events",
            Err(AiDebugError::Http {
                status: 500,
                message: "down".into(),
            }),
        );
        controller.add_log("a", Value::Null, "info");
        controller.flush_logs().await;
        assert_eq!(controller.pending_logs(), 1);
        controller.flush_logs().await;
        assert_eq!(controller.pending_logs(), 0);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn settings_are_saved_per_project() {
    run(async {
        #[derive(Clone, Default)]
        struct Shared(Rc<MemorySettingsStore>);
        impl AiDebugSettingsStore for Shared {
            fn load(&self, key: &str) -> Result<Option<String>, String> {
                self.0.load(key)
            }
            fn save(&self, key: &str, value: &str) -> Result<(), String> {
                self.0.save(key, value)
            }
        }
        let store = Shared::default();
        let transport = MockTransport::default();
        let controller =
            AiDebugController::new(config(&transport).settings_store(store.clone())).unwrap();
        controller
            .update_settings(AiDebugSettings {
                manual_model: AiDebugModel::Luna,
                manual_permission_mode: AiDebugPermissionMode::BypassPermissions,
                ..Default::default()
            })
            .unwrap();
        let reloaded =
            AiDebugController::new(config(&transport).settings_store(store.clone())).unwrap();
        assert_eq!(reloaded.settings().manual_model, AiDebugModel::Luna);
        let other = AiDebugController::new(
            config(&transport)
                .project_id("other")
                .settings_store(store.clone()),
        )
        .unwrap();
        assert_eq!(other.settings().manual_model, AiDebugModel::Sol);

        reloaded.send("go", &[], None, None).await.unwrap();
        let request = &transport.bodies("/request")[0];
        assert_eq!(request["model"], "luna");
        assert_eq!(request["permissionMode"], "bypassPermissions");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn slow_traces_report_one_incident() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        attach(&controller);
        let adapter = controller.logger_adapter();
        adapter.trace_start(1, "masamune.model.load|document|load|db.users");
        settle().await;
        tokio::time::sleep(Duration::from_secs(6)).await;
        settle().await;
        let incidents = transport.bodies("/incidents");
        assert_eq!(incidents.len(), 1);
        assert_eq!(incidents[0]["kind"], "performance");
        assert_eq!(
            incidents[0]["message"],
            "Performance threshold exceeded: masamune.model.load|document|load|db.users"
        );
        assert_eq!(incidents[0]["stackTrace"], "");
        assert_eq!(incidents[0]["metadata"]["category"], "model_load");
        assert_eq!(incidents[0]["metadata"]["thresholdMs"], 5000);
        adapter.trace_stop(1, "masamune.model.load|document|load|db.users");
        settle().await;
        controller.flush_logs().await;
        let events = transport.bodies("/events");
        let message = events[0]["events"][0]["message"].as_str().unwrap();
        assert!(
            message.contains(r#""threshold_exceeded":true"#),
            "{message}"
        );

        // A finished indicator cancels its check.
        adapter.trace_start(2, "masamune.indicator.show|busy");
        settle().await;
        tokio::time::sleep(Duration::from_secs(3)).await;
        adapter.trace_stop(2, "masamune.indicator.show|busy");
        settle().await;
        tokio::time::sleep(Duration::from_secs(20)).await;
        settle().await;
        assert_eq!(transport.bodies("/incidents").len(), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn handled_errors_follow_the_setting() {
    run(async {
        #[derive(Debug)]
        struct Failure;
        impl std::fmt::Display for Failure {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("sync failed")
            }
        }
        impl std::error::Error for Failure {}

        let transport = MockTransport::default();
        let controller =
            AiDebugController::new(config(&transport).report_handled_errors(false)).unwrap();
        attach(&controller);
        controller
            .logger_adapter()
            .error(&Failure, Some("sync_failed"));
        settle().await;
        controller.flush_logs().await;
        assert!(transport.bodies("/incidents").is_empty());
        let events = transport.bodies("/events");
        assert_eq!(events[0]["events"][0]["severity"], "error");

        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        attach(&controller);
        controller
            .logger_adapter()
            .error(&Failure, Some("sync_failed"));
        settle().await;
        let incidents = transport.bodies("/incidents");
        assert_eq!(incidents[0]["message"], "sync_failed: sync failed");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn prod_does_nothing() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(
            AiDebuggerConfig::new(Flavor::Prod)
                .project_id("p")
                .endpoint("https://samurai.test/base")
                .api_key("key")
                .transport(transport.clone()),
        )
        .unwrap();
        attach(&controller);
        assert!(!controller.enabled());
        controller.resume().await.unwrap();
        assert_eq!(controller.send("x", &[vec![1]], None, None).await, Ok(None));
        controller.report_error("boom", "").await;
        controller.add_log("tap", json!({}), "info");
        controller.flush_logs().await;
        controller
            .logger_adapter()
            .trace_start(1, "masamune.model.load|x");
        controller.end().await;
        tokio::time::sleep(Duration::from_secs(60)).await;
        settle().await;
        assert!(transport.paths().is_empty());
    })
    .await;
}

#[test]
fn parses_accounts() {
    let accounts =
        AiDebugAccount::parse(" a@x.com:p:1 , bad, :nopass, b@x.com:, c@x.com:pw, a@x.com:other ,");
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].id, "a@x.com");
    assert_eq!(accounts[0].password, "p:1");
    assert_eq!(accounts[1].id, "c@x.com");
    assert!(AiDebugAccount::parse("").is_empty());
    // The password never appears in debug output.
    assert!(!format!("{:?}", accounts[1]).contains("pw"));

    let config = AiDebuggerConfig::from_values(
        Flavor::Dev,
        Some("project"),
        None,
        Some("key"),
        Some("u@x.com:secret"),
    );
    assert!(config.enabled());
    let config = masamune_whisker_ai_debugger::ai_debugger_config!(Flavor::Prod);
    assert!(!config.enabled());
}

#[tokio::test(start_paused = true)]
async fn queue_survives_a_cancelled_attach() {
    run(async {
        let transport = MockTransport::default();
        let controller = AiDebugController::new(config(&transport)).unwrap();
        // An overlay whose owner is gone drops the task without running it.
        controller.attach(Rc::new(drop));
        controller
            .logger_adapter()
            .send(&masamune_whisker_logger::LogEvent::new("tap"));
        attach(&controller);
        settle().await;
        controller.flush_logs().await;
        let events = transport.bodies("/events");
        assert_eq!(events[0]["events"][0]["message"], "tap {}");
    })
    .await;
}
