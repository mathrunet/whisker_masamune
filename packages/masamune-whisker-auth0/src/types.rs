use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

/// Scopes requested when [`Auth0Config::scopes`] is not changed.
pub const DEFAULT_SCOPES: &[&str] = &["openid", "profile", "email", "offline_access"];

/// Seconds of validity a renewed or restored access token must have left.
pub const DEFAULT_MIN_TTL_SECS: u32 = 60;

/// Auth0 application settings.
///
/// `scheme` is the custom URL scheme of the callback URL
/// (`{scheme}://{domain}/ios/{bundle id}/callback` on iOS and
/// `{scheme}://{domain}/android/{application id}/callback` on Android). Register both URLs as
/// Allowed Callback URLs and Allowed Logout URLs of the Auth0 application, and the scheme with
/// the [`crate::cng::MasamuneAuth0`] plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth0Config {
    /// Tenant domain, for example `example.jp.auth0.com`.
    pub domain: String,
    /// Client ID of the native application.
    pub client_id: String,
    /// Callback URL scheme (lowercase), usually the bundle / application id.
    pub scheme: String,
    /// Requested scopes. `offline_access` is required for refresh tokens.
    pub scopes: Vec<String>,
}

impl Auth0Config {
    /// Settings with [`DEFAULT_SCOPES`].
    pub fn new(
        domain: impl Into<String>,
        client_id: impl Into<String>,
        scheme: impl Into<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            client_id: client_id.into(),
            scheme: scheme.into(),
            scopes: DEFAULT_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    /// Replaces the requested scopes.
    pub fn with_scopes<I, S>(mut self, scopes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scopes = scopes.into_iter().map(Into::into).collect();
        self
    }

    /// The scopes as one space separated string.
    pub fn scope(&self) -> String {
        self.scopes.join(" ")
    }
}

/// Profile claims of the ID token.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth0User {
    /// The `sub` claim (user ID).
    pub sub: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Tokens of a signed-in user, kept in the iOS Keychain / Android Keystore by the native
/// credentials manager.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth0Credentials {
    pub access_token: String,
    pub id_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Expiry of the access token in milliseconds since the Unix epoch.
    pub expires_at_ms: i64,
    #[serde(default)]
    pub scope: Option<String>,
    pub user: Auth0User,
}

impl std::fmt::Debug for Auth0Credentials {
    // Tokens are never printed.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth0Credentials")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("has_refresh_token", &self.refresh_token.is_some())
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

impl Auth0Credentials {
    /// Whether the access token expires within `min_ttl_secs` of `now_ms`.
    pub fn expires_within(&self, now_ms: i64, min_ttl_secs: u32) -> bool {
        self.expires_at_ms <= now_ms + i64::from(min_ttl_secs) * 1000
    }

    /// Whether a refresh token is available.
    pub fn can_renew(&self) -> bool {
        self.refresh_token.as_deref().is_some_and(|t| !t.is_empty())
    }
}

/// Failure of an Auth0 operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth0Error {
    /// The user closed the login screen.
    Cancelled,
    /// No credentials are stored.
    NoCredentials,
    /// The native SDK failed (`code` is the SDK's error code when known).
    Native { code: String, message: String },
    /// Anything else.
    Other(String),
}

impl std::fmt::Display for Auth0Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("auth0: cancelled"),
            Self::NoCredentials => f.write_str("auth0: no credentials"),
            Self::Native { code, message } => write!(f, "auth0 {code}: {message}"),
            Self::Other(message) => write!(f, "auth0: {message}"),
        }
    }
}

impl std::error::Error for Auth0Error {}

pub type Auth0Result<T> = Result<T, Auth0Error>;

/// Futures run on the Whisker UI thread and do not require `Send`.
pub type Auth0Future<'a, T> = Pin<Box<dyn Future<Output = Auth0Result<T>> + 'a>>;

/// Current time in milliseconds since the Unix epoch.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
