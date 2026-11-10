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

use masamune_whisker_firebase_app_check::FirebaseAppCheckMasamuneAdapter;

#[test]
fn activates_while_preparing() {
    let adapter = FirebaseAppCheckMasamuneAdapter {
        debug_token: Some("token".into()),
    };
    let calls = run(MasamuneAdapters::new().adapter(adapter), "true", || async {
    });
    assert_eq!(
        calls,
        [(
            "activate".into(),
            serde_json::json!({ "debugToken": "token" })
        )]
    );
}
