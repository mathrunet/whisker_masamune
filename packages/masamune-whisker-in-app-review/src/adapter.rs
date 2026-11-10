use masamune_whisker_adapter::MasamuneAdapter;

use crate::{NativeError, NativeResult, request_review};

/// In-app review settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `AppReviewMasamuneAdapter`): the store pages opened when the review prompt is unavailable.
#[derive(Debug, Clone, Default)]
pub struct InAppReviewMasamuneAdapter {
    pub google_play_store_url: String,
    pub app_store_url: String,
}

impl MasamuneAdapter for InAppReviewMasamuneAdapter {}

impl InAppReviewMasamuneAdapter {
    /// The store page of the running platform (Google Play except on iOS).
    pub fn store_url(&self) -> &str {
        if cfg!(target_os = "ios") {
            &self.app_store_url
        } else {
            &self.google_play_store_url
        }
    }
}

/// [`request_review`] with the store page of the installed [`InAppReviewMasamuneAdapter`].
pub async fn review() -> NativeResult<bool> {
    let adapter = InAppReviewMasamuneAdapter::primary()
        .ok_or_else(|| NativeError("InAppReviewMasamuneAdapter is not installed".into()))?;
    request_review(adapter.store_url()).await
}
