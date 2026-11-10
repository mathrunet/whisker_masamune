//! Platform-independent Firebase callable wire protocol shared by Functions adapters.

use std::time::Duration;

use serde_json::{Value, json};

use crate::{ApiError, AuthProvider, HeaderMap, Url, client};

/// Callable errors preserve the server status, message, and application details.
#[derive(Clone, Debug, PartialEq)]
pub enum CallableFailure {
    Transport(ApiError),
    Remote {
        http_status: u16,
        status: String,
        message: String,
        details: Value,
    },
}

impl From<ApiError> for CallableFailure {
    fn from(error: ApiError) -> Self {
        Self::Transport(error)
    }
}

impl std::fmt::Display for CallableFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(error) => error.fmt(f),
            Self::Remote {
                status, message, ..
            } => write!(f, "callable {status}: {message}"),
        }
    }
}

impl std::error::Error for CallableFailure {}

/// Callable transport with injectable auth and App Check token sources.
///
/// No native SDK is required unless a Firebase provider is selected. App Check
/// acquisition failures are returned before sending the request. No retries occur.
#[derive(Clone)]
pub struct CallableClient {
    pub endpoint: String,
    pub auth: AuthProvider,
    pub app_check: Option<AuthProvider>,
    pub timeout: Duration,
}

impl CallableClient {
    /// Creates an unauthenticated client for a project and region.
    pub fn new(project: &str, region: &str) -> Self {
        Self::at_endpoint(format!("https://{region}-{project}.cloudfunctions.net"))
    }

    /// Uses a base URL, including emulator URLs with a project/region path.
    pub fn at_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            auth: AuthProvider::none(),
            app_check: None,
            timeout: Duration::from_secs(60),
        }
    }

    /// Executes a function. Only callable-protocol headers are accepted by Firebase;
    /// custom headers are provided for compatible gateways and emulators.
    pub async fn call(
        &self,
        name: &str,
        data: Value,
        headers: HeaderMap,
        timeout: Option<Duration>,
    ) -> Result<Value, CallableFailure> {
        let timeout = timeout.unwrap_or(self.timeout);
        tokio::time::timeout(timeout, self.call_inner(name, data, headers, timeout))
            .await
            .unwrap_or(Err(CallableFailure::Transport(ApiError::Timeout)))
    }

    async fn call_inner(
        &self,
        name: &str,
        data: Value,
        mut headers: HeaderMap,
        timeout: Duration,
    ) -> Result<Value, CallableFailure> {
        if name.is_empty() || name.contains(['/', '?', '#']) || name == "." || name == ".." {
            return Err(ApiError::Other("invalid callable name".into()).into());
        }
        let mut url = Url::parse(&self.endpoint).map_err(|e| ApiError::Other(e.to_string()))?;
        if url.query().is_some() || url.fragment().is_some() {
            return Err(ApiError::Other(
                "callable endpoint cannot contain a query or fragment".into(),
            )
            .into());
        }
        url.path_segments_mut()
            .map_err(|_| ApiError::Other("invalid callable endpoint".into()))?
            .pop_if_empty()
            .push(name);
        // Providers own these headers; action headers cannot bypass token policy.
        headers.remove("content-type");
        headers.remove("authorization");
        headers.remove("x-firebase-appcheck");
        let mut request = client()
            .post(url)
            .headers(headers)
            .json(&json!({"data": data}))
            .timeout(timeout);
        if let Some(token) = self.auth.token(false).await? {
            request = request.bearer_auth(token);
        }
        if let Some(provider) = &self.app_check {
            let token = provider
                .token(false)
                .await?
                .filter(|token| !token.is_empty())
                .ok_or_else(|| ApiError::Other("required App Check token is missing".into()))?;
            request = request.header("X-Firebase-AppCheck", token);
        }
        let response = request.send().await.map_err(ApiError::from)?;
        let http_status = response.status().as_u16();
        let body = response.text().await.map_err(ApiError::from)?;
        let value: Value = match serde_json::from_str(&body) {
            Ok(value) => value,
            Err(_) if !(200..300).contains(&http_status) => {
                return Err(ApiError::Http {
                    status: http_status,
                    body,
                }
                .into());
            }
            Err(error) => return Err(ApiError::Decode(error.to_string()).into()),
        };
        if let Some(error) = value.get("error") {
            let status = error
                .get("status")
                .and_then(Value::as_str)
                .ok_or_else(|| ApiError::Decode("callable error is missing status".into()))?;
            let message = match error.get("message") {
                None => "",
                Some(Value::String(message)) => message,
                Some(_) => {
                    return Err(ApiError::Decode("invalid callable error message".into()).into());
                }
            };
            return Err(CallableFailure::Remote {
                http_status,
                status: status.into(),
                message: message.into(),
                details: error.get("details").cloned().unwrap_or(Value::Null),
            });
        }
        if !(200..300).contains(&http_status) {
            return Err(ApiError::Http {
                status: http_status,
                body,
            }
            .into());
        }
        // Firebase clients support both keys; explicit null is a valid result.
        value
            .get("data")
            .or_else(|| value.get("result"))
            .cloned()
            .ok_or_else(|| {
                ApiError::Decode("callable response is missing data/result".into()).into()
            })
    }
}
