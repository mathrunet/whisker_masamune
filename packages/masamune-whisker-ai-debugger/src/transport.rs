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
///
/// Idle connections are dropped after 15 s, and a request that fails before it is sent (a refused
/// connection or a pooled connection the tunnel already closed) is sent once more.
pub struct HttpTransport;

fn client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        let builder = reqwest::Client::builder();
        // reqwest's wasm client (`fetch`) has no timeouts and no connection pool.
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            // Tunnels such as Tailscale Serve close idle keep-alive connections; reusing one
            // fails with "error sending request".
            .pool_idle_timeout(std::time::Duration::from_secs(15));
        builder.build().expect("http client")
    })
}

/// The error message followed by its `source()` chain, so that the cause (connection closed,
/// DNS, TLS, …) is not lost behind reqwest's "error sending request for url (…)".
pub(crate) fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

/// Whether the request failed before the server could answer it, so sending it again is safe.
fn is_retryable(error: &reqwest::Error) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        !error.is_timeout() && (error.is_connect() || error.is_request())
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = error;
        false
    }
}

async fn send(
    url: &str,
    headers: &[(String, String)],
    body: &Value,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut request = client().post(url).json(body);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    request.send().await
}

impl AiDebugTransport for HttpTransport {
    fn post(
        &self,
        url: String,
        headers: Vec<(String, String)>,
        body: Value,
    ) -> LocalBoxFuture<'static, Result<Map<String, Value>, AiDebugError>> {
        async move {
            let response = match send(&url, &headers, &body).await {
                Err(error) if is_retryable(&error) => {
                    log::debug!(
                        target: crate::controller::LOG_TARGET,
                        "AI Debugger request failed, retrying once: {}",
                        error_chain(&error)
                    );
                    send(&url, &headers, &body).await
                }
                result => result,
            }
            .map_err(|e| {
                let message = error_chain(&e);
                log::warn!(target: crate::controller::LOG_TARGET, "AI Debugger request failed: {message}");
                AiDebugError::Transport(message)
            })?;
            let status = response.status().as_u16();
            let text = response
                .text()
                .await
                .map_err(|e| AiDebugError::Transport(error_chain(&e)))?;
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

    #[test]
    fn error_chain_appends_causes() {
        #[derive(Debug)]
        struct Outer(std::io::Error);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("error sending request")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let error = Outer(std::io::Error::other("connection closed"));
        assert_eq!(
            error_chain(&error),
            "error sending request: connection closed"
        );
    }

    /// The first connection is closed without an answer (a stale pooled connection); the
    /// retry gets the answer.
    #[tokio::test]
    async fn retries_a_connection_closed_before_the_answer() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/api/app-debug/runs",
            listener.local_addr().unwrap()
        );
        let server = std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            let (mut first, _) = listener.accept().unwrap();
            let _ = first.read(&mut buffer);
            drop(first);
            let (mut second, _) = listener.accept().unwrap();
            let _ = second.read(&mut buffer);
            let body = r#"{"ok":true}"#;
            write!(
                second,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let answer = HttpTransport
            .post(url, vec![], serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(answer["ok"], true);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn transport_errors_keep_the_cause() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        drop(listener);
        let Err(AiDebugError::Transport(message)) =
            HttpTransport.post(url, vec![], serde_json::json!({})).await
        else {
            panic!("expected a transport error");
        };
        assert!(message.starts_with("error sending request"), "{message}");
        assert!(
            message.len() > "error sending request for url ()".len() + 20,
            "{message}"
        );
    }
}
