use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_billing::{BillingMasamuneAdapter, load_products, products};
use whisker::WhiskerValue;
use whisker::runtime::module::{ModuleHost, with_module_host};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};
use whisker::{Owner, spawn_local};

type Calls = Rc<RefCell<Vec<(String, String)>>>;

/// A host whose store knows `yearly` and `monthly` (answered in that order).
fn host(calls: Calls) -> Rc<ModuleHost> {
    ModuleHost::new(
        move |_, method, args, _, result| {
            let json = match args.first() {
                Some(WhiskerValue::String(json)) => json.clone(),
                _ => String::new(),
            };
            calls.borrow_mut().push((method.to_owned(), json));
            let value = match method {
                "queryProducts" => r#"{"value":[{"id":"yearly"},{"id":"monthly"}]}"#,
                _ => r#"{"value":true}"#,
            };
            result(WhiskerValue::String(value.into()));
            true
        },
        |_, _, _| {},
    )
}

fn ids() -> Vec<String> {
    products()
        .get_untracked()
        .into_iter()
        .map(|product| product.id)
        .collect()
}

fn with_runtime(calls: &Calls, work: impl FnOnce()) {
    let host = host(calls.clone());
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| with_module_host(&host, work));
    runtime.shutdown();
}

#[test]
fn loads_the_products_in_the_configured_order_without_connecting() {
    let calls = Calls::default();
    with_runtime(&calls, || {
        let root = Owner::new(None);
        root.with(|| {
            MasamuneAdapters::new()
                .adapter(BillingMasamuneAdapter::new([
                    "monthly", "yearly", "unknown",
                ]))
                .install();
        });
        assert!(ids().is_empty());
        whisker::run_until_stalled();
        assert_eq!(ids(), ["monthly", "yearly"]);
        root.dispose();
    });
    let calls = calls.take();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, "queryProducts");
    let args: serde_json::Value = serde_json::from_str(&calls[0].1).unwrap();
    assert_eq!(
        args,
        serde_json::json!({ "ids": ["monthly", "yearly", "unknown"] })
    );
}

#[test]
fn does_not_load_when_disabled_and_fails_without_an_adapter() {
    let calls = Calls::default();
    with_runtime(&calls, || {
        let root = Owner::new(None);
        let failed = Rc::new(RefCell::new(None));
        root.with(|| {
            let failed = failed.clone();
            spawn_local(async move {
                *failed.borrow_mut() = Some(load_products().await.is_err());
            });
        });
        whisker::run_until_stalled();
        assert_eq!(*failed.borrow(), Some(true));
        root.with(|| {
            MasamuneAdapters::new()
                .adapter(BillingMasamuneAdapter {
                    load_on_start: false,
                    ..BillingMasamuneAdapter::new(["monthly"])
                })
                .install();
        });
        whisker::run_until_stalled();
        assert!(ids().is_empty());
        root.dispose();
    });
    assert!(calls.borrow().is_empty());
}
