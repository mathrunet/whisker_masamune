use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

#[cfg(feature = "firebase-auth")]
use crate::error::ApiError;
use crate::error::ApiResult;

type TokenFuture = Pin<Box<dyn Future<Output = ApiResult<Option<String>>>>>;

/// Source of the bearer token sent to the workers.
///
/// The closure receives `force_refresh` and returns `Ok(None)` when no header should be sent.
/// It runs on the UI thread, so it does not need to be `Send`.
#[derive(Clone)]
pub struct AuthProvider(Rc<dyn Fn(bool) -> TokenFuture>);

impl AuthProvider {
    /// Sends no `Authorization` header.
    pub fn none() -> Self {
        Self::new(|_| async { Ok(None) })
    }

    /// Always sends `token` (tests, server tokens).
    pub fn fixed(token: impl Into<String>) -> Self {
        let token: String = token.into();
        Self::new(move |_| {
            let token = token.clone();
            async move { Ok(Some(token)) }
        })
    }

    /// Custom token source.
    pub fn new<F, Fut>(f: F) -> Self
    where
        F: Fn(bool) -> Fut + 'static,
        Fut: Future<Output = ApiResult<Option<String>>> + 'static,
    {
        Self(Rc::new(move |force| Box::pin(f(force))))
    }

    /// The ID token of the signed-in Firebase user. Fails with [`ApiError::NotSignedIn`] when
    /// nobody is signed in.
    #[cfg(feature = "firebase-auth")]
    pub fn firebase() -> Self {
        Self::new(|force| async move {
            use whisker_firebase::auth::Auth;
            let auth = Auth::instance().map_err(|e| ApiError::Other(e.to_string()))?;
            let user = auth
                .current_user()
                .map_err(|e| ApiError::Other(e.to_string()))?
                .ok_or(ApiError::NotSignedIn)?;
            user.id_token(force)
                .await
                .map(Some)
                .map_err(|e| ApiError::Other(e.to_string()))
        })
    }

    /// Resolves the token.
    pub async fn token(&self, force_refresh: bool) -> ApiResult<Option<String>> {
        (self.0)(force_refresh).await
    }
}

/// UID of the signed-in Firebase user.
#[cfg(feature = "firebase-auth")]
pub fn current_uid() -> Option<String> {
    whisker_firebase::auth::Auth::instance()
        .ok()?
        .current_user()
        .ok()?
        .map(|u| u.uid)
}
