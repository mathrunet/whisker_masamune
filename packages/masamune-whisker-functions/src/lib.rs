#![doc = include_str!("../README.md")]

//! Typed server actions, inspired by `katana_functions/lib/src/{functions,functions_action,functions_adapter}.dart`.
//! See the crate README for setup, runtime adapters, and backend switching.

mod error;
mod retry;

use std::{future::Future, pin::Pin, rc::Rc, sync::OnceLock, time::Duration};

pub use error::{ApiError, ApiResult};
pub use masamune_whisker_auth::{Auth, AuthAdapter, AuthError, AuthFuture, AuthResult};
pub use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
pub use reqwest::{Method, Url};
pub use retry::{RetryPolicy, retry, with_timeout};
use serde::de::DeserializeOwned;
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

/// Structured Firebase callable errors (status, message and application details).
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

impl From<AuthError> for CallableFailure {
    fn from(error: AuthError) -> Self {
        Self::Transport(error.into())
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

/// Preserves existing backend errors, including structured Firebase failures.
#[derive(Clone, Debug, PartialEq)]
pub enum FunctionsError {
    Api(ApiError),
    Callable(Box<CallableFailure>),
    Configuration(String),
}

impl From<ApiError> for FunctionsError {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl From<AuthError> for FunctionsError {
    fn from(error: AuthError) -> Self {
        Self::Api(error.into())
    }
}

impl From<CallableFailure> for FunctionsError {
    fn from(error: CallableFailure) -> Self {
        match error {
            CallableFailure::Transport(error) => Self::Api(error),
            other => Self::Callable(Box::new(other)),
        }
    }
}

/// Flattens adapter errors for backend-neutral callers (model stores, storage adapters).
/// Remote callable errors keep their HTTP status so 404/403 checks still work.
impl From<FunctionsError> for ApiError {
    fn from(error: FunctionsError) -> Self {
        match error {
            FunctionsError::Api(error) => error,
            FunctionsError::Callable(failure) => match *failure {
                CallableFailure::Transport(error) => error,
                CallableFailure::Remote {
                    http_status,
                    status,
                    message,
                    ..
                } => ApiError::Http {
                    status: http_status,
                    body: format!("{status}: {message}"),
                },
            },
            FunctionsError::Configuration(message) => ApiError::Other(message),
        }
    }
}

impl std::fmt::Display for FunctionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(error) => error.fmt(f),
            Self::Callable(error) => error.fmt(f),
            Self::Configuration(message) => write!(f, "configuration: {message}"),
        }
    }
}

impl std::error::Error for FunctionsError {}

pub type FunctionsResult<T> = Result<T, FunctionsError>;
/// Futures run on the Whisker UI thread and do not require `Send`.
pub type FunctionsFuture<'a> = Pin<Box<dyn Future<Output = FunctionsResult<Value>> + 'a>>;

/// Transport-neutral request. The default method is POST and the default body is absent.
#[derive(Clone, Debug)]
pub struct FunctionRequest {
    pub action: String,
    pub path: Option<String>,
    /// Cloudflare: absent/`edge` or `region`. Other adapters ignore this field.
    pub worker_type: Option<String>,
    pub method: Method,
    pub query: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub timeout: Option<Duration>,
    pub body: Option<Value>,
}

impl FunctionRequest {
    pub fn new(action: impl Into<String>) -> Self {
        Self {
            action: action.into(),
            path: None,
            worker_type: None,
            method: Method::POST,
            query: Vec::new(),
            headers: HeaderMap::new(),
            timeout: None,
            body: None,
        }
    }

    /// Sets `path` from percent-encoded segments (`["db", "a b"]` → `db/a%20b`).
    pub fn with_segments<S: AsRef<str>>(mut self, segments: &[S]) -> Self {
        let mut url = Url::parse("http://localhost/").expect("static url");
        url.path_segments_mut()
            .expect("http url has a path")
            .clear()
            .extend(segments.iter().map(AsRef::as_ref));
        self.path = Some(url.path().trim_start_matches('/').to_owned());
        self
    }

    pub fn with_method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((key.into(), value.into()));
        self
    }

    pub fn with_body(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_worker_type(mut self, worker_type: impl Into<String>) -> Self {
        self.worker_type = Some(worker_type.into());
        self
    }
}

