use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_local_notification::{LocalNotificationMasamuneAdapter, NotificationConfig};
use whisker::Owner;
use whisker::WhiskerValue;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

type Calls = Rc<RefCell<Vec<(String, String)>>>;

/// A host answering `status` with `status` and failing `rearm` when `rearm_fails`.
fn host(calls: Calls, status: &'static str, rearm_fails: bool) -> Rc<ModuleHost> {
    ModuleHost::new(
        move |name, method, args, _, result| {
            let json = match args.first() {
                Some(WhiskerValue::String(json)) => json.clone(),
                _ => String::new(),
            };
            calls.borrow_mut().push((format!("{name}.{method}"), json));
            let value = match method {
                "status" => WhiskerValue::String(format!(r#"{{"value":"{status}"}}"#)),
                "rearm" if rearm_fails => WhiskerValue::Error("rearm failed".into()),
                _ => WhiskerValue::String(r#"{"value":true}"#.into()),
            };
            result(value);
            true
        },
        |_, _, _| {},
    )
}

fn adapter() -> LocalNotificationMasamuneAdapter {
    LocalNotificationMasamuneAdapter::new(NotificationConfig {
        channel_id: "app_notification",
        channel_name: "App",
        channel_description: "App notifications",
        small_icon: None,
    })
}

fn run(
    adapter: LocalNotificationMasamuneAdapter,
    status: &'static str,
    rearm_fails: bool,
) -> Vec<(String, String)> {
    let calls = Calls::default();
    let host = host(calls.clone(), status, rearm_fails);
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        with_module_host(&host, || {
            let root = Owner::new(None);
            root.with(|| MasamuneAdapters::new().adapter(adapter).install());
            whisker::run_until_stalled();
            root.dispose();
        })
    });
    runtime.shutdown();
    calls.take()
}

fn methods(calls: &[(String, String)]) -> Vec<&str> {
    calls
        .iter()
        .map(|(method, _)| method.rsplit('.').next().unwrap_or_default())
        .collect()
}

#[test]
fn configures_then_rearms_when_allowed() {
    let calls = run(adapter(), "authorized", false);
    assert_eq!(methods(&calls), ["configure", "status", "rearm"]);
    assert_eq!(
        calls[0].0,
        "masamune-whisker-local-notification:LocalNotification.configure"
    );
    let config: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(
        config,
        serde_json::json!({
            "channelId": "app_notification",
            "channelName": "App",
            "channelDescription": "App notifications",
            "smallIcon": "",
        })
    );
}

#[test]
fn does_not_rearm_when_denied_or_disabled() {
    assert_eq!(
        methods(&run(adapter(), "denied", false)),
        ["configure", "status"]
    );
    let disabled = LocalNotificationMasamuneAdapter {
        rearm_on_start: false,
        ..adapter()
    };
    assert_eq!(methods(&run(disabled, "authorized", false)), ["configure"]);
}

#[test]
fn native_errors_do_not_panic() {
    assert_eq!(
        methods(&run(adapter(), "authorized", true)),
        ["configure", "status", "rearm"]
    );
}
