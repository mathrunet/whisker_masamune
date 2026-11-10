use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};

use crate::{NotificationConfig, authorized, configure, rearm};

/// Notification settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `LocalNotificationMasamuneAdapter`): configures the channel while the app prepares and
/// re-arms the saved schedules after startup.
#[derive(Debug, Clone)]
pub struct LocalNotificationMasamuneAdapter {
    pub config: NotificationConfig<'static>,
    /// Re-arm the saved schedules after startup when notifications are allowed.
    pub rearm_on_start: bool,
}

impl LocalNotificationMasamuneAdapter {
    pub fn new(config: NotificationConfig<'static>) -> Self {
        Self {
            config,
            rearm_on_start: true,
        }
    }
}

impl MasamuneAdapter for LocalNotificationMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        configure(&self.config)?;
        Ok(())
    }

    fn start(&self) -> Option<StartFuture> {
        if !self.rearm_on_start {
            return None;
        }
        Some(Box::pin(async {
            if authorized() {
                rearm()?;
            }
            Ok(())
        }))
    }
}
