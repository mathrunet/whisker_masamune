#![doc = include_str!("../README.md")]

//! Authentication interface inspired by `katana_auth/lib/src/auth_adapter.dart`.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

/// Failure while resolving the signed-in user's token.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthError {
    /// No signed-in user while a token is required.
    NotSignedIn,
    /// Anything else (SDK failures, refresh errors, …).
    Other(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::NotSignedIn => f.write_str("not signed in"),
            AuthError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for AuthError {}

pub type AuthResult<T> = Result<T, AuthError>;
/// Futures run on the Whisker UI thread and do not require `Send`.
pub type AuthFuture<'a> = Pin<Box<dyn Future<Output = AuthResult<Option<String>>> + 'a>>;

/// Backend of the signed-in user. Implementations live in separate crates
/// (for example `masamune-whisker-auth-firebase`) so consumers only depend on this interface.
pub trait AuthAdapter {
    /// Access token sent as `Authorization: Bearer`. `Ok(None)` sends no header.
    fn access_token(&self, force_refresh: bool) -> AuthFuture<'_>;
    /// ID of the signed-in user, when known.
    fn user_id(&self) -> Option<String> {
        None
    }
}

/// Clonable handle of an [`AuthAdapter`], injected into adapters that need tokens.
#[derive(Clone)]
pub struct Auth(Rc<dyn AuthAdapter>);

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Auth")
    }
}

impl Default for Auth {
    fn default() -> Self {
        Self::none()
    }
}

impl Auth {
    pub fn new(adapter: impl AuthAdapter + 'static) -> Self {
        Self(Rc::new(adapter))
    }

    /// Sends no token.
    pub fn none() -> Self {
        Self::new(RuntimeAuthAdapter::default())
    }

    /// Always sends `token` (tests, server tokens).
    pub fn fixed(token: impl Into<String>) -> Self {
        Self::new(RuntimeAuthAdapter::default().with_token(token))
    }

    /// Custom token source. The closure receives `force_refresh`.
    pub fn from_fn<F, Fut>(f: F) -> Self
    where
        F: Fn(bool) -> Fut + 'static,
        Fut: Future<Output = AuthResult<Option<String>>> + 'static,
    {
        Self::new(FnAuthAdapter(Box::new(move |force| Box::pin(f(force)))))
    }

    /// Resolves the access token.
    pub async fn token(&self, force_refresh: bool) -> AuthResult<Option<String>> {
        self.0.access_token(force_refresh).await
    }

    /// ID of the signed-in user, when known.
    pub fn user_id(&self) -> Option<String> {
        self.0.user_id()
    }
}

/// In-memory adapter with a fixed token and user ID (development and tests).
/// Adapted from `katana_auth/lib/adapter/runtime_auth_adapter.dart`.
#[derive(Clone, Debug, Default)]
pub struct RuntimeAuthAdapter {
    token: Option<String>,
    user_id: Option<String>,
}

impl RuntimeAuthAdapter {
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }
}

impl AuthAdapter for RuntimeAuthAdapter {
    fn access_token(&self, _force_refresh: bool) -> AuthFuture<'_> {
        let token = self.token.clone();
        Box::pin(async move { Ok(token) })
    }

    fn user_id(&self) -> Option<String> {
        self.user_id.clone()
    }
}

type TokenFn = Box<dyn Fn(bool) -> AuthFuture<'static>>;

struct FnAuthAdapter(TokenFn);

impl AuthAdapter for FnAuthAdapter {
    fn access_token(&self, force_refresh: bool) -> AuthFuture<'_> {
        (self.0)(force_refresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test]
    async fn resolves_tokens_and_user_ids() {
        assert_eq!(Auth::none().token(false).await, Ok(None));
        assert_eq!(Auth::fixed("t").token(true).await, Ok(Some("t".into())));
        let runtime = Auth::new(RuntimeAuthAdapter::default().with_user_id("u"));
        assert_eq!(runtime.user_id().as_deref(), Some("u"));

        let forced = Rc::new(Cell::new(false));
        let observed = forced.clone();
        let auth = Auth::from_fn(move |force| {
            observed.set(force);
            async { Err(AuthError::NotSignedIn) }
        });
        assert_eq!(auth.token(true).await, Err(AuthError::NotSignedIn));
        assert!(forced.get());
        assert_eq!(auth.user_id(), None);
    }
}
