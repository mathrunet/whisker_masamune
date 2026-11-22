//! `BarcodeScanner` with `RuntimeBarcodeScannerAdapter`: permission flow, fed scans, the
//! duplicate filter and start / stop, without a camera.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_barcode_scanner::{
    BarcodeScanner, BarcodeScannerAdapter, BarcodeScannerMasamuneAdapter, CameraPermission,
    DEFAULT_DEBOUNCE, RuntimeBarcodeScannerAdapter, Scan, ScannerError, camera_permission,
    default_debounce, open_camera_settings, request_camera_permission,
};
use whisker::prelude::*;
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};
use whisker::{Owner, spawn_local};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    Scan(String),
    Permission(CameraPermission),
    Error(String),
}

type Log = Rc<RefCell<Vec<Seen>>>;

/// Installs `adapter`, mounts a scanner and runs `body` with its `active` signal and the log.
fn with_scanner(
    adapter: BarcodeScannerMasamuneAdapter,
    request_permission: bool,
    body: impl FnOnce(RwSignal<bool>, &Log),
) {
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        let root = Owner::new(None);
        let log = Log::default();
        let active = root.with(|| {
            MasamuneAdapters::new().adapter(adapter).install();
            let active = RwSignal::new(true);
            let (scans, permissions, errors) = (log.clone(), log.clone(), log.clone());
            let _scanner: Element = render! {
                BarcodeScanner(
                    active: active,
                    request_permission: request_permission,
                    identifier: "scanner".to_string(),
                    label: "QR scanner".to_string(),
                    on_scan: move |scan: Scan| scans.borrow_mut().push(Seen::Scan(scan.value)),
                    on_permission: move |status: CameraPermission| {
                        permissions.borrow_mut().push(Seen::Permission(status))
                    },
                    on_error: move |error: ScannerError| {
                        errors.borrow_mut().push(Seen::Error(error.code))
                    },
                )
            };
            active
        });
        whisker::run_until_stalled();
        body(active, &log);
        root.dispose();
    });
    runtime.shutdown();
}

/// Runs the effects scheduled by signal writes, then the tasks they spawned (the Host does this
/// every frame).
fn settle() {
    whisker::runtime::reactive::flush();
    whisker::run_until_stalled();
}

#[test]
fn reports_fed_scans_once_per_code() {
    let scanner = RuntimeBarcodeScannerAdapter::new();
    let feed = scanner.clone();
    with_scanner(
        BarcodeScannerMasamuneAdapter::with_adapter(scanner).debounce(Duration::from_secs(60)),
        true,
        |_, log| {
            assert!(feed.scan("a"));
            assert!(feed.scan("a"));
            assert!(feed.scan("b"));
            assert!(feed.fail(ScannerError::unavailable()));
            assert_eq!(
                *log.borrow(),
                [
                    Seen::Permission(CameraPermission::Authorized),
                    Seen::Scan("a".into()),
                    Seen::Scan("b".into()),
                    Seen::Error("unavailable".into()),
                ]
            );
        },
    );
    // Unmounted: scans wait in the queue.
    assert!(!feed.scan("c"));
    assert_eq!(feed.scan_feed().queued(), 1);
}

#[test]
fn stopping_queues_scans_and_restarting_forgets_the_last_code() {
    let scanner = RuntimeBarcodeScannerAdapter::new();
    let feed = scanner.clone();
    with_scanner(
        BarcodeScannerMasamuneAdapter::with_adapter(scanner).debounce(Duration::from_secs(60)),
        true,
        |active, log| {
            assert!(feed.scan("a"));
            active.set(false);
            settle();
            assert!(!feed.scan("a"));
            active.set(true);
            settle();
            // The queued scan is delivered on restart, and is not a duplicate any more.
            assert_eq!(feed.scan_feed().queued(), 0);
            assert_eq!(
                *log.borrow(),
                [
                    Seen::Permission(CameraPermission::Authorized),
                    Seen::Scan("a".into()),
                    Seen::Permission(CameraPermission::Authorized),
                    Seen::Scan("a".into()),
                ]
            );
        },
    );
}

#[test]
fn requests_an_undecided_permission() {
    let scanner = RuntimeBarcodeScannerAdapter::new()
        .with_permission(CameraPermission::NotDetermined)
        .grant_on_request(false);
    let feed = scanner.clone();
    with_scanner(
        BarcodeScannerMasamuneAdapter::with_adapter(scanner),
        true,
        |_, log| {
            assert!(!feed.scan("a"));
            assert_eq!(*log.borrow(), [Seen::Permission(CameraPermission::Denied)]);
        },
    );
    assert_eq!(feed.permission_requests(), 1);
}

#[test]
fn leaves_the_permission_alone_when_asked_to() {
    let scanner =
        RuntimeBarcodeScannerAdapter::new().with_permission(CameraPermission::NotDetermined);
    let feed = scanner.clone();
    with_scanner(
        BarcodeScannerMasamuneAdapter::with_adapter(scanner),
        false,
        |_, log| {
            assert_eq!(
                *log.borrow(),
                [Seen::Permission(CameraPermission::NotDetermined)]
            );
            assert!(!feed.scan("a"));
        },
    );
    assert_eq!(feed.permission_requests(), 0);
}

#[test]
fn permission_functions_use_the_installed_adapter() {
    let scanner =
        RuntimeBarcodeScannerAdapter::new().with_permission(CameraPermission::NotDetermined);
    let state = scanner.clone();
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        let root = Owner::new(None);
        let result = Rc::new(RefCell::new(None));
        root.with(|| {
            assert_eq!(default_debounce(), DEFAULT_DEBOUNCE);
            MasamuneAdapters::new()
                .adapter(
                    BarcodeScannerMasamuneAdapter::with_adapter(scanner)
                        .debounce(Duration::from_millis(500)),
                )
                .install();
            assert_eq!(default_debounce(), Duration::from_millis(500));
            assert_eq!(camera_permission(), CameraPermission::NotDetermined);
            assert_eq!(open_camera_settings(), Ok(true));
            let result = result.clone();
            spawn_local(async move {
                *result.borrow_mut() = Some(request_camera_permission().await);
            });
        });
        whisker::run_until_stalled();
        assert_eq!(*result.borrow(), Some(Ok(CameraPermission::Authorized)));
        root.dispose();
    });
    runtime.shutdown();
    assert_eq!(state.settings_opened(), 1);
    assert_eq!(state.permission_status(), CameraPermission::Authorized);
}
