use masamune_whisker_adapter::{AdapterError, MasamuneAdapter};

use crate::activate;

/// App Check settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `FirebaseAppCheckMasamuneAdapter`): activates App Check while the app prepares, before Remote
/// Config, Firestore or callable functions request tokens.
#[derive(Debug, Clone, Default)]
pub struct FirebaseAppCheckMasamuneAdapter {
    /// Fixed debug token for debug builds (pass it through an environment variable, never commit it).
    pub debug_token: Option<String>,
}

impl MasamuneAdapter for FirebaseAppCheckMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        activate(self.debug_token.as_deref())?;
        Ok(())
    }
}
