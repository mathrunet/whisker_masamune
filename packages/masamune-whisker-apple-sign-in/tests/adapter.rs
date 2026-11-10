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

use masamune_whisker_apple_sign_in::{AppleSignInMasamuneAdapter, link, sign_in};

#[test]
fn requests_the_adapter_scopes() {
    let adapter = AppleSignInMasamuneAdapter {
        scopes: vec!["email".into()],
    };
    let calls = run(
        MasamuneAdapters::new().adapter(adapter),
        r#"{"uid":"u"}"#,
        || async {
            sign_in().await.unwrap();
            link().await.unwrap();
        },
    );
    assert_eq!(
        calls[0],
        ("signIn".into(), serde_json::json!({ "scopes": ["email"] }))
    );
    assert_eq!(
        calls[1],
        ("link".into(), serde_json::json!({ "scopes": ["email"] }))
    );
}

#[test]
fn requests_the_default_scopes_without_an_adapter() {
    let calls = run(MasamuneAdapters::new(), r#"{"uid":"u"}"#, || async {
        sign_in().await.unwrap();
    });
    assert_eq!(
        calls[0].1,
        serde_json::json!({ "scopes": ["email", "name"] })
    );
}
