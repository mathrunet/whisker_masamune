//! Shared data exchange for the masamune-whisker-* modules.
//!
//! Each module talks to its host implementation (Kotlin, Swift, Web) with JSON strings.
//! Arguments are `[Str(json)]`, results are a `{"value": ...}` JSON string, and failures are `WhiskerValue::Error`.
//! Event payloads are JSON strings as well.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use whisker::PlatformModule;
use whisker::WhiskerValue;

/// A failed native call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeError(pub String);

impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NativeError {}

pub type NativeResult<T> = Result<T, NativeError>;

/// Entry point for calls to a single host module.
///
/// The name is `<crate name>:<module name>` (KSP's `whisker.crateName` and Kotlin's `Name(...)`).
#[derive(Debug, Clone)]
pub struct NativeModule {
    name: String,
}

impl NativeModule {
    /// Pass the calling crate's `env!("CARGO_PKG_NAME")` as `crate_name`.
    pub fn new(crate_name: &str, module_name: &str) -> Self {
        Self {
            name: format!("{crate_name}:{module_name}"),
        }
    }

    fn platform(&self) -> PlatformModule {
        PlatformModule::named(self.name.clone())
    }

    /// Synchronous call (use only for work that finishes immediately on the main thread).
    pub fn call<A: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        args: &A,
    ) -> NativeResult<T> {
        let args = encode_args(args)?;
        decode_result(self.platform().invoke(method, args))
    }

    /// Asynchronous call (the host resolves the promise in an `AsyncFunction`).
    pub async fn call_async<A: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        args: &A,
    ) -> NativeResult<T> {
        let args = encode_args(args)?;
        decode_result(self.platform().invoke_async(method, args).await)
    }

    /// Receives events as JSON. Events are delivered only while the returned value is held.
    pub fn subscribe<T: DeserializeOwned + 'static>(
        &self,
        event: &str,
        handler: impl Fn(T) + 'static,
    ) -> Subscription {
        let sub = self.platform().on_event(event, move |payload| {
            let value = match payload {
                WhiskerValue::String(json) => serde_json::from_str::<T>(&json).ok(),
                _ => None,
            };
            if let Some(value) = value {
                handler(value);
            }
        });
        Subscription {
            _inner: Box::new(sub),
        }
    }
}

/// An event subscription. Dropping it stops the subscription.
pub struct Subscription {
    _inner: Box<dyn std::any::Any>,
}

impl Subscription {
    /// Wraps any value whose drop ends a subscription (for example a native subscription
    /// combined with a Rust-side listener).
    pub fn new(inner: impl std::any::Any) -> Self {
        Self {
            _inner: Box::new(inner),
        }
    }
}

fn encode_args<A: Serialize>(args: &A) -> NativeResult<Vec<WhiskerValue>> {
    let json = serde_json::to_string(args).map_err(|e| NativeError(e.to_string()))?;
    Ok(vec![WhiskerValue::String(json)])
}

fn decode_result<T: DeserializeOwned>(value: WhiskerValue) -> NativeResult<T> {
    match value {
        WhiskerValue::String(json) => {
            let mut root: Value =
                serde_json::from_str(&json).map_err(|e| NativeError(e.to_string()))?;
            let inner = root
                .get_mut("value")
                .map(Value::take)
                .unwrap_or(Value::Null);
            serde_json::from_value(inner).map_err(|e| NativeError(e.to_string()))
        }
        WhiskerValue::Error(message) => Err(NativeError(message)),
        WhiskerValue::Null => {
            serde_json::from_value(Value::Null).map_err(|e| NativeError(e.to_string()))
        }
        other => Err(NativeError(format!("unexpected native result: {other:?}"))),
    }
}
