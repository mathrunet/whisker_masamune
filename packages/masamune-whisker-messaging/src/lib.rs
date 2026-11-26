#![doc = include_str!("../README.md")]

mod adapter;
mod runtime;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

pub use adapter::{
    MessagingMasamuneAdapter, current_token, latest_message, latest_opened_message,
    take_pending_message, take_pending_opened_message,
};
pub use runtime::RuntimeMessagingAdapter;
use serde::{Deserialize, Serialize};

/// Failure of a push messaging operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagingError {
    /// Push messaging is unavailable on this device or platform.
    Unsupported(String),
    /// The user denied notification permission.
    PermissionDenied,
    /// Temporary or transport failure.
    Unavailable(String),
    /// Provider-reported error.
    Backend(String),
}

impl std::fmt::Display for MessagingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(msg) => write!(f, "messaging unsupported: {msg}"),
            Self::PermissionDenied => f.write_str("messaging permission denied"),
            Self::Unavailable(msg) => write!(f, "messaging unavailable: {msg}"),
            Self::Backend(msg) => write!(f, "messaging backend error: {msg}"),
        }
    }
}

impl std::error::Error for MessagingError {}

pub type MessagingResult<T> = Result<T, MessagingError>;

/// Future returned by [`MessagingAdapter`] methods (runs on the UI thread).
pub type MessagingFuture<'a, T> = Pin<Box<dyn Future<Output = MessagingResult<T>> + 'a>>;

/// Token that keeps a push messaging event subscription active until dropped.
pub struct MessagingSubscription {
    _guard: Box<dyn std::any::Any>,
}

impl MessagingSubscription {
    pub fn new(guard: impl std::any::Any + 'static) -> Self {
        Self {
            _guard: Box::new(guard),
        }
    }

    /// An inert subscription that does nothing when dropped.
    pub fn empty() -> Self {
        Self::new(())
    }
}

impl std::fmt::Debug for MessagingSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MessagingSubscription")
    }
}

/// A received or opened remote push notification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushMessage {
    /// Provider message identifier, if available.
    #[serde(default)]
    pub id: Option<String>,
    /// Notification title, if present.
    #[serde(default)]
    pub title: Option<String>,
    /// Notification body, if present.
    #[serde(default)]
    pub body: Option<String>,
    /// Custom key-value payload attached to the notification.
    #[serde(default)]
    pub data: BTreeMap<String, String>,
}

/// Object-safe push messaging backend interface.
pub trait MessagingAdapter {
    /// Requests OS push notification permission. Returns `true` when granted.
    fn request_permission(&self) -> MessagingFuture<'_, bool>;

    /// Returns the current device registration token (such as an FCM token), if available.
    fn token(&self) -> MessagingFuture<'_, Option<String>>;

    /// Deletes the current device registration token (for example on sign-out or account deletion).
    fn delete_token(&self) -> MessagingFuture<'_, ()>;

    /// Returns the notification that launched the app from a terminated state, once.
    fn initial_message(&self) -> MessagingFuture<'_, Option<PushMessage>>;

    /// Subscribes to token rotations while the returned [`MessagingSubscription`] is held.
    fn on_token_refresh(&self, handler: Rc<dyn Fn(String)>) -> MessagingSubscription;

    /// Subscribes to foreground messages while the returned [`MessagingSubscription`] is held.
    fn on_message(&self, handler: Rc<dyn Fn(PushMessage)>) -> MessagingSubscription;

    /// Subscribes to notifications tapped while the app is running or in the background.
    fn on_message_opened(&self, handler: Rc<dyn Fn(PushMessage)>) -> MessagingSubscription;
}

/// Clonable handle suitable for `AppState` and [`MessagingMasamuneAdapter`].
#[derive(Clone)]
pub struct Messaging(Rc<dyn MessagingAdapter>);

impl std::fmt::Debug for Messaging {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Messaging")
    }
}

impl Messaging {
    pub fn new(adapter: impl MessagingAdapter + 'static) -> Self {
        Self(Rc::new(adapter))
    }

    pub fn request_permission(&self) -> MessagingFuture<'_, bool> {
        self.0.request_permission()
    }

    pub fn token(&self) -> MessagingFuture<'_, Option<String>> {
        self.0.token()
    }

    pub fn delete_token(&self) -> MessagingFuture<'_, ()> {
        self.0.delete_token()
    }

    pub fn initial_message(&self) -> MessagingFuture<'_, Option<PushMessage>> {
        self.0.initial_message()
    }

    pub fn on_token_refresh(&self, handler: impl Fn(String) + 'static) -> MessagingSubscription {
        self.0.on_token_refresh(Rc::new(handler))
    }

    pub fn on_message(&self, handler: impl Fn(PushMessage) + 'static) -> MessagingSubscription {
        self.0.on_message(Rc::new(handler))
    }

    pub fn on_message_opened(
        &self,
        handler: impl Fn(PushMessage) + 'static,
    ) -> MessagingSubscription {
        self.0.on_message_opened(Rc::new(handler))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[tokio::test]
    async fn runtime_adapter_handles_tokens_and_messages() {
        let runtime = RuntimeMessagingAdapter::new()
            .with_permission(true)
            .with_token("runtime-fcm-token");
        let messaging = Messaging::new(runtime.clone());

        assert!(messaging.request_permission().await.unwrap());
        assert_eq!(
            messaging.token().await.unwrap().as_deref(),
            Some("runtime-fcm-token")
        );

        let received = Rc::new(RefCell::new(Vec::new()));
        let _sub = messaging.on_message({
            let received = received.clone();
            move |msg| received.borrow_mut().push(msg)
        });
        let sample = PushMessage {
            id: Some("m1".into()),
            title: Some("Ready".into()),
            body: Some("Done".into()),
            data: BTreeMap::from([("gift_id".into(), "g1".into())]),
        };
        runtime.emit_message(sample.clone());
        assert_eq!(*received.borrow(), vec![sample]);

        messaging.delete_token().await.unwrap();
        assert_eq!(messaging.token().await.unwrap(), None);
        assert_eq!(runtime.deleted_count(), 1);
    }
}
