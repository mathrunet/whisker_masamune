//! 同梱フォントを `font_family("名前")` で使えるように登録する。
//!
//! フォントファイルはアプリが whisker-asset で同梱し、`whisker_asset::resolve` したパスを渡す。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Serialize;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Fonts")
}

/// 1 つのフォントファイルと、そのファイルを割り当てるウェイト（100〜900）。
#[derive(Debug, Clone, Serialize)]
pub struct FontFile {
    pub path: String,
    pub weights: Vec<u16>,
}

/// ファミリー名と、そのファミリーを構成するファイル。
#[derive(Debug, Clone, Serialize)]
pub struct FontFamily {
    pub family: String,
    pub files: Vec<FontFile>,
}

impl FontFamily {
    pub fn new(family: impl Into<String>) -> Self {
        Self {
            family: family.into(),
            files: Vec::new(),
        }
    }

    pub fn file(mut self, path: impl Into<String>, weights: &[u16]) -> Self {
        self.files.push(FontFile {
            path: path.into(),
            weights: weights.to_vec(),
        });
        self
    }
}

/// フォントを登録し、登録できたファミリー名を返す。登録済みのファミリーは読み直さない。
///
/// 最初の Text を計測する前（`app()` の先頭）で呼ぶ。
pub fn register(families: &[FontFamily]) -> NativeResult<Vec<String>> {
    module().call("register", &serde_json::json!({ "families": families }))
}
