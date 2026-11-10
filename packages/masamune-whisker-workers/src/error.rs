/// Failure of a server request.
#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    /// No signed-in user while the request needs one.
    NotSignedIn,
    /// Non-2xx response (status and body).
    Http { status: u16, body: String },
    /// The request could not reach the server (offline, DNS, connection refused, …).
    Network(String),
    /// The request or the caller's deadline expired.
    Timeout,
    /// The response had an unexpected shape.
    Decode(String),
    /// Anything else.
    Other(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::NotSignedIn => f.write_str("not signed in"),
            ApiError::Http { status, body } => write!(f, "http {status}: {body}"),
            ApiError::Network(message) => write!(f, "network: {message}"),
            ApiError::Timeout => f.write_str("timeout"),
            ApiError::Decode(message) => write!(f, "decode: {message}"),
            ApiError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    /// Whether retrying may succeed (network failures, timeouts, 5xx).
    pub fn is_transient(&self) -> bool {
        match self {
            ApiError::Network(_) | ApiError::Timeout => true,
            ApiError::Http { status, .. } => *status >= 500,
            _ => false,
        }
    }

    /// Whether the device could not reach the server.
    pub fn is_offline(&self) -> bool {
        matches!(self, ApiError::Network(_))
    }

    /// HTTP status of an [`ApiError::Http`].
    pub fn status(&self) -> Option<u16> {
        match self {
            ApiError::Http { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Whether the server answered 404.
    pub fn is_not_found(&self) -> bool {
        self.status() == Some(404)
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            ApiError::Timeout
        } else if error.is_connect() || error.is_request() {
            ApiError::Network(error.to_string())
        } else if error.is_decode() {
            ApiError::Decode(error.to_string())
        } else {
            ApiError::Other(error.to_string())
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
