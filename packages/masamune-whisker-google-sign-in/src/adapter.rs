use masamune_whisker_adapter::MasamuneAdapter;

/// Google Sign-In settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `FirebaseGoogleAuthMasamuneAdapter`): the scopes [`crate::sign_in`] and [`crate::link`] request.
#[derive(Debug, Clone)]
pub struct GoogleSignInMasamuneAdapter {
    pub scopes: Vec<String>,
}

impl Default for GoogleSignInMasamuneAdapter {
    fn default() -> Self {
        Self {
            scopes: DEFAULT_SCOPES
                .iter()
                .map(|scope| (*scope).to_owned())
                .collect(),
        }
    }
}

impl MasamuneAdapter for GoogleSignInMasamuneAdapter {}

/// Scopes requested without an installed adapter.
pub const DEFAULT_SCOPES: &[&str] = &["email", "profile"];

pub(crate) fn scopes() -> Vec<String> {
    GoogleSignInMasamuneAdapter::primary()
        .map(|adapter| adapter.scopes.clone())
        .unwrap_or_else(|| GoogleSignInMasamuneAdapter::default().scopes)
}
