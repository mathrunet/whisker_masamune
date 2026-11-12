use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_billing::{load_products, products, BillingMasamuneAdapter};
use whisker::runtime::module::{with_module_host, ModuleHost};
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};
use whisker::WhiskerValue;
use whisker::{spawn_local, Owner};

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

/// Store availability is optional at startup; invalid requests and bridge failures are not.
#[test]
fn unavailable_startup_can_reload_products_and_unexpected_errors_are_reported() {
    use masamune_whisker_billing::{NativeError, StoreFailure};
    use masamune_whisker_logger::{Flavor, LogEntry, LoggerConfig, RuntimeLoggerAdapter};
    use std::collections::VecDeque;

    let logger = RuntimeLoggerAdapter::default();
    LoggerConfig::new(Flavor::Dev)
        .adapter(logger.clone())
        .init()
        .unwrap();
    for (status, code, expected) in [
        ("billing_unavailable", 3, true),
        ("service_unavailable", 2, true),
        ("service_disconnected", -1, true),
        ("network_error", 12, true),
        ("not_supported", -2, true),
        ("developer_error", 5, false),
        ("unknown", 99, false),
    ] {
        logger.clear();
        let error = serde_json::json!({
            "billingError": 1, "operation": "connect", "status": status, "code": code,
        })
        .to_string();
        let failure = StoreFailure::from_native(&NativeError(error.clone())).unwrap();
        assert_eq!(failure.is_availability_failure(), expected);
        let replies = Rc::new(RefCell::new(VecDeque::from([
            WhiskerValue::Error(error),
            WhiskerValue::String(r#"{"value":[{"id":"monthly","price":"¥500"}]}"#.into()),
        ])));
        let host = ModuleHost::new(
            move |_, method, _, _, result| {
                assert_eq!(method, "queryProducts");
                result(replies.borrow_mut().pop_front().unwrap());
                true
            },
            |_, _, _| {},
        );
        let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
        runtime.enter(|| with_module_host(&host, || {
            let root = Owner::new(None);
            root.with(|| {
                MasamuneAdapters::new().adapter(BillingMasamuneAdapter::new(["monthly"])).install();
            });
            whisker::run_until_stalled();
            assert!(ids().is_empty());
            let reported = logger.entries().iter().any(|entry| matches!(entry,
                LogEntry::Error { context: Some(context), .. } if context.starts_with("masamune.adapter.start|")));
            assert_eq!(reported, !expected, "{status}");
            root.with(|| spawn_local(async {
                let loaded = load_products().await.unwrap();
                assert_eq!(loaded[0].price, "¥500");
            }));
            whisker::run_until_stalled();
            assert_eq!(ids(), ["monthly"]);
            root.dispose();
        }));
        runtime.shutdown();
    }
    for message in [
        "billing setup failed: 3 Billing service unavailable on device.",
        "no context",
        "{}",
        r#"{"billingError":2,"operation":"connect","status":"billing_unavailable","code":3}"#,
        r#"{"billingError":1,"status":"billing_unavailable","code":3}"#,
    ] {
        assert!(StoreFailure::from_native(&NativeError(message.into())).is_none());
    }
}
