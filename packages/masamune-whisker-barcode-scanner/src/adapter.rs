use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use masamune_whisker_adapter::MasamuneAdapter;
use serde_json::json;

use crate::{CameraPermission, NativeResult, Scan, ScannerError, module};

/// Result of [`BarcodeScannerAdapter::request_permission`].
pub type PermissionFuture<'a> = Pin<Box<dyn Future<Output = NativeResult<CameraPermission>> + 'a>>;

/// Where the camera permission and the scans come from.
///
/// [`NativeBarcodeScannerAdapter`] (the default) uses the device camera.
/// [`RuntimeBarcodeScannerAdapter`] replaces the camera with scans fed from Rust, for unit tests
/// and the `test` flavor (E2E tests on simulators without a camera).
pub trait BarcodeScannerAdapter: 'static {
    /// The current camera permission.
    fn permission_status(&self) -> CameraPermission;

    /// Asks for the camera permission when it has not been decided yet, and returns the result.
    /// An already decided permission is returned without a prompt.
    fn request_permission(&self) -> PermissionFuture<'_>;

    /// Opens the app's page in the system settings (where a denied permission can be changed).
    fn open_settings(&self) -> NativeResult<bool>;

    /// Scans delivered instead of the camera. `None` (the default) makes
    /// [`crate::BarcodeScanner`] show the native camera preview.
    fn feed(&self) -> Option<ScanFeed> {
        None
    }
}

/// The device camera (`AVCaptureSession` on iOS, CameraX on Android).
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeBarcodeScannerAdapter;

impl BarcodeScannerAdapter for NativeBarcodeScannerAdapter {
    fn permission_status(&self) -> CameraPermission {
        module()
            .call::<_, String>("permissionStatus", &json!({}))
            .map(|status| CameraPermission::from_native(&status))
            .unwrap_or(CameraPermission::Denied)
    }

    fn request_permission(&self) -> PermissionFuture<'_> {
        Box::pin(async {
            module()
                .call_async::<_, String>("requestPermission", &json!({}))
                .await
                .map(|status| CameraPermission::from_native(&status))
        })
    }

    fn open_settings(&self) -> NativeResult<bool> {
        module().call("openSettings", &json!({}))
    }
}

/// An event of a [`ScanFeed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedEvent {
    /// A decoded code.
    Scan(Scan),
    /// A camera failure.
    Error(ScannerError),
}

type Listener = Rc<dyn Fn(&FeedEvent) -> bool>;

#[derive(Default)]
struct FeedState {
    next_id: u64,
    listeners: Vec<(u64, Listener)>,
    queued: VecDeque<Scan>,
}

/// Scans pushed from Rust to the mounted [`crate::BarcodeScanner`] elements.
///
/// A scan goes to every scanner that is active (with the permission granted); when none takes
/// it, it is queued and delivered when a scanner becomes active. Errors are not queued. The
/// handle is cheap to clone and shares its state.
#[derive(Clone, Default)]
pub struct ScanFeed {
    state: Rc<RefCell<FeedState>>,
}

impl std::fmt::Debug for ScanFeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.state.borrow();
        f.debug_struct("ScanFeed")
            .field("listeners", &state.listeners.len())
            .field("queued", &state.queued.len())
            .finish()
    }
}

impl ScanFeed {
    /// An empty feed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Delivers `event`. Returns whether a listener took it (a scan nobody took is queued).
    pub fn push(&self, event: FeedEvent) -> bool {
        let taken = self.dispatch(&event);
        if !taken && let FeedEvent::Scan(scan) = event {
            self.state.borrow_mut().queued.push_back(scan);
        }
        taken
    }

    /// Hands queued scans to the listeners, stopping at the first one nobody takes.
    pub fn flush(&self) {
        loop {
            let Some(scan) = self.state.borrow_mut().queued.pop_front() else {
                return;
            };
            if !self.dispatch(&FeedEvent::Scan(scan.clone())) {
                self.state.borrow_mut().queued.push_front(scan);
                return;
            }
        }
    }

    /// Number of scans waiting for an active scanner.
    pub fn queued(&self) -> usize {
        self.state.borrow().queued.len()
    }

    /// Registers `listener`, which returns whether it took the event. It stays registered while
    /// the returned [`FeedListener`] is held.
    pub fn listen(&self, listener: impl Fn(&FeedEvent) -> bool + 'static) -> FeedListener {
        let mut state = self.state.borrow_mut();
        let id = state.next_id;
        state.next_id += 1;
        state.listeners.push((id, Rc::new(listener)));
        FeedListener {
            feed: self.clone(),
            id,
        }
    }

    fn dispatch(&self, event: &FeedEvent) -> bool {
        // Listeners run without the borrow, so they may push or listen themselves.
        let listeners: Vec<Listener> = self
            .state
            .borrow()
            .listeners
            .iter()
            .map(|(_, listener)| listener.clone())
            .collect();
        let mut taken = false;
        for listener in listeners {
            taken |= listener(event);
        }
        taken
    }
}

/// Registration of a [`ScanFeed`] listener. Dropping it removes the listener.
pub struct FeedListener {
    feed: ScanFeed,
    id: u64,
}

