use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_location::{LocationSettings, watch_position};
use whisker::Owner;
use whisker::WhiskerValue;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

type Calls = Rc<RefCell<Vec<(String, String)>>>;

fn run(body: impl FnOnce(Calls)) -> Vec<(String, String)> {
    let calls = Calls::default();
    let recorded = calls.clone();
    let host = ModuleHost::new(
        move |_, method, args, _, result| {
            let json = match args.first() {
                Some(WhiskerValue::String(json)) => json.clone(),
                _ => String::new(),
            };
            recorded.borrow_mut().push((method.to_string(), json));
            result(WhiskerValue::String(r#"{"value":true}"#.into()));
            true
        },
        |_, _, _| {},
    );
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        with_module_host(&host, || {
            let root = Owner::new(None);
            let inner = calls.clone();
            root.with(|| body(inner));
            root.dispose();
        })
    });
    runtime.shutdown();
    calls.take()
}

fn methods(calls: &[(String, String)]) -> Vec<&str> {
    calls.iter().map(|(method, _)| method.as_str()).collect()
}

#[test]
fn stops_when_the_last_watcher_is_dropped() {
    let calls = run(|calls| {
        let first = watch_position(&LocationSettings::default(), |_| {}).unwrap();
        let second = watch_position(
            &LocationSettings {
                distance_filter: f64::NAN,
                ..Default::default()
            },
            |_| {},
        )
        .unwrap();
        drop(first);
        assert_eq!(methods(&calls.borrow()), ["startUpdates", "startUpdates"]);
        drop(second);
    });
    assert_eq!(
        methods(&calls),
        ["startUpdates", "startUpdates", "stopUpdates"]
    );
    let settings: serde_json::Value = serde_json::from_str(&calls[1].1).unwrap();
    assert_eq!(settings["distanceFilter"], 0.0);
    assert_eq!(settings["background"], false);
}
