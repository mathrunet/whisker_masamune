//! QR code scanning for Whisker apps: a camera preview element that reports decoded codes, and
//! decoding of still images (for example a photo picked with `masamune-whisker-image-picker`).
//!
//! - [`BarcodeScanner`]: camera preview (`AVCaptureSession` + `AVCaptureMetadataOutput` on iOS,
//!   CameraX + ZXing core on Android) with start / stop (`active`), torch, the camera permission
//!   flow and a duplicate filter. The native element tag is
//!   `masamune-whisker-barcode-scanner:Scanner`.
//! - [`decode_qr_from_image`] / [`decode_qr_from_bytes`] / [`decode_qr_codes`]: PNG / JPEG
//!   decoding in pure Rust (`image` + `rqrr`), on every platform.
//! - [`BarcodeScannerAdapter`]: where the permission and the scans come from.
//!   [`RuntimeBarcodeScannerAdapter`] replaces the camera with scans fed from Rust for tests and
//!   the `test` flavor; install it with [`BarcodeScannerMasamuneAdapter`].
//!
//! On iOS the camera needs `NSCameraUsageDescription`; write it with the
//! [`cng::MasamuneBarcodeScanner`] CNG plugin. Android declares `CAMERA` in the library manifest.
//!
//! See the crate README for usage. Native sources:
//! `android/src/main/kotlin/net/mathru/whisker/barcode_scanner/` and
//! `ios/Sources/MasamuneWhiskerBarcodeScanner/`.

pub mod cng;

mod adapter;
mod debounce;
mod decode;
mod scanner;

pub use adapter::{
    BarcodeScannerAdapter, BarcodeScannerMasamuneAdapter, DEFAULT_DEBOUNCE, FeedEvent,
    FeedListener, NativeBarcodeScannerAdapter, PermissionFuture, RuntimeBarcodeScannerAdapter,
    ScanFeed, barcode_scanner_adapter, camera_permission, default_debounce, open_camera_settings,
    request_camera_permission,
};
pub use debounce::ScanDebouncer;
pub use decode::{
    DecodeError, decode_qr_codes, decode_qr_codes_in, decode_qr_from_bytes, decode_qr_from_image,
    decode_qr_from_image_async,
};
pub use masamune_whisker_bridge::{NativeError, NativeResult};
#[doc(hidden)]
pub use scanner::{__whisker_element_module_definition, NativeBarcodeScanner};
pub use scanner::{BarcodeScanner, BarcodeScannerProps};

use masamune_whisker_bridge::NativeModule;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "BarcodeScanner")
}

/// Camera permission state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraPermission {
    /// The camera can be used.
    Authorized,
    /// The user refused; only the system settings can change it.
    Denied,
    /// The camera is blocked by the device (parental controls, MDM). iOS only.
    Restricted,
    /// Not asked yet: requesting shows the system prompt.
    NotDetermined,
}

impl CameraPermission {
    /// Parses the native status string (`authorized`, `denied`, `restricted`, `notDetermined`).
    /// Unknown values count as denied.
    pub fn from_native(status: &str) -> Self {
        match status {
            "authorized" => Self::Authorized,
            "restricted" => Self::Restricted,
            "notDetermined" => Self::NotDetermined,
            _ => Self::Denied,
        }
    }

    /// Whether the camera can be used.
    pub fn is_authorized(self) -> bool {
        self == Self::Authorized
    }
}

/// Symbology of a scanned code.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BarcodeFormat {
    /// QR code.
    Qr,
    /// A symbology this crate does not name (the native identifier).
    Other(String),
}

impl BarcodeFormat {
    /// Parses the native format string (`qr`).
    pub fn from_native(format: &str) -> Self {
        match format {
            "" | "qr" => Self::Qr,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// A decoded code reported by [`BarcodeScanner`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scan {
    /// The text of the code.
    pub value: String,
    /// Its symbology.
    pub format: BarcodeFormat,
}

impl Scan {
    /// A QR code with `value`.
    pub fn qr(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            format: BarcodeFormat::Qr,
        }
    }
}

/// A camera failure reported by [`BarcodeScanner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannerError {
    /// `unavailable` (no usable camera, for example the iOS simulator), `configuration` (the
    /// session could not be set up) or `interrupted` (another app took the camera).
    pub code: String,
    /// Platform description, for logs at debug level.
    pub message: String,
}

impl ScannerError {
    /// The `unavailable` error.
    pub fn unavailable() -> Self {
        Self {
            code: "unavailable".into(),
            message: "no camera is available".into(),
        }
    }
}

impl std::fmt::Display for ScannerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "barcode scanner error: {}", self.code)
    }
}

impl std::error::Error for ScannerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker::WhiskerValue;

    #[test]
    fn schema_declares_property_order_and_events() {
        let schema = scanner::native_barcode_scanner_schema::schema();
        assert_eq!(schema.name, "masamune-whisker-barcode-scanner:Scanner");
        assert_eq!(
            schema
                .properties
                .iter()
                .map(|property| property.name.as_str())
                .collect::<Vec<_>>(),
            [
                "request-permission",
                "facing",
                "debounce-ms",
                "torch",
                "active"
            ]
        );
        assert_eq!(
            schema
                .events
                .iter()
                .map(|event| event.name.as_str())
                .collect::<Vec<_>>(),
            ["scan", "permission", "error"]
        );
    }

    #[test]
    fn event_bodies_deserialize() {
        let scan: scanner::ScanEvent = WhiskerValue::map([(
            "detail",
            WhiskerValue::map([
                ("value", WhiskerValue::String("hello".into())),
                ("format", WhiskerValue::String("qr".into())),
            ]),
        )])
        .deserialize_into()
        .unwrap();
        assert_eq!(scan.detail.value, "hello");
        let empty: [(&str, WhiskerValue); 0] = [];
        let permission: scanner::PermissionEvent =
            WhiskerValue::map(empty).deserialize_into().unwrap();
        assert_eq!(
            CameraPermission::from_native(&permission.detail.status),
            CameraPermission::Denied
        );
    }

    #[test]
    fn native_strings_parse() {
        assert_eq!(
            CameraPermission::from_native("notDetermined"),
            CameraPermission::NotDetermined
        );
        assert_eq!(
            CameraPermission::from_native("restricted"),
            CameraPermission::Restricted
        );
        assert!(CameraPermission::from_native("authorized").is_authorized());
        assert_eq!(BarcodeFormat::from_native("qr"), BarcodeFormat::Qr);
        assert_eq!(
            BarcodeFormat::from_native("ean13"),
            BarcodeFormat::Other("ean13".into())
        );
        assert_eq!(
            ScannerError::unavailable().to_string(),
            "barcode scanner error: unavailable"
        );
    }
}
