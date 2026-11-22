use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use whisker::prelude::*;
use whisker::{Accessibility, ElementBuilder, Style};

use crate::adapter::{FeedEvent, ScanFeed, barcode_scanner_adapter, default_debounce};
use crate::{BarcodeFormat, CameraPermission, Scan, ScanDebouncer, ScannerError};

// Every native event carries its body under `detail`. Fields default so that a partial body
// still reaches the handler.

/// Body of the `scan` event.
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ScanEvent {
    #[serde(default)]
    pub detail: ScanDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ScanDetail {
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub format: String,
}

/// Body of the `permission` event.
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PermissionEvent {
    #[serde(default)]
    pub detail: PermissionDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PermissionDetail {
    #[serde(default)]
    pub status: String,
}

/// Body of the `error` event.
#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ErrorEvent {
    #[serde(default)]
    pub detail: ErrorDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ErrorDetail {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

// Configuration is declared before `torch` and `active`, so the native view knows the camera
// and the duplicate window before it starts. Numbers cross as strings like other module
// element attributes.
#[doc(hidden)]
#[whisker::module_element(name = "masamune-whisker-barcode-scanner:Scanner", measurement = None)]
pub fn native_barcode_scanner(
    request_permission: Signal<bool>,
    facing: Signal<String>,
    debounce_ms: Signal<String>,
    torch: Signal<bool>,
    active: Signal<bool>,
    style: Style,
    on_scan: ScanEvent,
    on_permission: PermissionEvent,
    on_error: ErrorEvent,
) {
}

/// Element schema referenced by the generated Host bootstrap code (Web, desktop).
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [native_barcode_scanner_schema::element_provider()],
    )
}

/// Camera preview that reports QR codes.
///
/// While `active` is true the camera runs (after the permission is granted) and every decoded
/// code is passed to `on_scan`, with repeats of the same code dropped by a [`ScanDebouncer`]
/// (window: `debounce`, else the installed [`crate::BarcodeScannerMasamuneAdapter`]'s). Setting
/// `active` to false stops the camera and turns the torch off; activating again forgets the last
/// code, so the same code can be scanned again.
///
/// Permission: when the element starts and the camera permission is not decided, the system
/// prompt is shown (unless `request_permission` is false). The decided state is passed to
/// `on_permission` every time the element starts; with [`CameraPermission::Denied`] or
/// [`CameraPermission::Restricted`] the preview stays black. Use
/// [`crate::open_camera_settings`] to let the user change it.
///
/// With a [`crate::RuntimeBarcodeScannerAdapter`] installed, an empty box is shown and the codes
/// come from the adapter instead of the camera.
///
/// Pass `identifier` and `label` together for UI automation (Maestro `id` / `text`).
#[allow(clippy::too_many_arguments)]
#[component]
pub fn barcode_scanner(
    /// Run the camera and report codes (default true).
    active: Option<Signal<bool>>,
    /// Turn the torch on while scanning (default false; ignored without a torch).
    torch: Option<Signal<bool>>,
    /// Use the front camera instead of the back one.
    #[prop(default = false)]
    front_camera: bool,
    /// Show the system prompt when the permission is not decided (default true).
    #[prop(default = true)]
    request_permission: bool,
    /// Window in which the same code is reported once (default: the adapter's).
    debounce: Option<Duration>,
    /// A decoded code.
    on_scan: Option<Callback<Scan>>,
    /// The camera permission when the scanner starts.
    on_permission: Option<Callback<CameraPermission>>,
    /// The camera could not be started (no camera, configuration failure).
    on_error: Option<Callback<ScannerError>>,
    /// Accessibility identifier (Maestro `id`).
    identifier: Option<String>,
    /// Accessibility label (Maestro `text`).
    label: Option<String>,
    /// Structured Whisker style.
    style: Option<Style>,
) -> Element {
    let active = active.unwrap_or_else(|| Signal::from(true));
    let torch = torch.unwrap_or_default();
    let window = debounce.unwrap_or_else(default_debounce);
    let debouncer = Rc::new(RefCell::new(ScanDebouncer::new(window)));
    let accessibility = accessibility(identifier.clone(), label.clone());
    let style = style.clone().unwrap_or_default();

    let adapter = barcode_scanner_adapter();
    if let Some(feed) = adapter.feed() {
        attach_feed(
            feed,
            FeedHandlers {
                active,
                request_permission,
                debouncer,
                on_scan,
                on_permission,
                on_error,
            },
        );
        let mut placeholder = View::builder().style(style);
        if let Some(accessibility) = accessibility {
            placeholder = placeholder.accessibility(accessibility);
        }
        return placeholder.build();
    }

    {
        let debouncer = debouncer.clone();
        effect(move || {
            if active.get() {
                debouncer.borrow_mut().reset();
            }
        });
    }
    let mut builder = NativeBarcodeScanner::builder()
        .request_permission(request_permission)
        .facing(if front_camera { "front" } else { "back" }.to_string())
        .debounce_ms(window.as_millis().to_string())
        .torch(torch)
        .active(active)
        .style(style)
        .on_scan(move |event: ScanEvent| {
            let scan = Scan {
                format: BarcodeFormat::from_native(&event.detail.format),
                value: event.detail.value,
            };
            if debouncer.borrow_mut().accept(&scan.value)
                && let Some(cb) = &on_scan
            {
                cb.run(scan);
            }
        })
        .on_permission(move |event: PermissionEvent| {
            if let Some(cb) = &on_permission {
                cb.run(CameraPermission::from_native(&event.detail.status));
            }
        })
        .on_error(move |event: ErrorEvent| {
            if let Some(cb) = &on_error {
                cb.run(ScannerError {
                    code: event.detail.code,
                    message: event.detail.message,
                });
            }
        });
    if let Some(accessibility) = accessibility {
        builder = builder.accessibility(accessibility);
    }
    builder.build()
}

fn accessibility(identifier: Option<String>, label: Option<String>) -> Option<Accessibility> {
    if identifier.is_none() && label.is_none() {
        return None;
    }
    let mut accessibility = Accessibility::new();
    if let Some(identifier) = identifier {
        accessibility = accessibility.identifier(identifier);
    }
    if let Some(label) = label {
        accessibility = accessibility.label(label);
    }
    Some(accessibility)
}

/// What [`attach_feed`] reports to.
pub(crate) struct FeedHandlers {
    pub active: Signal<bool>,
    pub request_permission: bool,
    pub debouncer: Rc<RefCell<ScanDebouncer>>,
    pub on_scan: Option<Callback<Scan>>,
    pub on_permission: Option<Callback<CameraPermission>>,
    pub on_error: Option<Callback<ScannerError>>,
}

/// Connects a scanner to a [`ScanFeed`] for the lifetime of the current owner, mirroring the
/// native element: the permission is resolved (and requested) whenever `active` turns true, and
/// scans are taken only while active with the permission granted.
pub(crate) fn attach_feed(feed: ScanFeed, handlers: FeedHandlers) {
    let FeedHandlers {
        active,
        request_permission,
        debouncer,
        on_scan,
        on_permission,
        on_error,
    } = handlers;
    let permission: RwSignal<Option<CameraPermission>> = RwSignal::new(None);

    let listener = {
        let debouncer = debouncer.clone();
        feed.listen(move |event| match event {
            FeedEvent::Scan(scan) => {
                let ready = active.try_get_untracked() == Some(true)
                    && permission.try_get_untracked().flatten()
                        == Some(CameraPermission::Authorized);
                if !ready {
                    return false;
                }
                if debouncer.borrow_mut().accept(&scan.value)
                    && let Some(cb) = &on_scan
                {
                    cb.run(scan.clone());
                }
                true
            }
            FeedEvent::Error(error) => {
                if let Some(cb) = &on_error {
                    cb.run(error.clone());
                }
                true
            }
        })
    };
    let listener = Rc::new(RefCell::new(Some(listener)));
    on_cleanup(move || {
        listener.borrow_mut().take();
    });

    effect(move || {
        if !active.get() {
            return;
        }
        debouncer.borrow_mut().reset();
        let feed = feed.clone();
        spawn_local(async move {
            let adapter = barcode_scanner_adapter();
            let mut status = adapter.permission_status();
            if status == CameraPermission::NotDetermined && request_permission {
                status = adapter
                    .request_permission()
                    .await
                    .unwrap_or(CameraPermission::Denied);
            }
            if !permission.try_set(Some(status)) {
                return;
            }
            if let Some(cb) = &on_permission {
                cb.run(status);
            }
            if status == CameraPermission::Authorized {
                feed.flush();
            }
        });
    });
}
