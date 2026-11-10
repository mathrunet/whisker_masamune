use masamune_whisker_adapter::{MasamuneAdapter, MasamuneAdapters};
use masamune_whisker_webview::{
    DEFAULT_MAX_DOWNLOAD_BYTES, WebViewDefaults, WebViewMasamuneAdapter, max_download_bytes,
    webview_defaults,
};
use whisker::Owner;
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

fn in_runtime(work: impl FnOnce()) {
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        let root = Owner::new(None);
        root.with(work);
        root.dispose();
    });
    runtime.shutdown();
}

fn configured() -> WebViewMasamuneAdapter {
    WebViewMasamuneAdapter::new()
        .user_agent("TestAgent/1.0")
        .allowed_origins([
            "HTTPS://Host.Example:443/__samurai/",
            "https://host.example",
        ])
        .grant_media_permissions(true)
        .max_download_bytes(1024)
}

#[test]
fn builder_normalizes_and_exposes_values() {
    let adapter = configured();
    assert_eq!(adapter.get_user_agent(), Some("TestAgent/1.0"));
    assert_eq!(adapter.get_allowed_origins(), ["https://host.example"]);
    assert!(adapter.get_grant_media_permissions());
    assert_eq!(adapter.get_max_download_bytes(), 1024);

    let plain = WebViewMasamuneAdapter::new().user_agent("");
    assert_eq!(plain.get_user_agent(), None);
    assert!(plain.get_allowed_origins().is_empty());
    assert!(!plain.get_grant_media_permissions());
    assert_eq!(plain.get_max_download_bytes(), DEFAULT_MAX_DOWNLOAD_BYTES);
}

#[test]
fn installed_adapter_provides_the_defaults() {
    in_runtime(|| {
        assert_eq!(webview_defaults(), WebViewDefaults::default());

        MasamuneAdapters::new().adapter(configured()).install();
        let expected = WebViewDefaults {
            user_agent: "TestAgent/1.0".into(),
            allowed_origins: vec!["https://host.example".into()],
            grant_media_permissions: true,
            max_download_bytes: 1024,
        };
        assert_eq!(webview_defaults(), expected);
        assert_eq!(max_download_bytes(), 1024);

        // prepare is idempotent.
        let primary = WebViewMasamuneAdapter::primary().unwrap();
        primary.prepare().unwrap();
        primary.prepare().unwrap();
        assert_eq!(webview_defaults(), expected);

        // Installing again without the adapter falls back to the built-in defaults.
        MasamuneAdapters::new().install();
        assert_eq!(webview_defaults(), WebViewDefaults::default());
        assert_eq!(max_download_bytes(), DEFAULT_MAX_DOWNLOAD_BYTES);
    });
}
