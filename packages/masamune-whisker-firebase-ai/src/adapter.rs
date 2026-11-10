use std::borrow::Cow;

use masamune_whisker_adapter::MasamuneAdapter;

use crate::GenerateRequest;

/// Firebase AI settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `FirebaseAIMasamuneAdapter`): the model used by requests whose [`GenerateRequest::model`] is empty.
#[derive(Debug, Clone, Default)]
pub struct FirebaseAiMasamuneAdapter {
    pub model: String,
}

impl MasamuneAdapter for FirebaseAiMasamuneAdapter {}

/// `request` with the adapter's model filled in when it names none.
pub(crate) fn with_default_model(request: &GenerateRequest) -> Cow<'_, GenerateRequest> {
    if !request.model.is_empty() {
        return Cow::Borrowed(request);
    }
    match FirebaseAiMasamuneAdapter::primary() {
        Some(adapter) => Cow::Owned(GenerateRequest {
            model: adapter.model.clone(),
            ..request.clone()
        }),
        None => Cow::Borrowed(request),
    }
}
