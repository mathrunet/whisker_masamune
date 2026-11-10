//! Shares files and text via the OS share sheet.

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Share")
}

/// Shares a file via the share sheet. `subject` and `text` are omitted when empty.
pub fn share_file(path: &str, mime_type: &str, subject: &str, text: &str) -> NativeResult<bool> {
    module().call(
        "shareFile",
        &json!({ "path": path, "mimeType": mime_type, "subject": subject, "text": text }),
    )
}

/// Shares text via the share sheet.
pub fn share_text(text: &str, subject: &str) -> NativeResult<bool> {
    module().call("shareText", &json!({ "text": text, "subject": subject }))
}
