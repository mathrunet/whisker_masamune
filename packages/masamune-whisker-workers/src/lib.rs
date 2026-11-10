//! HTTP client for the Masamune Cloudflare Workers (and Firebase callable functions).
//!
//! - [`Workers`]: one worker endpoint. Sends JSON with `Authorization: Bearer <token>` from an
//!   [`AuthProvider`] and returns the decoded JSON body.
//! - [`ApiError`]: the error every backend crate (Turso, TiDB, Firestore) shares.
//! - [`RetryPolicy`] / [`retry`] / [`with_timeout`]: retry and deadline helpers.
//! - [`Callable`] (feature `firebase-functions`): Firebase HTTPS callable functions with App Check.
//!
//! Requests run on the app's tokio runtime (Whisker's `tokio` feature).

mod auth;
#[cfg(feature = "firebase-functions")]
mod callable;
mod error;
mod retry;

use std::sync::OnceLock;
use std::time::Duration;

pub use auth::AuthProvider;
#[cfg(feature = "firebase-auth")]
pub use auth::current_uid;
#[cfg(feature = "firebase-functions")]
pub use callable::Callable;
pub use error::{ApiError, ApiResult};
pub use reqwest::Method;
pub use retry::{RetryPolicy, retry, with_timeout};
use serde_json::Value;

/// Shared HTTP client (connect timeout 10 s, overall timeout 30 s unless a request overrides it).
pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("http client")
    })
}

/// Reads a response as JSON. Non-2xx statuses become [`ApiError::Http`]; an empty body is `null`.
pub async fn read_json(response: reqwest::Response) -> ApiResult<Value> {
    let status = response.status();
    let text = response.text().await?;
    if !status.is_success() {
        return Err(ApiError::Http {
            status: status.as_u16(),
            body: text,
        });
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| ApiError::Decode(e.to_string()))
}

/// One Cloudflare Worker endpoint (for example `https://api-edge.example.com`).
#[derive(Clone)]
pub struct Workers {
    base: String,
    auth: AuthProvider,
    timeout: Duration,
}

impl std::fmt::Debug for Workers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workers")
            .field("base", &self.base)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl Workers {
    /// Endpoint without authentication and a 30 s timeout.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base: base_url.into().trim_end_matches('/').to_owned(),
            auth: AuthProvider::none(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Sends the token of `auth` with every request.
    pub fn with_auth(mut self, auth: AuthProvider) -> Self {
        self.auth = auth;
        self
    }

    /// Default timeout of requests that do not pass their own.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn auth(&self) -> &AuthProvider {
        &self.auth
    }

    /// `base/path` (a leading `/` in `path` is ignored).
    pub fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }

    /// URL from percent-encoded path segments and query parameters.
    pub fn url_with(&self, segments: &[&str], query: &[(&str, String)]) -> ApiResult<reqwest::Url> {
        let mut url =
            reqwest::Url::parse(&self.base).map_err(|e| ApiError::Other(e.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| ApiError::Other(format!("invalid base url: {}", self.base)))?
            .pop_if_empty()
            .extend(segments);
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }

    /// `POST base/path` with a JSON body and the default timeout.
    pub async fn post(&self, path: &str, body: &Value) -> ApiResult<Value> {
        self.post_with_timeout(path, body, self.timeout).await
    }

    /// `POST base/path` with a JSON body.
    pub async fn post_with_timeout(
        &self,
        path: &str,
        body: &Value,
        timeout: Duration,
    ) -> ApiResult<Value> {
        let url =
            reqwest::Url::parse(&self.url(path)).map_err(|e| ApiError::Other(e.to_string()))?;
        self.send(Method::POST, url, Some(body), timeout).await
    }

    /// Sends one request to `url` with authentication and returns the JSON body.
    pub async fn send(
        &self,
        method: Method,
        url: reqwest::Url,
        body: Option<&Value>,
        timeout: Duration,
    ) -> ApiResult<Value> {
        let mut request = client()
            .request(method, url)
            .header("Accept", "application/json")
            .timeout(timeout);
        if let Some(token) = self.auth.token(false).await? {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        read_json(request.send().await?).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn posts_json_with_bearer_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/delete_user"))
            .and(header("authorization", "Bearer t"))
            .and(body_json(json!({"userId": "u"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
            .mount(&server)
            .await;
        let workers =
            Workers::new(format!("{}/", server.uri())).with_auth(AuthProvider::fixed("t"));
        let value = workers
            .post("/delete_user", &json!({"userId": "u"}))
            .await
            .unwrap();
        assert_eq!(value, json!({"ok": true}));
    }

    #[tokio::test]
    async fn maps_http_errors_and_empty_bodies() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/fail"))
            .respond_with(ResponseTemplate::new(503).set_body_string("down"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/empty"))
            .respond_with(ResponseTemplate::new(202))
            .mount(&server)
            .await;
        let workers = Workers::new(server.uri());
        let error = workers.post("fail", &json!({})).await.unwrap_err();
        assert_eq!(
            error,
            ApiError::Http {
                status: 503,
                body: "down".into()
            }
        );
        assert!(error.is_transient());
        assert_eq!(
            workers.post("empty", &json!({})).await.unwrap(),
            Value::Null
        );
    }

    #[tokio::test]
    async fn builds_encoded_urls() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/turso/database/u%201/user"))
            .and(query_param("where", "[1]"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
            .mount(&server)
            .await;
        let workers = Workers::new(server.uri());
        let url = workers
            .url_with(
                &["turso", "database", "u 1", "user"],
                &[("where", "[1]".into())],
            )
            .unwrap();
        let value = workers
            .send(Method::GET, url, None, Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(value, json!({"data": []}));
    }
}
