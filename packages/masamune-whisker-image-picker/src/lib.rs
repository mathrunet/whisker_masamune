//! Picks an image from the photo library or camera and saves it as a PNG with the long side scaled down to the given size.

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

/// Picks an image (from the camera if `camera` is true). Returns the path of a PNG whose long side is at most `max_size`, or `None` if cancelled.
pub async fn pick_image(camera: bool, max_size: u32) -> NativeResult<Option<PickedImage>> {
    module()
        .call_async(
            "pickImage",
            &json!({ "source": if camera { "camera" } else { "gallery" }, "maxSize": max_size }),
        )
        .await
}
