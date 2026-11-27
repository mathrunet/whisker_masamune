use std::cell::RefCell;

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};
use whisker::prelude::*;

use crate::{Messaging, MessagingSubscription, PushMessage};

/// Push messaging settings for [`masamune_whisker_adapter::MasamuneAdapters`].
///
/// `prepare` subscribes to foreground messages, notification taps and token refreshes once per
/// runtime. `start` reads the cold-start launch notification (if any) and optionally requests
/// permission when [`Self::request_permission_on_start`] is `true` (default `false`, so apps can
/// prompt at a contextual moment).
#[derive(Clone, Debug)]
pub struct MessagingMasamuneAdapter {
    pub messaging: Messaging,
    /// Whether to request notification permission during startup (default `false`).
    pub request_permission_on_start: bool,
}

impl MessagingMasamuneAdapter {
    pub fn new(messaging: Messaging) -> Self {
        Self {
            messaging,
            request_permission_on_start: false,
        }
    }

    pub fn request_permission_on_start(mut self, enabled: bool) -> Self {
        self.request_permission_on_start = enabled;
        self
    }
}

whisker::runtime_local! {
    static SUBSCRIPTIONS: RefCell<Vec<MessagingSubscription>> = RefCell::new(Vec::new());
    static TOKEN: RwSignal<Option<String>> = signal(None);
    static LATEST_MESSAGE: RwSignal<Option<PushMessage>> = signal(None);
    static LATEST_OPENED: RwSignal<Option<PushMessage>> = signal(None);
}

impl MasamuneAdapter for MessagingMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        SUBSCRIPTIONS.with(|subs| {
            let mut subs = subs.borrow_mut();
            if subs.is_empty() {
                subs.push(self.messaging.on_token_refresh(|token| {
                    if !token.is_empty() {
                        TOKEN.with(|slot| slot.set(Some(token)));
                    }
                }));
                subs.push(self.messaging.on_message(|message| {
                    LATEST_MESSAGE.with(|slot| slot.set(Some(message)));
                }));
                subs.push(self.messaging.on_message_opened(|message| {
                    LATEST_OPENED.with(|slot| slot.set(Some(message)));
                }));
            }
        });
        Ok(())
    }

    fn start(&self) -> Option<StartFuture> {
        let messaging = self.messaging.clone();
        let request_permission = self.request_permission_on_start;
        Some(Box::pin(async move {
            if request_permission {
                let _ = messaging.request_permission().await;
            }
            if let Ok(Some(message)) = messaging.initial_message().await {
                LATEST_OPENED.with(|slot| slot.set(Some(message)));
            }
            Ok(())
        }))
    }
}

/// Latest push registration token observed through token refresh events.
pub fn current_token() -> ReadSignal<Option<String>> {
    TOKEN.with(|slot| slot.read_only())
}

/// Most recent foreground push message that has not been taken with [`take_pending_message`].
pub fn latest_message() -> ReadSignal<Option<PushMessage>> {
    LATEST_MESSAGE.with(|slot| slot.read_only())
}

/// Takes and clears the pending foreground push message.
pub fn take_pending_message() -> Option<PushMessage> {
    LATEST_MESSAGE.with(|slot| {
        let pending = slot.get_untracked();
        if pending.is_some() {
            slot.set(None);
        }
        pending
    })
}

/// Most recent notification tap (cold start or background) that has not been taken with
/// [`take_pending_opened_message`].
pub fn latest_opened_message() -> ReadSignal<Option<PushMessage>> {
    LATEST_OPENED.with(|slot| slot.read_only())
}

/// Takes and clears the pending opened push message.
pub fn take_pending_opened_message() -> Option<PushMessage> {
    LATEST_OPENED.with(|slot| {
        let pending = slot.get_untracked();
        if pending.is_some() {
            slot.set(None);
        }
        pending
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeMessagingAdapter;
    use masamune_whisker_adapter::MasamuneAdapters;
    use whisker::Owner;
    use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

    #[test]
    fn adapter_delivers_initial_and_live_messages() {
        let runtime_adapter = RuntimeMessagingAdapter::new()
            .with_token("t1")
            .with_initial_message(PushMessage {
                id: Some("launch".into()),
                ..Default::default()
            });
        let rt = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
        rt.enter(|| {
            let root = Owner::new(None);
            root.with(|| {
                MasamuneAdapters::new()
                    .adapter(MessagingMasamuneAdapter::new(Messaging::new(
                        runtime_adapter.clone(),
                    )))
                    .install();
            });
            whisker::run_until_stalled();
            assert_eq!(
                take_pending_opened_message().and_then(|m| m.id),
                Some("launch".into())
            );
            assert!(take_pending_opened_message().is_none());

            runtime_adapter.emit_token_refresh("t2");
            assert_eq!(current_token().get_untracked().as_deref(), Some("t2"));

            runtime_adapter.emit_message(PushMessage {
                id: Some("fg".into()),
                ..Default::default()
            });
            assert_eq!(take_pending_message().and_then(|m| m.id), Some("fg".into()));
            assert_eq!(runtime_adapter.permission_requests(), 0);
            root.dispose();
        });
        rt.shutdown();
    }
}
