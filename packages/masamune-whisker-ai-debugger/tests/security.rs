//! Profile and flavor security boundary, including a caller using the exported macro.
use std::rc::Rc;

use futures_util::future::LocalBoxFuture;
use masamune_whisker_ai_debugger::{
    AiDebugController, AiDebugError, AiDebugSettings, AiDebugSettingsStore, AiDebugTransport,
    AiDebuggerConfig, ai_debugger_config,
};
use masamune_whisker_logger::Flavor;
use serde_json::{Map, Value};

struct ForbiddenIo;
impl AiDebugTransport for ForbiddenIo {
    fn post(
        &self,
        _: String,
        _: Vec<(String, String)>,
        _: Value,
    ) -> LocalBoxFuture<'static, Result<Map<String, Value>, AiDebugError>> {
        panic!("disabled debugger must not send requests")
    }
}
impl AiDebugSettingsStore for ForbiddenIo {
    fn load(&self, _: &str) -> Result<Option<String>, String> {
        panic!("disabled settings read")
    }
    fn save(&self, _: &str, _: &str) -> Result<(), String> {
        panic!("disabled settings write")
    }
}

#[test]
fn requires_both_debug_profile_and_explicit_dev_flavor() {
    assert!(!ai_debugger_config!(Flavor::Prod).enabled());
    assert!(!ai_debugger_config!(Flavor::from_env(None).unwrap()).enabled());
    assert_eq!(
        ai_debugger_config!(Flavor::Dev).enabled(),
        cfg!(debug_assertions)
    );
}

#[tokio::test]
async fn disabled_controller_does_not_start_tasks_or_access_io() {
    for flavor in [Flavor::Prod, Flavor::Dev] {
        if cfg!(debug_assertions) && flavor == Flavor::Dev {
            continue;
        }
        let controller = AiDebugController::new(
            AiDebuggerConfig::from_values(
                flavor,
                Some("p"),
                Some("https://unused.test"),
                Some("key"),
                Some("u:p"),
            )
            .transport(ForbiddenIo)
            .settings_store(ForbiddenIo),
        )
        .unwrap();
        assert!(!controller.enabled());
        controller.attach(Rc::new(|_| panic!("disabled task spawn")));
        controller.settings();
        controller
            .update_settings(AiDebugSettings::default())
            .unwrap();
        controller.register().await.unwrap();
        controller.resume().await.unwrap();
        controller.heartbeat().await.unwrap();
        controller.report_error("error", "stack").await;
        controller.add_log("message", Value::Null, "error");
        controller.flush_logs().await;
        controller.end().await;
        assert_eq!(controller.pending_logs(), 0);
    }
}
