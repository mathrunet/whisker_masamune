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

use masamune_whisker_admob::{
    AdmobMasamuneAdapter, default_banner_ad_unit_id, show_default_interstitial,
};

#[test]
fn uses_the_default_ad_units() {
    let adapter = AdmobMasamuneAdapter {
        banner_ad_unit_id: Some("banner".into()),
        interstitial_ad_unit_id: Some("interstitial".into()),
        ..Default::default()
    };
    let banner = Rc::new(RefCell::new(None));
    let seen = banner.clone();
    let calls = run(
        MasamuneAdapters::new().adapter(adapter),
        "true",
        move || async move {
            *seen.borrow_mut() = default_banner_ad_unit_id();
            assert!(show_default_interstitial().await.unwrap());
        },
    );
    assert_eq!(banner.borrow().as_deref(), Some("banner"));
    assert_eq!(
        calls,
        [(
            "showInterstitial".into(),
            serde_json::json!({ "adUnitId": "interstitial" })
        )]
    );
}

#[test]
fn requests_tracking_on_start_only_when_asked() {
    let quiet = run(
        MasamuneAdapters::new().adapter(AdmobMasamuneAdapter::default()),
        r#""authorized""#,
        || async {},
    );
    assert!(quiet.is_empty());
    let asking = AdmobMasamuneAdapter {
        request_tracking_on_start: true,
        ..Default::default()
    };
    let calls = run(
        MasamuneAdapters::new().adapter(asking),
        r#""authorized""#,
        || async {},
    );
    assert_eq!(calls[0].0, "requestTrackingAuthorization");
}
