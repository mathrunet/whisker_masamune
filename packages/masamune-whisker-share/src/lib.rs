//! OS の共有シートでファイルとテキストを送る。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Share")
}

/// ファイルを共有シートで送る。`subject`・`text` は空なら付けない。
pub fn share_file(path: &str, mime_type: &str, subject: &str, text: &str) -> NativeResult<bool> {
    module().call(
        "shareFile",
        &json!({ "path": path, "mimeType": mime_type, "subject": subject, "text": text }),
    )
}

/// テキストを共有シートで送る。
pub fn share_text(text: &str, subject: &str) -> NativeResult<bool> {
    module().call("shareText", &json!({ "text": text, "subject": subject }))
}
