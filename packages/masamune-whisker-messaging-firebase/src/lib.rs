#![doc = include_str!("../README.md")]

pub mod cng;

use std::rc::Rc;

use masamune_whisker_bridge::{NativeError, NativeModule};
pub use masamune_whisker_messaging::{
    Messaging, MessagingAdapter, MessagingError, MessagingFuture, MessagingMasamuneAdapter,
    MessagingResult, MessagingSubscription, PushMessage,
};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseMessaging")
}

fn map_error(error: NativeError) -> MessagingError {
    let msg = error.0;
    if msg.contains("unsupported") || msg.contains("not registered") {
        MessagingError::Unsupported(msg)
    } else if msg.contains("denied") {
        MessagingError::PermissionDenied
    } else {
        MessagingError::Backend(msg)
    }
}

/// Firebase Cloud Messaging (FCM) [`MessagingAdapter`] backed by the native iOS / Android SDK.
#[derive(Debug, Clone, Copy, Default)]
pub struct FirebaseMessagingAdapter;

impl FirebaseMessagingAdapter {
    pub fn new() -> Self {
        Self
    }
}

/// Creates a [`Messaging`] handle backed by Firebase Cloud Messaging.
pub fn firebase_messaging() -> Messaging {
    Messaging::new(FirebaseMessagingAdapter::new())
}

/// Creates a [`MessagingMasamuneAdapter`] backed by Firebase Cloud Messaging.
pub fn firebase_messaging_adapter() -> MessagingMasamuneAdapter {
    MessagingMasamuneAdapter::new(firebase_messaging())
}

impl MessagingAdapter for FirebaseMessagingAdapter {
    fn request_permission(&self) -> MessagingFuture<'_, bool> {
        Box::pin(async {
            module()
                .call_async::<_, bool>("requestPermission", &json!({}))
                .await
                .map_err(map_error)
        })
    }

    fn token(&self) -> MessagingFuture<'_, Option<String>> {
        Box::pin(async {
            module()
                .call_async::<_, Option<String>>("getToken", &json!({}))
                .await
                .map(|token| token.filter(|value| !value.is_empty()))
                .map_err(map_error)
        })
    }

    fn delete_token(&self) -> MessagingFuture<'_, ()> {
        Box::pin(async {
            let _: serde_json::Value = module()
                .call_async("deleteToken", &json!({}))
                .await
                .map_err(map_error)?;
            Ok(())
        })
    }

    fn initial_message(&self) -> MessagingFuture<'_, Option<PushMessage>> {
        Box::pin(async {
            module()
                .call_async::<_, Option<PushMessage>>("getInitialMessage", &json!({}))
                .await
                .map_err(map_error)
        })
    }

    fn on_token_refresh(&self, handler: Rc<dyn Fn(String)>) -> MessagingSubscription {
        let sub = module().subscribe("tokenRefresh", move |token: String| {
            if !token.is_empty() {
                handler(token);
            }
        });
        MessagingSubscription::new(sub)
    }

    fn on_message(&self, handler: Rc<dyn Fn(PushMessage)>) -> MessagingSubscription {
        let sub = module().subscribe("message", move |message: PushMessage| {
            handler(message);
        });
        MessagingSubscription::new(sub)
    }

    fn on_message_opened(&self, handler: Rc<dyn Fn(PushMessage)>) -> MessagingSubscription {
        let sub = module().subscribe("messageOpened", move |message: PushMessage| {
            handler(message);
        });
        MessagingSubscription::new(sub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use whisker::WhiskerValue;
    use whisker::runtime::module::{ModuleHost, with_module_host};
    use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};
    use whisker::{Owner, spawn_local};

    #[test]
    fn bridges_native_token_and_permission_calls() {
        let calls: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let host = ModuleHost::new(
            {
                let calls = calls.clone();
                move |module_name, method, _, _, result| {
                    assert_eq!(
                        module_name,
                        "masamune-whisker-messaging-firebase:FirebaseMessaging"
                    );
                    calls.borrow_mut().push(method.to_owned());
                    let payload = match method {
                        "requestPermission" => r#"{"value":true}"#,
                        "getToken" => r#"{"value":"fcm-token-1"}"#,
                        "deleteToken" => r#"{"value":true}"#,
                        "getInitialMessage" => {
                            r#"{"value":{"id":"m1","title":"Title","body":"Body","data":{"gift_id":"g1"}}}"#
                        }
                        _ => r#"{"value":null}"#,
                    };
                    result(WhiskerValue::String(payload.into()));
                    true
                }
            },
            |_, _, _| {},
        );
        let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
        runtime.enter(|| {
            with_module_host(&host, || {
                let root = Owner::new(None);
                root.with(|| {
                    spawn_local(async {
                        let messaging = firebase_messaging();
                        assert!(messaging.request_permission().await.unwrap());
                        assert_eq!(
                            messaging.token().await.unwrap().as_deref(),
                            Some("fcm-token-1")
                        );
                        let msg = messaging.initial_message().await.unwrap().unwrap();
                        assert_eq!(msg.id.as_deref(), Some("m1"));
                        assert_eq!(msg.data.get("gift_id").map(String::as_str), Some("g1"));
                        messaging.delete_token().await.unwrap();
                    });
                });
                whisker::run_until_stalled();
                root.dispose();
            });
        });
        runtime.shutdown();
        assert_eq!(
            *calls.borrow(),
            [
                "requestPermission",
                "getToken",
                "getInitialMessage",
                "deleteToken"
            ]
        );
    }
}
