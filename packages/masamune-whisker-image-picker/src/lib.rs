//! Picks an image from the photo library or camera and saves it as a PNG with the long side scaled down to the given size.
//!
//! On iOS the camera needs `NSCameraUsageDescription` in `Info.plist`; write it with the
//! [`cng::MasamuneImagePicker`] CNG plugin.
//!
//! In development, [`set_debug_picture`] makes the camera return a prepared image instead of
//! opening (Flutter's `Camera.setDebugPicture`, used by `masamune-whisker-ai-debugger`).

pub mod cng;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "ImagePicker")
}

use std::cell::RefCell;

use serde::Deserialize;
use serde_json::json;

thread_local! {
    static DEBUG_PICTURE: RefCell<Option<PickedImage>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PickedImage {
    pub path: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

/// Picks an image (from the camera if `camera` is true). Returns the path of a PNG whose long side is at most `max_size`, or `None` if cancelled.
///
/// On iOS the library uses `PHPickerViewController` and the camera `UIImagePickerController`; the
/// PNG is written to `Caches/masamune_whisker_image_picker/`. Taking a photo fails when the device
/// has no camera (for example the simulator) or camera access was denied.
///
/// While a debug picture is set ([`set_debug_picture`]), taking a photo returns it without
/// opening the camera (it is not resized).
pub async fn pick_image(camera: bool, max_size: u32) -> NativeResult<Option<PickedImage>> {
    if camera && let Some(image) = debug_picture() {
        return Ok(Some(image));
    }
    module()
        .call_async(
            "pickImage",
            &json!({ "source": if camera { "camera" } else { "gallery" }, "maxSize": max_size }),
        )
        .await
}

/// Makes [`pick_image`] with `camera = true` return `image` until [`unset_debug_picture`].
/// For development tooling only; the setting lives in memory on the UI thread.
pub fn set_debug_picture(image: PickedImage) {
    DEBUG_PICTURE.with(|slot| *slot.borrow_mut() = Some(image));
}

/// Restores the real camera. Returns false when no debug picture was set.
pub fn unset_debug_picture() -> bool {
    DEBUG_PICTURE.with(|slot| slot.borrow_mut().take().is_some())
}

/// The debug picture currently returned by the camera.
pub fn debug_picture() -> Option<PickedImage> {
    DEBUG_PICTURE.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_picture_replaces_the_camera() {
        let image = PickedImage {
            path: "/tmp/debug.png".into(),
            width: 10,
            height: 20,
        };
        set_debug_picture(image.clone());
        assert_eq!(debug_picture(), Some(image.clone()));
        let picked = block_on(pick_image(true, 100));
        assert_eq!(picked.unwrap(), Some(image));
        assert!(unset_debug_picture());
        assert!(!unset_debug_picture());
        assert_eq!(debug_picture(), None);
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("debug picture should resolve immediately"),
        }
    }
}
