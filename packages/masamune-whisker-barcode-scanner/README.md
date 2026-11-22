# masamune-whisker-barcode-scanner

QR code scanning for Whisker apps:

- `BarcodeScanner`: a camera preview element that reports decoded QR codes, with start / stop,
  torch, front / back camera, the camera permission flow and a duplicate filter.
  - **iOS**: `AVCaptureSession` + `AVCaptureVideoPreviewLayer` + `AVCaptureMetadataOutput` (QR).
  - **Android**: Jetpack CameraX (`PreviewView` + `ImageAnalysis`) with ZXing core
    (`com.google.zxing:core`). No Google Play services or ML Kit.
  - Hosts other than iOS and Android render an empty box.
- `decode_qr_from_image` / `decode_qr_from_bytes` / `decode_qr_codes`: read QR codes from PNG or
  JPEG images in pure Rust (`image` + `rqrr`) on every platform, for example from a photo picked
  with `masamune-whisker-image-picker`.
- `RuntimeBarcodeScannerAdapter`: replaces the camera with scans fed from Rust, for unit tests and
  the `test` flavor (E2E tests on simulators, which have no camera).

## Setup

iOS requires `NSCameraUsageDescription`. Write it with the CNG plugin in `whisker.rs`:

```rust,ignore
use masamune_whisker_barcode_scanner::cng::MasamuneBarcodeScanner;

app.plugin::<MasamuneBarcodeScanner>(|c| {
    c.camera_usage("Scan the QR code on your ticket.");
});
```

Without `camera_usage` a short English default is written, unless another plugin (such as
`masamune-whisker-image-picker`) already set the key.

Android needs no setup: the library manifest adds `android.permission.CAMERA` and declares the
camera features as optional (`android:required="false"`), so devices without a camera can still
install the app.

## Usage

```rust,ignore
use masamune_whisker_barcode_scanner::{
    BarcodeScanner, CameraPermission, Scan, ScannerError, open_camera_settings,
};
use whisker::prelude::*;

#[component]
fn ticket_scanner() -> Element {
    let active = RwSignal::new(true);
    let torch = RwSignal::new(false);
    let denied = RwSignal::new(false);
    render! {
        BarcodeScanner(
            active: active,
            torch: torch,
            // Maestro: `id: ticket_scanner`, `text: Ticket scanner`.
            identifier: "ticket_scanner".to_string(),
            label: l().ticket_scanner_label().to_string(),
            on_scan: move |scan: Scan| {
                active.set(false); // stop the camera while the ticket is checked
                check_ticket(scan.value);
            },
            on_permission: move |status: CameraPermission| {
                denied.set(!status.is_authorized());
            },
            on_error: move |error: ScannerError| log::debug!("scanner: {}", error.code),
            style: css!(width: percent(100), aspect_ratio: 1.0),
        )
    }
}
```

- `active` (default `true`) starts and stops the camera; the camera also stops while the element
  is not on screen. Turning it off and on again forgets the last code, so the same code can be
  scanned again.
- `on_scan` receives each code once while it stays in view: a repeat of the same code is dropped
  until the code has been out of view for the debounce window (`debounce`, default 1.5 s or the
  installed adapter's). A different code is reported at once.
- `torch` lights the flash while the camera runs (ignored on devices without one);
  `front_camera: true` uses the front camera.
- `on_error` codes: `unavailable` (no camera, for example the iOS simulator), `configuration`
  (the session could not be set up) and `interrupted` (another app took the camera).
- Pass `identifier` and `label` together so Maestro can find the element by `id` and `text`.

### Camera permission

When the scanner starts and the permission has not been decided, the system prompt is shown
(`request_permission: false` turns this off). `on_permission` then receives the state each time
the scanner starts; with `Denied` / `Restricted` the preview stays black. Explain why the camera
is needed in your own UI and offer `open_camera_settings()`:

```rust,ignore
use masamune_whisker_barcode_scanner::{
    CameraPermission, camera_permission, open_camera_settings, request_camera_permission,
};

if camera_permission() == CameraPermission::NotDetermined {
    let status = request_camera_permission().await?;
}
let _ = open_camera_settings();
```

Android does not report whether the user was asked before, so the first request is remembered:
the state is `NotDetermined` until then (and while the system would still show the prompt), and
`Denied` after the prompt stops appearing. `Restricted` is iOS only.

### Reading a code from a photo

Combine with `masamune-whisker-image-picker` to read a QR code from the photo library (for example
a screenshot of a ticket):

```rust,ignore
use masamune_whisker_barcode_scanner::{DecodeError, decode_qr_from_image_async};
use masamune_whisker_image_picker::pick;

spawn_local(async move {
    let Ok(Some(picked)) = pick(false).await else { return };
    match decode_qr_from_image_async(picked.path).await {
        Ok(value) => check_ticket(value),
        Err(DecodeError::NotFound) => show_message(l().no_qr_code_found()),
        Err(_) => show_message(l().image_unreadable()),
    }
});
```

`decode_qr_from_image` reads a PNG or JPEG file on the calling thread; the `_async` variant runs
it on a worker thread. Large images are scaled down to 1280 px on the long side before detection,
light-on-dark codes are retried inverted, and `decode_qr_codes` returns every code in the image.
`DecodeError` has fixed descriptions without paths or contents. Decoding is slow in unoptimized
debug builds of the app (about a second for a phone photo); release builds are fast.

## Tests and the `test` flavor

Install `RuntimeBarcodeScannerAdapter` in the `test` flavor (or in unit tests) through
`BarcodeScannerMasamuneAdapter`. `BarcodeScanner` then shows an empty box (with the same
accessibility identifier and label) and reports the scans you feed it:

```rust,ignore
use masamune_whisker_barcode_scanner::{
    BarcodeScannerMasamuneAdapter, CameraPermission, RuntimeBarcodeScannerAdapter,
};

let scanner = RuntimeBarcodeScannerAdapter::new();
let adapter = if flavor == Flavor::Test {
    BarcodeScannerMasamuneAdapter::with_adapter(scanner.clone())
} else {
    BarcodeScannerMasamuneAdapter::new()
};
MasamuneAdapters::new().adapter(adapter).install();

// Later (a debug hook, a test seed, a unit test):
scanner.scan("https://example.com/ticket/1");
```

A scan that no active scanner takes is queued and delivered when one becomes active, so a seed
scan can be pushed before the scanner screen opens. `with_permission(CameraPermission::Denied)`
and `with_permission(CameraPermission::NotDetermined).grant_on_request(false)` exercise the
permission paths; `permission_requests()` and `settings_opened()` count the calls. `fail(…)`
reports a camera error.

## Implementing another adapter

`BarcodeScannerAdapter` has `permission_status`, `request_permission`, `open_settings` and an
optional `feed` (a `ScanFeed`). An adapter that returns a feed replaces the camera preview; one that
returns `None` keeps the native camera.
