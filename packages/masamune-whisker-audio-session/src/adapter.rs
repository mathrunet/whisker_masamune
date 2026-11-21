use masamune_whisker_adapter::{AdapterError, MasamuneAdapter};

use crate::{AudioSessionConfig, configure};

/// Audio session settings for [`masamune_whisker_adapter::MasamuneAdapters`] (the counterpart of
/// `AudioSession.configure` and the `AudioServiceConfig` of Flutter's `audio_service`):
/// configures the session and the Android notification channel while the app prepares.
#[derive(Debug, Clone, Default)]
pub struct AudioSessionMasamuneAdapter {
    pub config: AudioSessionConfig,
}

impl AudioSessionMasamuneAdapter {
    pub fn new(config: AudioSessionConfig) -> Self {
        Self { config }
    }
}

impl MasamuneAdapter for AudioSessionMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        configure(&self.config)?;
        Ok(())
    }
}
