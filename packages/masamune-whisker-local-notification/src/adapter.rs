use std::cell::RefCell;

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};
use whisker::prelude::*;

use crate::{
    NotificationChannel, NotificationConfig, NotificationTap, Subscription, authorized, configure,
    create_channel, initial_tap, on_tap, rearm,
};

/// Notification settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `LocalNotificationMasamuneAdapter`).
///
/// `prepare` creates the extra [`channels`](Self::channels), configures the channel and subscribes
/// to [`on_tap`] once per runtime. `start`
/// re-arms the saved schedules (when notifications are allowed) and reads [`initial_tap`]. Every
/// tap is published to [`latest_tap`] and kept for [`take_pending_tap`].
#[derive(Debug, Clone)]
pub struct LocalNotificationMasamuneAdapter {
    pub config: NotificationConfig<'static>,
    /// Re-arm the saved schedules after startup when notifications are allowed.
    pub rearm_on_start: bool,
    /// Android channels created before `config` is applied (for example a
    /// [`NotificationImportance::High`](crate::NotificationImportance::High) channel for urgent
    /// notifications). A channel with the ID of `config` sets the importance of the default
    /// channel when it is created for the first time.
    pub channels: Vec<NotificationChannel>,
}

impl LocalNotificationMasamuneAdapter {
    pub fn new(config: NotificationConfig<'static>) -> Self {
        Self {
            config,
            rearm_on_start: true,
            channels: Vec::new(),
        }
    }

    /// Adds a channel created at startup (see [`channels`](Self::channels)).
    pub fn channel(mut self, channel: NotificationChannel) -> Self {
        self.channels.push(channel);
        self
    }
}

whisker::runtime_local! {
    static SUBSCRIPTION: RefCell<Option<Subscription>> = RefCell::new(None);
    static LATEST_TAP: RwSignal<Option<NotificationTap>> = signal(None);
}

impl MasamuneAdapter for LocalNotificationMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        for channel in &self.channels {
            create_channel(channel)?;
        }
        configure(&self.config)?;
        SUBSCRIPTION.with(|subscription| {
            let mut subscription = subscription.borrow_mut();
            if subscription.is_none() {
                *subscription = Some(on_tap(deliver));
            }
        });
        Ok(())
    }

    fn start(&self) -> Option<StartFuture> {
        let rearm_on_start = self.rearm_on_start;
        Some(Box::pin(async move {
            let rearmed = if rearm_on_start && authorized() {
                rearm().map(|_| ())
            } else {
                Ok(())
            };
            if let Some(tap) = initial_tap().await {
                deliver(tap);
            }
            rearmed?;
            Ok(())
        }))
    }
}

fn deliver(tap: NotificationTap) {
    LATEST_TAP.with(|latest| latest.set(Some(tap)));
}

/// The most recent notification tap that has not been taken with [`take_pending_tap`]. Reading it
/// inside an effect tracks it, so the effect runs again when a tap arrives.
pub fn latest_tap() -> Option<NotificationTap> {
    LATEST_TAP.with(|latest| latest.get())
}

/// Takes the most recent notification tap, clearing [`latest_tap`]. Returns `None` when there is
/// none or it was already taken.
pub fn take_pending_tap() -> Option<NotificationTap> {
    LATEST_TAP.with(|latest| {
        let pending = latest.get_untracked();
        if pending.is_some() {
            latest.set(None);
        }
        pending
    })
}