impl Drop for FeedListener {
    fn drop(&mut self) {
        if let Ok(mut state) = self.feed.state.try_borrow_mut() {
            state.listeners.retain(|(id, _)| *id != self.id);
        }
    }
}

struct RuntimeState {
    permission: CameraPermission,
    grant_on_request: bool,
    permission_requests: usize,
    settings_opened: usize,
}

/// A camera-less scanner for tests and the `test` flavor.
///
/// [`crate::BarcodeScanner`] shows an empty box instead of the camera and reports the codes
/// passed to [`scan`](Self::scan). The permission starts as granted; set another state with
/// [`with_permission`](Self::with_permission) to test the denied and first-run paths. Clones share
/// their state, so keep one to feed scans after registering the adapter.
///
/// ```
/// use masamune_whisker_barcode_scanner::{CameraPermission, RuntimeBarcodeScannerAdapter};
///
/// let scanner = RuntimeBarcodeScannerAdapter::new()
///     .with_permission(CameraPermission::NotDetermined)
///     .grant_on_request(true);
/// // Register `scanner.clone()` with `BarcodeScannerMasamuneAdapter::with_adapter`, then:
/// scanner.scan("https://example.com/ticket/1");
/// ```
#[derive(Clone)]
pub struct RuntimeBarcodeScannerAdapter {
    state: Rc<RefCell<RuntimeState>>,
    feed: ScanFeed,
}

impl Default for RuntimeBarcodeScannerAdapter {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(RuntimeState {
                permission: CameraPermission::Authorized,
                grant_on_request: true,
                permission_requests: 0,
                settings_opened: 0,
            })),
            feed: ScanFeed::new(),
        }
    }
}

impl std::fmt::Debug for RuntimeBarcodeScannerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeBarcodeScannerAdapter")
            .field("permission", &self.state.borrow().permission)
            .field("feed", &self.feed)
            .finish()
    }
}

impl RuntimeBarcodeScannerAdapter {
    /// Permission granted, no scans.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts from `permission`.
    pub fn with_permission(self, permission: CameraPermission) -> Self {
        self.set_permission(permission);
        self
    }

    /// What a request answers while the permission is [`CameraPermission::NotDetermined`]:
    /// granted (`true`, the default) or denied.
    pub fn grant_on_request(self, grant: bool) -> Self {
        self.state.borrow_mut().grant_on_request = grant;
        self
    }

    /// Changes the permission (for example after the user went to the settings).
    pub fn set_permission(&self, permission: CameraPermission) {
        self.state.borrow_mut().permission = permission;
    }

    /// Reports a QR code with `value`.
    pub fn scan(&self, value: impl Into<String>) -> bool {
        self.push_scan(Scan::qr(value))
    }

    /// Reports `scan`. Returns whether an active scanner took it; otherwise it is queued.
    pub fn push_scan(&self, scan: Scan) -> bool {
        self.feed.push(FeedEvent::Scan(scan))
    }

    /// Reports a camera failure to the mounted scanners.
    pub fn fail(&self, error: ScannerError) -> bool {
        self.feed.push(FeedEvent::Error(error))
    }

    /// The feed that replaces the camera.
    pub fn scan_feed(&self) -> ScanFeed {
        self.feed.clone()
    }

    /// How many times the permission was requested.
    pub fn permission_requests(&self) -> usize {
        self.state.borrow().permission_requests
    }

    /// How many times the settings were opened.
    pub fn settings_opened(&self) -> usize {
        self.state.borrow().settings_opened
    }
}

impl BarcodeScannerAdapter for RuntimeBarcodeScannerAdapter {
    fn permission_status(&self) -> CameraPermission {
        self.state.borrow().permission
    }

    fn request_permission(&self) -> PermissionFuture<'_> {
        let mut state = self.state.borrow_mut();
        state.permission_requests += 1;
        if state.permission == CameraPermission::NotDetermined {
            state.permission = if state.grant_on_request {
                CameraPermission::Authorized
            } else {
                CameraPermission::Denied
            };
        }
        let permission = state.permission;
        Box::pin(async move { Ok(permission) })
    }

    fn open_settings(&self) -> NativeResult<bool> {
        self.state.borrow_mut().settings_opened += 1;
        Ok(true)
    }

    fn feed(&self) -> Option<ScanFeed> {
        Some(self.feed.clone())
    }
}

/// Default window of the duplicate filter (see [`crate::ScanDebouncer`]).
pub const DEFAULT_DEBOUNCE: Duration = Duration::from_millis(1500);

/// Scanner settings for [`masamune_whisker_adapter::MasamuneAdapters`]: the
/// [`BarcodeScannerAdapter`] behind [`crate::BarcodeScanner`] and the permission functions, and
/// the default duplicate window.
///
/// Without an installed adapter the device camera and [`DEFAULT_DEBOUNCE`] are used.
///
/// ```ignore
/// let adapter = if flavor == Flavor::Test {
///     BarcodeScannerMasamuneAdapter::with_adapter(RuntimeBarcodeScannerAdapter::new())
/// } else {
///     BarcodeScannerMasamuneAdapter::new()
/// };
/// MasamuneAdapters::new().adapter(adapter.debounce(Duration::from_secs(2))).install();
/// ```
#[derive(Clone)]
pub struct BarcodeScannerMasamuneAdapter {
    adapter: Rc<dyn BarcodeScannerAdapter>,
    debounce: Duration,
}

