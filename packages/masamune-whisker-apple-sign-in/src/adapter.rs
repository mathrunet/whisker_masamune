use masamune_whisker_adapter::MasamuneAdapter;

/// Sign in with Apple settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `FirebaseAppleAuthMasamuneAdapter`): the scopes [`crate::sign_in`] and [`crate::link`] request.
#[derive(Debug, Clone)]
pub struct AppleSignInMasamuneAdapter {
    pub scopes: Vec<String>,
}

impl Default for AppleSignInMasamuneAdapter {
    fn default() -> Self {
        Self {
            scopes: DEFAULT_SCOPES
                .iter()
                .map(|scope| (*scope).to_owned())
                .collect(),
        }
    }
}

impl MasamuneAdapter for AppleSignInMasamuneAdapter {}

/// Scopes requested without an installed adapter.
pub const DEFAULT_SCOPES: &[&str] = &["email", "name"];

pub(crate) fn scopes() -> Vec<String> {
    AppleSignInMasamuneAdapter::primary()
        .map(|adapter| adapter.scopes.clone())
        .unwrap_or_else(|| AppleSignInMasamuneAdapter::default().scopes)
}
