//! HTTP transport to SamuraiAI (`_postToSamuraiAI` / `AIDebugHttpException`).

use futures_util::FutureExt;
use futures_util::future::LocalBoxFuture;
use serde_json::{Map, Value};

/// An AI Debugger failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiDebugError {
    /// A required value (`MASAMUNE_AI_DEBUGGER_*`) is missing or a setting is invalid.
    Config(String),
    /// SamuraiAI answered with a non-2xx status.
    Http { status: u16, message: String },
    /// The request could not be sent or the answer could not be read.
    Transport(String),
}

impl AiDebugError {
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Http { status, .. } => Some(*status),
            _ => None,
        }
    }
}

impl std::fmt::Display for AiDebugError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(message) | Self::Transport(message) => f.write_str(message),
            Self::Http { message, .. } => f.write_str(message),
        }
    }
}

impl std::error::Error for AiDebugError {}

/// Posts JSON and returns the JSON object of a 2xx answer (`{}` when the body is empty).
///
/// Replace it with [`crate::AiDebuggerConfig::transport`] to use another backend or to test.
pub trait AiDebugTransport {
    fn post(
        &self,
        url: String,
        headers: Vec<(String, String)>,
        body: Value,
    ) -> LocalBoxFuture<'static, Result<Map<String, Value>, AiDebugError>>;
}

/// reqwest with a shared client (connect timeout 10 s, overall timeout 30 s; the browser's own
/// timeouts on the web).
pub struct HttpTransport;

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        let builder = reqwest::Client::builder();
        // reqwest's wasm client (`fetch`) has no timeouts.
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30));
        builder.build().expect("http client")
    })
}

impl AiDebugTransport for HttpTransport {
    fn post(
        &self,
        url: String,
        headers: Vec<(String, String)>,
        body: Value,
    ) -> LocalBoxFuture<'static, Result<Map<String, Value>, AiDebugError>> {
        async move {
            let mut request = client().post(&url).json(&body);
            for (name, value) in headers {
                request = request.header(name, value);
            }
            let response = request
                .send()
                .await
                .map_err(|e| AiDebugError::Transport(e.to_string()))?;
            let status = response.status().as_u16();
            let text = response
                .text()
                .await
                .map_err(|e| AiDebugError::Transport(e.to_string()))?;
            decode(status, &text)
        }
        .boxed_local()
    }
}

/// Applies the Flutter package's response rules.
pub(crate) fn decode(status: u16, text: &str) -> Result<Map<String, Value>, AiDebugError> {
    let decoded: Option<Value> = if text.trim().is_empty() {
        Some(Value::Object(Map::new()))
    } else {
        serde_json::from_str(text).ok()
    };
    if !(200..300).contains(&status) {
        let message = decoded
            .as_ref()
            .and_then(|v| v.get("error"))
            .map(|e| match e {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_else(|| format!("AI Debugger HTTP {status}"));
        return Err(AiDebugError::Http { status, message });
    }
    match decoded {
        Some(Value::Object(map)) => Ok(map),
        _ => Err(AiDebugError::Transport(
            "AI Debugger response is not a JSON object".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_responses() {
        assert!(decode(200, "").unwrap().is_empty());
        assert_eq!(
            decode(200, r#"{"sessionId":"s"}"#).unwrap()["sessionId"],
            "s"
        );
        assert_eq!(
            decode(410, r#"{"error":"run expired"}"#),
            Err(AiDebugError::Http {
                status: 410,
                message: "run expired".into()
            })
        );
        assert_eq!(
            decode(500, "oops"),
            Err(AiDebugError::Http {
                status: 500,
                message: "AI Debugger HTTP 500".into()
            })
        );
        assert!(matches!(decode(200, "[]"), Err(AiDebugError::Transport(_))));
    }
}