/// An application operation with a typed response and optional custom error handling.
/// Adapted from `katana_functions/lib/src/functions_action.dart`.
pub trait FunctionsAction {
    type Response: DeserializeOwned;
    fn request(&self) -> FunctionsResult<FunctionRequest>;
    fn decode(&self, value: Value) -> FunctionsResult<Self::Response> {
        serde_json::from_value(value).map_err(|e| ApiError::Decode(e.to_string()).into())
    }
    fn on_error(&self, error: FunctionsError) -> FunctionsResult<Self::Response> {
        Err(error)
    }
}

/// Object-safe backend interface; inject it at application startup.
pub trait FunctionsAdapter {
    fn send(&self, request: FunctionRequest) -> FunctionsFuture<'_>;
}

/// Clonable handle suitable for AppState; no process-global adapter or implicit fallback.
#[derive(Clone)]
pub struct Functions(Rc<dyn FunctionsAdapter>);

impl std::fmt::Debug for Functions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Functions")
    }
}

impl Functions {
    pub fn new(adapter: impl FunctionsAdapter + 'static) -> Self {
        Self(Rc::new(adapter))
    }
    pub async fn execute<A: FunctionsAction>(&self, action: A) -> FunctionsResult<A::Response> {
        let result = async {
            let request = action.request()?;
            action.decode(self.0.send(request).await?)
        }
        .await;
        result.or_else(|error| action.on_error(error))
    }
    /// Sends a raw request (used by backend crates such as model stores and storage adapters).
    pub fn send(&self, request: FunctionRequest) -> FunctionsFuture<'_> {
        self.0.send(request)
    }
}

/// Generic JSON REST API adapter with an optional bearer [`Auth`].
/// Adapted from `katana_functions/lib/adapter/rest_api_functions_adapter.dart`.
#[derive(Clone, Debug)]
pub struct RestApiFunctionsAdapter {
    endpoint: String,
    auth: Auth,
    timeout: Duration,
}

impl RestApiFunctionsAdapter {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into().trim_end_matches('/').to_owned(),
            auth: Auth::none(),
            timeout: Duration::from_secs(30),
        }
    }
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.auth = auth;
        self
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Managed authentication and JSON headers take precedence over request headers.
    /// The deadline includes token acquisition as well as the HTTP exchange.
    async fn dispatch(&self, request: FunctionRequest) -> FunctionsResult<Value> {
        let path = request.path.as_deref().unwrap_or(&request.action);
        let mut url = Url::parse(&format!(
            "{}/{}",
            self.endpoint,
            path.trim_start_matches('/')
        ))
        .map_err(|e| FunctionsError::Configuration(e.to_string()))?;
        if !request.query.is_empty() {
            url.query_pairs_mut().extend_pairs(&request.query);
        }
        let timeout = request.timeout.unwrap_or(self.timeout);
        let mut headers = request.headers;
        let body = request.body;
        let value = with_timeout(timeout, async {
            headers.remove("accept");
            let token = self.auth.token(false).await?;
            if token.is_some() {
                headers.remove("authorization");
            }
            if body.is_some() {
                headers.remove("content-type");
            }
            let mut http = client()
                .request(request.method, url)
                .headers(headers)
                .header("Accept", "application/json")
                .timeout(timeout);
            if let Some(token) = token {
                http = http.bearer_auth(token);
            }
            if let Some(body) = &body {
                http = http.json(body);
            }
            read_json(http.send().await?).await
        })
        .await?;
        Ok(value)
    }
}

impl FunctionsAdapter for RestApiFunctionsAdapter {
    fn send(&self, request: FunctionRequest) -> FunctionsFuture<'_> {
        Box::pin(self.dispatch(request))
    }
}

/// In-memory backend. The callback receives the complete request and can return errors.
/// Adapted from `katana_functions/lib/adapter/runtime_functions_adapter.dart`.
#[derive(Clone)]
pub struct RuntimeFunctionsAdapter(Rc<dyn Fn(FunctionRequest) -> FunctionsFuture<'static>>);

impl RuntimeFunctionsAdapter {
    pub fn new<F, Fut>(handler: F) -> Self
    where
        F: Fn(FunctionRequest) -> Fut + 'static,
        Fut: Future<Output = FunctionsResult<Value>> + 'static,
    {
        Self(Rc::new(move |request| Box::pin(handler(request))))
    }
}

impl FunctionsAdapter for RuntimeFunctionsAdapter {
    fn send(&self, request: FunctionRequest) -> FunctionsFuture<'_> {
        (self.0)(request)
    }
}
