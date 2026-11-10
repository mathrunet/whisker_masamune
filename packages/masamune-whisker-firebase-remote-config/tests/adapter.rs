use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapters;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};
use whisker::{Owner, WhiskerValue, spawn_local};

type Calls = Rc<RefCell<Vec<(String, serde_json::Value)>>>;

/// Installs `adapters`, runs `work` to completion and returns the native calls (method, args).
fn run<F>(
    adapters: MasamuneAdapters,
    answer: &'static str,
    work: impl FnOnce() -> F,
) -> Vec<(String, serde_json::Value)>
where
    F: std::future::Future<Output = ()> + 'static,
{
    let calls = Calls::default();
    let recorded = calls.clone();
    let host = ModuleHost::new(
        move |_, method, args, _, result| {
            let args = match args.first() {
                Some(WhiskerValue::String(json)) => serde_json::from_str(json).unwrap_or_default(),
                _ => serde_json::Value::Null,
            };
            recorded.borrow_mut().push((method.to_owned(), args));
            result(WhiskerValue::String(format!(r#"{{"value":{answer}}}"#)));
            true
        },
        |_, _, _| {},
    );
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        with_module_host(&host, || {
            let root = Owner::new(None);
            root.with(|| {
                adapters.install();
                spawn_local(work());
            });
            whisker::run_until_stalled();
            root.dispose();
        })
    });
    runtime.shutdown();
    calls.take()
}

use std::time::Duration;

use masamune_whisker_firebase_remote_config::{FirebaseRemoteConfigMasamuneAdapter, fetch};

#[test]
fn fetches_with_the_adapter_intervals() {
    let adapter = FirebaseRemoteConfigMasamuneAdapter {
        fetch_timeout: Duration::from_secs(3),
        minimum_fetch_interval: Duration::from_secs(3600),
        fetch_on_start: false,
    };
    let calls = run(
        MasamuneAdapters::new().adapter(adapter),
        r#"{"version":"2"}"#,
        || async {
            assert_eq!(
                fetch().await.unwrap().get("version").map(String::as_str),
                Some("2")
            );
        },
    );
    assert_eq!(
        calls,
        [(
            "fetchAndActivate".into(),
            serde_json::json!({ "fetchTimeoutSeconds": 3, "minimumFetchIntervalSeconds": 3600 })
        )]
    );
}

#[test]
fn fetches_on_start_only_when_asked() {
    assert!(
        run(
            MasamuneAdapters::new().adapter(FirebaseRemoteConfigMasamuneAdapter::default()),
            "{}",
            || async {}
        )
        .is_empty()
    );
    let adapter = FirebaseRemoteConfigMasamuneAdapter {
        fetch_on_start: true,
        ..Default::default()
    };
    let calls = run(MasamuneAdapters::new().adapter(adapter), "{}", || async {});
    assert_eq!(calls[0].0, "fetchAndActivate");
}
