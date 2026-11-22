use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_local_notification::{
    ChannelStatus, InterruptionLevel, LocalNotificationMasamuneAdapter, NotificationChannel,
    NotificationConfig, NotificationImportance, NotificationOptions, NotificationSetting,
    NotificationTap, Schedule, can_use_full_screen_intent, channel_enabled, channel_status,
    create_channel, latest_tap, on_tap, schedule, schedule_daily, schedule_daily_with_options,
    schedule_daily_with_payload, schedule_once, schedule_once_with_options, take_pending_tap,
    time_sensitive_setting,
};
use serde_json::{Value, json};
use whisker::Owner;
use whisker::WhiskerValue;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

const MODULE: &str = "masamune-whisker-local-notification:LocalNotification";

type Calls = Rc<RefCell<Vec<(String, String)>>>;
type Observed = Rc<RefCell<Vec<(String, bool)>>>;

/// A mock host. Methods answer with the JSON in `responses` (`{"value":true}` when missing), and
/// `rearm` fails when `rearm_fails`.
#[derive(Default)]
struct Host {
    calls: Calls,
    observed: Observed,
    responses: HashMap<&'static str, String>,
    rearm_fails: bool,
}

impl Host {
    fn status(mut self, status: &str) -> Self {
        self.responses
            .insert("status", format!(r#"{{"value":"{status}"}}"#));
        self
    }

    fn respond(mut self, method: &'static str, json: &str) -> Self {
        self.responses.insert(method, json.into());
        self
    }

    fn build(&self) -> Rc<ModuleHost> {
        let calls = self.calls.clone();
        let observed = self.observed.clone();
        let responses = self.responses.clone();
        let rearm_fails = self.rearm_fails;
        ModuleHost::new(
            move |name, method, args, _, result| {
                let json = match args.first() {
                    Some(WhiskerValue::String(json)) => json.clone(),
                    _ => String::new(),
                };
                calls.borrow_mut().push((format!("{name}.{method}"), json));
                let value = match (method, responses.get(method)) {
                    ("rearm", _) if rearm_fails => WhiskerValue::Error("rearm failed".into()),
                    (_, Some(json)) => WhiskerValue::String(json.clone()),
                    _ => WhiskerValue::String(r#"{"value":true}"#.into()),
                };
                result(value);
                true
            },
            move |module, event, start| {
                observed
                    .borrow_mut()
                    .push((format!("{module}.{event}"), start));
            },
        )
    }
}

fn adapter() -> LocalNotificationMasamuneAdapter {
    LocalNotificationMasamuneAdapter::new(NotificationConfig {
        channel_id: "app_notification",
        channel_name: "App",
        channel_description: "App notifications",
        small_icon: None,
    })
}

/// Runs `body` inside a runtime bound to `host`.
fn with_host<T>(host: &Host, body: impl FnOnce(&Rc<ModuleHost>) -> T) -> T {
    let module_host = host.build();
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    let value = runtime.enter(|| with_module_host(&module_host, || body(&module_host)));
    runtime.shutdown();
    value
}

/// Installs `adapter`, runs its startup work, then runs `after` while the runtime is alive.
fn install<T>(
    host: &Host,
    adapter: LocalNotificationMasamuneAdapter,
    after: impl FnOnce(&Rc<ModuleHost>) -> T,
) -> T {
    with_host(host, |module_host| {
        let root = Owner::new(None);
        root.with(|| MasamuneAdapters::new().adapter(adapter).install());
        whisker::run_until_stalled();
        let value = after(module_host);
        root.dispose();
        value
    })
}

fn run(adapter: LocalNotificationMasamuneAdapter, host: Host) -> Vec<(String, String)> {
    install(&host, adapter, |_| ());
    host.calls.take()
}

fn methods(calls: &[(String, String)]) -> Vec<&str> {
    calls
        .iter()
        .map(|(method, _)| method.rsplit('.').next().unwrap_or_default())
        .collect()
}

fn tap(id: &str, payload: Option<&str>) -> NotificationTap {
    NotificationTap {
        id: id.into(),
        payload: payload.map(Into::into),
    }
}

#[test]
fn configures_then_rearms_when_allowed() {
    let calls = run(adapter(), Host::default().status("authorized"));
    assert_eq!(
        methods(&calls),
        ["configure", "status", "rearm", "getInitialTap"]
    );
    assert_eq!(calls[0].0, format!("{MODULE}.configure"));
    let config: Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(
        config,
        json!({
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
        methods(&run(adapter(), Host::default().status("denied"))),
        ["configure", "status", "getInitialTap"]
    );
    let disabled = LocalNotificationMasamuneAdapter {
        rearm_on_start: false,
        ..adapter()
    };
    assert_eq!(
        methods(&run(disabled, Host::default().status("authorized"))),
        ["configure", "getInitialTap"]
    );
}

#[test]
fn native_errors_do_not_panic() {
    let host = Host {
        rearm_fails: true,
        ..Host::default()
    }
    .status("authorized")
    .respond("getInitialTap", "not json");
    assert_eq!(
        methods(&run(adapter(), host)),
        ["configure", "status", "rearm", "getInitialTap"]
    );
}

#[test]
fn initial_tap_is_published_until_taken() {
    let host = Host::default().status("authorized").respond(
        "getInitialTap",
        r#"{"value":{"id":"night","payload":"/trail"}}"#,
    );
    let (latest, taken, again, after) = install(&host, adapter(), |_| {
        (
            latest_tap(),
            take_pending_tap(),
            take_pending_tap(),
            latest_tap(),
        )
    });
    assert_eq!(latest, Some(tap("night", Some("/trail"))));
    assert_eq!(taken, Some(tap("night", Some("/trail"))));
    assert_eq!(again, None);
    assert_eq!(after, None);
}

#[test]
fn no_initial_tap_leaves_latest_empty() {
    let host = Host::default().respond("getInitialTap", r#"{"value":null}"#);
    assert_eq!(install(&host, adapter(), |_| latest_tap()), None);
}

#[test]
fn tap_events_are_published() {
    let host = Host::default().respond("getInitialTap", r#"{"value":null}"#);
    let (first, second) = install(&host, adapter(), |module_host| {
        assert!(module_host.dispatch_event(
            MODULE,
            "tap",
            WhiskerValue::String(r#"{"id":"scene","payload":null}"#.into()),
        ));
        let first = take_pending_tap();
        module_host.dispatch_event(
            MODULE,
            "tap",
            WhiskerValue::String(r#"{"id":"night","payload":"{\"route\":\"/trail\"}"}"#.into()),
        );
        (first, latest_tap())
    });
    assert_eq!(first, Some(tap("scene", None)));
    assert_eq!(second, Some(tap("night", Some(r#"{"route":"/trail"}"#))));
    assert_eq!(
        host.observed.borrow().first(),
        Some(&(format!("{MODULE}.tap"), true))
    );
}

#[test]
fn installing_again_keeps_one_subscription() {
    let host = Host::default();
    with_host(&host, |_| {
        let root = Owner::new(None);
        root.with(|| MasamuneAdapters::new().adapter(adapter()).install());
        root.with(|| MasamuneAdapters::new().adapter(adapter()).install());
        whisker::run_until_stalled();
        root.dispose();
    });
    let starts = host
        .observed
        .borrow()
        .iter()
        .filter(|(_, start)| *start)
        .count();
    assert_eq!(starts, 1);
}

#[test]
fn on_tap_ignores_events_without_id() {
    let host = Host::default();
    let received = with_host(&host, |module_host| {
        let received = Rc::new(RefCell::new(Vec::new()));
        let sink = received.clone();
        let subscription = on_tap(move |tap| sink.borrow_mut().push(tap));
        module_host.dispatch_event(MODULE, "tap", WhiskerValue::String(r#"{"id":""}"#.into()));
        module_host.dispatch_event(MODULE, "tap", WhiskerValue::String("{}".into()));
        module_host.dispatch_event(
            MODULE,
            "tap",
            WhiskerValue::String(r#"{"id":"night"}"#.into()),
        );
        drop(subscription);
        module_host.dispatch_event(
            MODULE,
            "tap",
            WhiskerValue::String(r#"{"id":"late"}"#.into()),
        );
        received.take()
    });
    assert_eq!(received, [tap("night", None)]);
    assert_eq!(
        host.observed.borrow().as_slice(),
        [
            (format!("{MODULE}.tap"), true),
            (format!("{MODULE}.tap"), false)
        ]
    );
}

#[test]
fn schedule_once_sends_time_and_payload() {
    let host = Host::default().respond("scheduleOnce", r#"{"value":1795000000000}"#);
    let (with_payload, without) = with_host(&host, |_| {
        (
            schedule_once("scene", 1_795_000_000_000, "Title", "Text", Some("/scene")),
            schedule_once("night", 1_795_000_000_000, "Title", "Text", None),
        )
    });
    assert_eq!(with_payload, Ok(1_795_000_000_000));
    assert_eq!(without, Ok(1_795_000_000_000));
    let calls = host.calls.take();
    assert_eq!(calls[0].0, format!("{MODULE}.scheduleOnce"));
    assert_eq!(
        serde_json::from_str::<Value>(&calls[0].1).unwrap(),
        json!({
            "id": "scene",
            "atMs": 1_795_000_000_000_i64,
            "title": "Title",
            "text": "Text",
            "payload": "/scene",
        })
    );
    assert_eq!(
        serde_json::from_str::<Value>(&calls[1].1).unwrap()["payload"],
        Value::Null
    );
}

#[test]
fn schedule_once_reports_native_errors() {
    let failing = ModuleHost::new(
        |_, _, _, _, result| {
            result(WhiskerValue::Error("at_ms is in the past".into()));
            true
        },
        |_, _, _| {},
    );
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    let result = runtime.enter(|| {
        with_module_host(&failing, || {
            schedule_once("night", 0, "Title", "Text", None)
        })
    });
    runtime.shutdown();
    assert_eq!(
        result.unwrap_err().to_string(),
        "at_ms is in the past".to_string()
    );
}

#[test]
fn daily_schedules_send_optional_payload() {
    let host = Host::default().respond("scheduleDaily", r#"{"value":1795000000000}"#);
    with_host(&host, |_| {
        schedule_daily("night", 21, 30, "Title", "Text").unwrap();
        schedule_daily_with_payload("night", 21, 30, "Title", "Text", Some("/trail")).unwrap();
    });
    let calls = host.calls.take();
    assert_eq!(
        serde_json::from_str::<Value>(&calls[0].1).unwrap(),
        json!({
            "id": "night",
            "hour": 21,
            "minute": 30,
            "title": "Title",
            "text": "Text",
            "payload": null,
        })
    );
    assert_eq!(
        serde_json::from_str::<Value>(&calls[1].1).unwrap()["payload"],
        json!("/trail")
    );
}

#[test]
fn saved_schedules_read_old_and_once_entries() {
    let old = Host::default().respond(
        "schedule",
        r#"{"value":{"id":"night","hour":21,"minute":30,"title":"T","text":"X"}}"#,
    );
    assert_eq!(
        with_host(&old, |_| schedule("night")),
        Ok(Some(Schedule {
            id: "night".into(),
            hour: 21,
            minute: 30,
            title: "T".into(),
            text: "X".into(),
            payload: None,
            once: false,
            at_ms: None,
        }))
    );
    let once = Host::default().respond(
        "schedule",
        r#"{"value":{"id":"scene","once":true,"atMs":1795000000000,"hour":9,"minute":5,"payload":"/scene","title":"T","text":"X"}}"#,
    );
    let saved = with_host(&once, |_| schedule("scene")).unwrap().unwrap();
    assert!(saved.once);
    assert_eq!(saved.at_ms, Some(1_795_000_000_000));
    assert_eq!(saved.payload.as_deref(), Some("/scene"));
    let missing = Host::default().respond("schedule", r#"{"value":null}"#);
    assert_eq!(with_host(&missing, |_| schedule("x")), Ok(None));
}

#[test]
fn latest_tap_is_tracked_by_effects() {
    let host = Host::default().respond("getInitialTap", r#"{"value":null}"#);
    let seen = install(&host, adapter(), |module_host| {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let owner = Owner::new(None);
        owner.with(|| whisker::prelude::effect(move || sink.borrow_mut().push(latest_tap())));
        module_host.dispatch_event(
            MODULE,
            "tap",
            WhiskerValue::String(r#"{"id":"night"}"#.into()),
        );
        whisker::runtime::reactive::flush();
        owner.dispose();
        seen.take()
    });
    assert_eq!(seen, [None, Some(tap("night", None))]);
}

#[test]
fn adapter_creates_channels_before_configuring() {
    let adapter = adapter().channel(
        NotificationChannel::new("urgent", "Urgent")
            .description("Calls from the staff")
            .importance(NotificationImportance::High),
    );
    let calls = run(adapter, Host::default().status("denied"));
    assert_eq!(
        methods(&calls),
        ["createChannel", "configure", "status", "getInitialTap"]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&calls[0].1).unwrap(),
        json!({
            "id": "urgent",
            "name": "Urgent",
            "description": "Calls from the staff",
            "importance": "high",
        })
    );
}

#[test]
fn schedule_options_are_sent_only_when_set() {
    let host = Host::default()
        .respond("scheduleOnce", r#"{"value":1795000000000}"#)
        .respond("scheduleDaily", r#"{"value":1795000000000}"#);
    let urgent = NotificationOptions::new()
        .channel("urgent")
        .interruption_level(InterruptionLevel::TimeSensitive)
        .full_screen(true);
    with_host(&host, |_| {
        schedule_once_with_options("call", 1_795_000_000_000, "T", "X", Some("/call"), &urgent)
            .unwrap();
        schedule_daily_with_options(
            "quiet",
            7,
            0,
            "T",
            "X",
            None,
            &NotificationOptions::new().interruption_level(InterruptionLevel::Passive),
        )
        .unwrap();
        schedule_once_with_options(
            "plain",
            1_795_000_000_000,
            "T",
            "X",
            None,
            &NotificationOptions::default(),
        )
        .unwrap();
    });
    let calls = host.calls.take();
    assert_eq!(
        serde_json::from_str::<Value>(&calls[0].1).unwrap(),
        json!({
            "id": "call",
            "atMs": 1_795_000_000_000_i64,
            "title": "T",
            "text": "X",
            "payload": "/call",
            "channelId": "urgent",
            "interruptionLevel": "timeSensitive",
            "fullScreen": true,
        })
    );
    let daily = serde_json::from_str::<Value>(&calls[1].1).unwrap();
    assert_eq!(daily["interruptionLevel"], json!("passive"));
    assert!(daily.get("channelId").is_none() && daily.get("fullScreen").is_none());
    let plain = serde_json::from_str::<Value>(&calls[2].1).unwrap();
    assert_eq!(
        plain.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["id", "atMs", "title", "text", "payload"]
    );
}

#[test]
fn channel_status_reads_native_state() {
    let high = Host::default().respond(
        "channelStatus",
        r#"{"value":{"exists":true,"enabled":true,"importance":"high"}}"#,
    );
    assert_eq!(
        with_host(&high, |_| (
            channel_status("urgent"),
            channel_enabled("urgent")
        )),
        (
            Ok(Some(ChannelStatus {
                enabled: true,
                importance: NotificationImportance::High,
            })),
            true
        )
    );
    assert_eq!(
        serde_json::from_str::<Value>(&high.calls.take()[0].1).unwrap(),
        json!({ "id": "urgent" })
    );
    let blocked = Host::default().respond(
        "channelStatus",
        r#"{"value":{"exists":true,"enabled":false,"importance":"none"}}"#,
    );
    assert_eq!(
        with_host(&blocked, |_| channel_status("urgent")),
        Ok(Some(ChannelStatus {
            enabled: false,
            importance: NotificationImportance::None,
        }))
    );
    let missing = Host::default().respond("channelStatus", r#"{"value":{"exists":false}}"#);
    assert_eq!(with_host(&missing, |_| channel_status("x")), Ok(None));
    assert!(!with_host(&missing, |_| channel_enabled("x")));
}

#[test]
fn create_channel_defaults_to_default_importance() {
    let host = Host::default();
    with_host(&host, |_| {
        create_channel(&NotificationChannel::new("news", "News")).unwrap()
    });
    assert_eq!(
        serde_json::from_str::<Value>(&host.calls.take()[0].1).unwrap()["importance"],
        json!("default")
    );
}

#[test]
fn time_sensitive_setting_and_full_screen_queries() {
    for (answer, expected) in [
        (r#"{"value":"enabled"}"#, NotificationSetting::Enabled),
        (r#"{"value":"disabled"}"#, NotificationSetting::Disabled),
        (
            r#"{"value":"notSupported"}"#,
            NotificationSetting::NotSupported,
        ),
        ("not json", NotificationSetting::NotSupported),
    ] {
        let host = Host::default().respond("timeSensitiveSetting", answer);
        let seen = with_host(&host, |_| {
            let seen = Rc::new(RefCell::new(None));
            let sink = seen.clone();
            let root = Owner::new(None);
            root.with(|| {
                whisker::spawn_local(async move {
                    *sink.borrow_mut() = Some(time_sensitive_setting().await);
                })
            });
            whisker::run_until_stalled();
            root.dispose();
            seen.take()
        });
        assert_eq!(seen, Some(expected));
    }
    let allowed = Host::default().respond("canUseFullScreenIntent", r#"{"value":true}"#);
    assert!(with_host(&allowed, |_| can_use_full_screen_intent()));
    let broken = Host::default().respond("canUseFullScreenIntent", "not json");
    assert!(!with_host(&broken, |_| can_use_full_screen_intent()));
}
