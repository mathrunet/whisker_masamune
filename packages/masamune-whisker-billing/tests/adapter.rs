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

#[test]
fn consumable_apis_and_runtime_billing_adapter_work() {
    use masamune_whisker_billing::{
        Product, ProductKind, Purchase, PurchaseState, RuntimeBillingAdapter, consume,
        purchase_consumable, query_inapp_products, query_inapp_purchases,
    };

    let runtime = RuntimeBillingAdapter::new().with_products([
        Product {
            id: "single".into(),
            kind: ProductKind::Consumable,
            price: "¥980".into(),
            ..Default::default()
        },
        Product {
            id: "monthly".into(),
            kind: ProductKind::Subscription,
            price: "¥680".into(),
            ..Default::default()
        },
    ]);
    assert_eq!(
        runtime
            .query_products(&["single", "monthly"], Some(ProductKind::Consumable))
            .len(),
        1
    );
    let seen = Rc::new(RefCell::new(Vec::new()));
    let _sub = runtime.on_purchases({
        let seen = seen.clone();
        move |u| seen.borrow_mut().push(u)
    });
    assert!(
        runtime
            .purchase("single", None, None, None)
            .unwrap()
            .is_ok()
    );
    assert_eq!(seen.borrow().len(), 1);
    let purchased = runtime.query_purchases();
    assert_eq!(purchased.len(), 1);
    assert_eq!(purchased[0].state, PurchaseState::Purchased);
    runtime.consume(&purchased[0]);
    assert!(runtime.query_purchases().is_empty());
    assert_eq!(runtime.consumed_tokens().len(), 1);

    let calls = Calls::default();
    let host = ModuleHost::new(
        {
            let calls = calls.clone();
            move |_, method, args, _, result| {
                let json = match args.first() {
                    Some(WhiskerValue::String(json)) => json.clone(),
                    _ => String::new(),
                };
                calls.borrow_mut().push((method.to_owned(), json));
                let reply = match method {
                    "queryInAppProducts" => {
                        r#"{"value":[{"id":"single","kind":"consumable","price":"¥980"}]}"#
                    }
                    "queryInAppPurchases" => {
                        r#"{"value":[{"productId":"single","transactionId":"t1","token":"tok1","state":"purchased"}]}"#
                    }
                    "purchase" => r#"{"value":{"status":"ok","code":0,"message":""}}"#,
                    _ => r#"{"value":null}"#,
                };
                result(WhiskerValue::String(reply.into()));
                true
            }
        },
        |_, _, _| {},
    );
    let rt = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    rt.enter(|| {
        with_module_host(&host, || {
            let root = Owner::new(None);
            root.with(|| {
                spawn_local(async {
                    let products = query_inapp_products(&["single"]).await.unwrap();
                    assert_eq!(products[0].kind, ProductKind::Consumable);
                    assert!(purchase_consumable("single", None).await.unwrap().is_ok());
                    let purchases = query_inapp_purchases().await.unwrap();
                    assert_eq!(purchases.len(), 1);
                    consume(&Purchase {
                        token: "tok1".into(),
                        transaction_id: "t1".into(),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
                });
            });
            whisker::run_until_stalled();
            root.dispose();
        });
    });
    rt.shutdown();
    let recorded = calls.borrow();
    let methods: Vec<&str> = recorded.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(
        methods,
        [
            "queryInAppProducts",
            "purchase",
            "queryInAppPurchases",
            "consume"
        ]
    );
}
