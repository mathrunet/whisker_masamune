use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_wakelock::{Wakelock, disable, enable, is_enabled};
use whisker::Owner;
use whisker::WhiskerValue;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

type Calls = Rc<RefCell<Vec<(String, String)>>>;

/// Runs `body` against a fake host that records calls and answers `{"value": true}`.
fn run(body: impl FnOnce()) -> Vec<(String, String)> {
    let calls = Calls::default();
    let recorded = calls.clone();
    let host = ModuleHost::new(
        move |name, method, args, _, result| {
            let json = match args.first() {
                Some(WhiskerValue::String(json)) => json.clone(),
                _ => String::new(),
            };
            recorded
                .borrow_mut()
                .push((format!("{name}.{method}"), json));
            result(WhiskerValue::String(r#"{"value":true}"#.into()));
            true
        },
        |_, _, _| {},
    );
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        with_module_host(&host, || {
            let root = Owner::new(None);
            root.with(body);
            whisker::run_until_stalled();
            root.dispose();
        })
    });
    runtime.shutdown();
    calls.take()
}

#[test]
fn native_calls_use_the_wire_format() {
    let calls = run(|| {
        enable().unwrap();
        disable().unwrap();
        assert!(is_enabled().unwrap());
        assert!(Wakelock::native().set_enabled(true).unwrap());
    });
    let calls: Vec<(&str, serde_json::Value)> = calls
        .iter()
        .map(|(method, json)| (method.as_str(), serde_json::from_str(json).unwrap()))
        .collect();
    assert_eq!(
        calls,
        vec![
            (
                "masamune-whisker-wakelock:Wakelock.setEnabled",
                serde_json::json!({ "enabled": true })
            ),
            (
                "masamune-whisker-wakelock:Wakelock.setEnabled",
                serde_json::json!({ "enabled": false })
            ),
            (
                "masamune-whisker-wakelock:Wakelock.isEnabled",
                serde_json::json!({})
            ),
            (
                "masamune-whisker-wakelock:Wakelock.setEnabled",
                serde_json::json!({ "enabled": true })
            ),
        ]
    );
}
