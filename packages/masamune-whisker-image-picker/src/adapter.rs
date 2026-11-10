use masamune_whisker_adapter::MasamuneAdapter;

use crate::{NativeResult, PickedImage, pick_image};

/// Image picker settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `PickerMasamuneAdapter`): the size [`pick`] scales images down to.
///
/// The usage descriptions are build-time settings of [`crate::cng::MasamuneImagePicker`].
#[derive(Debug, Clone)]
pub struct ImagePickerMasamuneAdapter {
    /// Longest side of picked images in pixels.
    pub max_size: u32,
}

impl Default for ImagePickerMasamuneAdapter {
    fn default() -> Self {
        Self {
            max_size: DEFAULT_MAX_SIZE,
        }
    }
}

impl MasamuneAdapter for ImagePickerMasamuneAdapter {}

/// Longest side used without an installed adapter.
pub const DEFAULT_MAX_SIZE: u32 = 1024;

/// [`pick_image`] with the size of the installed [`ImagePickerMasamuneAdapter`].
pub async fn pick(camera: bool) -> NativeResult<Option<PickedImage>> {
    let max_size =
        ImagePickerMasamuneAdapter::primary().map_or(DEFAULT_MAX_SIZE, |adapter| adapter.max_size);
    pick_image(camera, max_size).await
}