impl std::fmt::Debug for BarcodeScannerMasamuneAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BarcodeScannerMasamuneAdapter")
            .field("debounce", &self.debounce)
            .finish_non_exhaustive()
    }
}

impl Default for BarcodeScannerMasamuneAdapter {
    fn default() -> Self {
        Self::with_adapter(NativeBarcodeScannerAdapter)
    }
}

impl BarcodeScannerMasamuneAdapter {
    /// The device camera with [`DEFAULT_DEBOUNCE`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses `adapter` (for example [`RuntimeBarcodeScannerAdapter`]) with [`DEFAULT_DEBOUNCE`].
    pub fn with_adapter(adapter: impl BarcodeScannerAdapter) -> Self {
        Self {
            adapter: Rc::new(adapter),
            debounce: DEFAULT_DEBOUNCE,
        }
    }

    /// Window in which the same code is reported once (zero reports every detection).
    pub fn debounce(mut self, debounce: Duration) -> Self {
        self.debounce = debounce;
        self
    }

    /// The configured adapter.
    pub fn adapter(&self) -> Rc<dyn BarcodeScannerAdapter> {
        self.adapter.clone()
    }

    /// The configured duplicate window.
    pub fn get_debounce(&self) -> Duration {
        self.debounce
    }
}

impl MasamuneAdapter for BarcodeScannerMasamuneAdapter {}

/// The adapter of the installed [`BarcodeScannerMasamuneAdapter`], or the device camera.
pub fn barcode_scanner_adapter() -> Rc<dyn BarcodeScannerAdapter> {
    BarcodeScannerMasamuneAdapter::primary()
        .map(|installed| installed.adapter())
        .unwrap_or_else(|| Rc::new(NativeBarcodeScannerAdapter))
}

/// The duplicate window of the installed [`BarcodeScannerMasamuneAdapter`], or
/// [`DEFAULT_DEBOUNCE`].
pub fn default_debounce() -> Duration {
    BarcodeScannerMasamuneAdapter::primary()
        .map_or(DEFAULT_DEBOUNCE, |installed| installed.debounce)
}

/// The current camera permission (through [`barcode_scanner_adapter`]).
pub fn camera_permission() -> CameraPermission {
    barcode_scanner_adapter().permission_status()
}

/// Requests the camera permission (through [`barcode_scanner_adapter`]). Shows the system prompt
/// only while the permission is [`CameraPermission::NotDetermined`].
pub async fn request_camera_permission() -> NativeResult<CameraPermission> {
    let adapter = barcode_scanner_adapter();
    adapter.request_permission().await
}

/// Opens the app's page in the system settings (through [`barcode_scanner_adapter`]).
pub fn open_camera_settings() -> NativeResult<bool> {
    barcode_scanner_adapter().open_settings()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn feed_queues_until_a_listener_takes_scans() {
        let feed = ScanFeed::new();
        assert!(!feed.push(FeedEvent::Scan(Scan::qr("a"))));
        assert!(!feed.push(FeedEvent::Error(ScannerError::unavailable())));
        assert_eq!(feed.queued(), 1);

        let accepting = Rc::new(Cell::new(false));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let listener = {
            let accepting = accepting.clone();
            let seen = seen.clone();
            feed.listen(move |event| {
                if !accepting.get() {
                    return false;
                }
                seen.borrow_mut().push(event.clone());
                true
            })
        };
        feed.flush();
        assert_eq!(feed.queued(), 1);
        accepting.set(true);
        feed.flush();
        assert_eq!(feed.queued(), 0);
        assert!(feed.push(FeedEvent::Scan(Scan::qr("b"))));
        assert_eq!(
            *seen.borrow(),
            [
                FeedEvent::Scan(Scan::qr("a")),
                FeedEvent::Scan(Scan::qr("b"))
            ]
        );

        drop(listener);
        assert!(!feed.push(FeedEvent::Scan(Scan::qr("c"))));
        assert_eq!(feed.queued(), 1);
    }

    #[test]
    fn runtime_permission_requests() {
        let denied = RuntimeBarcodeScannerAdapter::new()
            .with_permission(CameraPermission::NotDetermined)
            .grant_on_request(false);
        let result = block_on(denied.request_permission());
        assert_eq!(result, Ok(CameraPermission::Denied));
        assert_eq!(denied.permission_status(), CameraPermission::Denied);
        // A decided permission is returned as is.
        denied.set_permission(CameraPermission::Restricted);
        let result = block_on(denied.request_permission());
        assert_eq!(result, Ok(CameraPermission::Restricted));
        assert_eq!(denied.permission_requests(), 2);
        assert_eq!(denied.open_settings(), Ok(true));
        assert_eq!(denied.settings_opened(), 1);
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("runtime adapter futures resolve immediately"),
        }
    }
}
