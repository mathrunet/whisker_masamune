//! 写真ライブラリ・カメラから画像を選び、長辺を指定サイズ以下に縮小した PNG として保存する。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "ImagePicker")
}

use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Clone, Deserialize)]
pub struct PickedImage {
    pub path: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

/// 画像を選ぶ（`camera` が true ならカメラ）。長辺を `max_size` 以下に縮小した PNG のパス。キャンセルは `None`。
pub async fn pick_image(camera: bool, max_size: u32) -> NativeResult<Option<PickedImage>> {
    module()
        .call_async(
            "pickImage",
            &json!({ "source": if camera { "camera" } else { "gallery" }, "maxSize": max_size }),
        )
        .await
}
