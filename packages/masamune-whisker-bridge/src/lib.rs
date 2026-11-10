//! masamune-whisker-* モジュール共通の受け渡し。
//!
//! 各モジュールの Host 実装（Kotlin・Swift・Web）とは JSON 文字列でやり取りする。
//! 引数は `[Str(json)]`、結果は `{"value": ...}` の JSON 文字列、失敗は `WhiskerValue::Error`。
//! イベントのペイロードも JSON 文字列。

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use whisker::PlatformModule;
use whisker::WhiskerValue;

/// ネイティブ呼び出しの失敗。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeError(pub String);

impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NativeError {}

pub type NativeResult<T> = Result<T, NativeError>;

/// 1 つの Host モジュールへの呼び出し口。
///
/// 名前は `<crate 名>:<モジュール名>`（KSP の `whisker.crateName` と Kotlin の `Name(...)`）。
#[derive(Debug, Clone)]
pub struct NativeModule {
    name: String,
}

impl NativeModule {
    /// `crate_name` には呼び出し側 crate の `env!("CARGO_PKG_NAME")` を渡す。
    pub fn new(crate_name: &str, module_name: &str) -> Self {
        Self {
            name: format!("{crate_name}:{module_name}"),
        }
    }

    fn platform(&self) -> PlatformModule {
        PlatformModule::named(self.name.clone())
    }

    /// 同期呼び出し（メインスレッドで即座に終わる処理だけに使う）。
    pub fn call<A: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        args: &A,
    ) -> NativeResult<T> {
        let args = encode_args(args)?;
        decode_result(self.platform().invoke(method, args))
    }

    /// 非同期呼び出し（Host 側は `AsyncFunction` で promise を解決する）。
    pub async fn call_async<A: Serialize, T: DeserializeOwned>(
        &self,
        method: &str,
        args: &A,
    ) -> NativeResult<T> {
        let args = encode_args(args)?;
        decode_result(self.platform().invoke_async(method, args).await)
    }

    /// イベントを JSON として受け取る。戻り値を保持している間だけ通知される。
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

/// イベント購読。破棄すると購読が止まる。
pub struct Subscription {
    _inner: Box<dyn std::any::Any>,
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
