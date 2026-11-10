#![doc = include_str!("../README.md")]

//! Typed server actions, inspired by `katana_functions/lib/src/{functions,functions_action,functions_adapter}.dart`.
//! See the crate README for setup, runtime adapters, and backend switching.

use std::{future::Future, pin::Pin, rc::Rc, time::Duration};

use masamune_whisker_workers::Workers;
pub use masamune_whisker_workers::{
    ApiError, AuthProvider, CallableFailure, HeaderMap, HeaderName, HeaderValue, Method, Url,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

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

impl From<CallableFailure> for FunctionsError {
    fn from(error: CallableFailure) -> Self {
        match error {
            CallableFailure::Transport(error) => Self::Api(error),
            other => Self::Callable(Box::new(other)),
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
}

/// Generic JSON REST API adapter, reusing the existing Workers transport.
/// Adapted from `katana_functions/lib/adapter/rest_api_functions_adapter.dart`.
#[derive(Clone, Debug)]
pub struct RestApiFunctionsAdapter {
    workers: Workers,
    timeout: Duration,
}

impl RestApiFunctionsAdapter {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            workers: Workers::new(endpoint),
            timeout: Duration::from_secs(30),
        }
    }
    pub fn with_auth(mut self, auth: AuthProvider) -> Self {
        self.workers = self.workers.with_auth(auth);
        self
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl FunctionsAdapter for RestApiFunctionsAdapter {
    fn send(&self, request: FunctionRequest) -> FunctionsFuture<'_> {
        Box::pin(async move {
            let path = request.path.as_deref().unwrap_or(&request.action);
            let mut url = Url::parse(&self.workers.url(path))
                .map_err(|e| FunctionsError::Configuration(e.to_string()))?;
            if !request.query.is_empty() {
                url.query_pairs_mut().extend_pairs(&request.query);
            }
            Ok(self
                .workers
                .send_with_headers(
                    request.method,
                    url,
                    request.body.as_ref(),
                    request.timeout.unwrap_or(self.timeout),
                    request.headers,
                )
                .await?)
        })
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
