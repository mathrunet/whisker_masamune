//! Compile check of the README usage (not run).

use std::time::Duration;

use masamune_whisker_barcode_scanner::{
    BarcodeScanner, CameraPermission, DecodeError, Scan, ScannerError, decode_qr_from_image_async,
    open_camera_settings,
};
use whisker::prelude::*;

#[allow(dead_code)]
#[component]
fn ticket_scanner() -> Element {
    let active = RwSignal::new(true);
    let torch = RwSignal::new(false);
    let denied = RwSignal::new(false);
    render! {
        BarcodeScanner(
            active: active,
            torch: torch,
            front_camera: false,
            debounce: Duration::from_secs(2),
            identifier: "ticket_scanner".to_string(),
            label: "Ticket scanner".to_string(),
            on_scan: move |scan: Scan| {
                active.set(false);
                let _ = scan.value;
            },
            on_permission: move |status: CameraPermission| {
                denied.set(!status.is_authorized());
                if !status.is_authorized() {
                    let _ = open_camera_settings();
                }
            },
            on_error: move |_error: ScannerError| {},
            style: css!(flex_grow: 1.0),
        )
    }
}

#[allow(dead_code)]
async fn from_picked_photo(path: String) -> Option<String> {
    match decode_qr_from_image_async(path).await {
        Ok(value) => Some(value),
        Err(DecodeError::NotFound) => None,
        Err(_) => None,
    }
}

#[test]
fn usage_compiles() {}
