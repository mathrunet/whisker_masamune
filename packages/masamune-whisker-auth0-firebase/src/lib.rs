#![doc = include_str!("../README.md")]

//! Ported from `RoyaltyBankAuthAdapter` (`flutter_royaltybank_package/lib/adapters/royalty_bank_auth_adapter.dart`)
//! and `Auth0FirebaseAuthAdapter` / `Auth0CustomTokenFunctionsAction`
//! (`flutter_app_royaltybank/lib/adapter.dart`).

use std::future::Future;
use std::pin::Pin;

use masamune_whisker_auth::{AuthAdapter, AuthError, AuthFuture, AuthResult};
use masamune_whisker_auth_firebase::FirebaseAuthAdapter;
pub use masamune_whisker_auth0::{Auth0, Auth0Credentials, Auth0Error};
use masamune_whisker_functions::{
    ApiError, FunctionRequest, Functions, FunctionsAction, FunctionsError, FunctionsResult,
};
use serde_json::{Value, json};

/// Name of the callable that exchanges an Auth0 access token for a Firebase custom token.
pub const CUSTOM_TOKEN_ACTION: &str = "auth0_custom_token";
/// The `prompt` [`login`] sends, so a remaining browser session never signs in silently.
pub const LOGIN_PROMPT: &str = "login";

/// Failure of the Auth0 → Firebase flow.
#[derive(Debug, Clone, PartialEq)]
pub enum Auth0FirebaseError {
    Auth0(Auth0Error),
    /// The `auth0_custom_token` callable failed.
    Functions(FunctionsError),
    /// Firebase Auth failed.
    Firebase(String),
    /// The `on_auth0_signin` callback of [`login`] failed.
    Callback(String),
}

impl std::fmt::Display for Auth0FirebaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth0(error) => error.fmt(f),
            Self::Functions(error) => write!(f, "{CUSTOM_TOKEN_ACTION}: {error}"),
            Self::Firebase(message) => write!(f, "firebase auth: {message}"),
            Self::Callback(message) => write!(f, "auth0 sign-in callback: {message}"),
        }
    }
}

impl std::error::Error for Auth0FirebaseError {}

impl From<Auth0Error> for Auth0FirebaseError {
    fn from(error: Auth0Error) -> Self {
        Self::Auth0(error)
    }
}

impl From<FunctionsError> for Auth0FirebaseError {
    fn from(error: FunctionsError) -> Self {
        Self::Functions(error)
    }
}

pub type Auth0FirebaseResult<T> = Result<T, Auth0FirebaseError>;

/// Exchanges an Auth0 access token for a Firebase custom token
/// (`firebase/functions/src/auth0_custom_token.ts`).
///
/// The body is `{"auth0_access_token", "auth0_domain"}`. The response is the callable's
/// result, `{"token": "..."}`; a bare string is accepted too. An empty token is an error.
#[derive(Clone, PartialEq, Eq)]
pub struct Auth0CustomTokenAction {
    pub access_token: String,
    pub domain: String,
}

impl std::fmt::Debug for Auth0CustomTokenAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth0CustomTokenAction")
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}

impl Auth0CustomTokenAction {
    pub fn new(access_token: impl Into<String>, domain: impl Into<String>) -> Self {
        Self {
            access_token: access_token.into(),
            domain: domain.into(),
        }
    }
}

impl FunctionsAction for Auth0CustomTokenAction {
    type Response = String;

    fn request(&self) -> FunctionsResult<FunctionRequest> {
        Ok(FunctionRequest::new(CUSTOM_TOKEN_ACTION).with_body(json!({
            "auth0_access_token": self.access_token,
            "auth0_domain": self.domain,
        })))
    }

    fn decode(&self, value: Value) -> FunctionsResult<String> {
        let token = match &value {
            Value::String(token) => Some(token.as_str()),
            Value::Object(object) => object.get("token").and_then(Value::as_str),
            _ => None,
        };
        match token {
            Some(token) if !token.is_empty() => Ok(token.to_owned()),
            _ => Err(ApiError::Decode("failed to exchange the Auth0 access token".into()).into()),
        }
    }
}

/// Futures run on the Whisker UI thread and do not require `Send`.
pub type SessionFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, String>> + 'a>>;

/// The Firebase Auth calls the flow needs. [`NativeFirebaseSession`] uses whisker-firebase;
/// tests pass a fake.
pub trait FirebaseSession {
    /// UID of the signed-in Firebase user.
    fn current_uid(&self) -> Option<String>;
    /// Signs in with a custom token and returns the UID.
    fn sign_in_with_custom_token<'a>(&'a self, token: &'a str) -> SessionFuture<'a, String>;
    fn sign_out(&self) -> Result<(), String>;
}

/// Firebase Authentication of the default app (whisker-firebase).
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeFirebaseSession;

impl FirebaseSession for NativeFirebaseSession {
    fn current_uid(&self) -> Option<String> {
        masamune_whisker_auth_firebase::current_uid()
    }

    fn sign_in_with_custom_token<'a>(&'a self, token: &'a str) -> SessionFuture<'a, String> {
        Box::pin(async move {
            let auth = whisker_firebase::auth::Auth::instance().map_err(|e| e.to_string())?;
            let credential = auth
                .sign_in_with_custom_token(token)
                .await
                .map_err(|e| e.to_string())?;
            Ok(credential.user.uid)
        })
    }

    fn sign_out(&self) -> Result<(), String> {
        let auth = whisker_firebase::auth::Auth::instance().map_err(|e| e.to_string())?;
        auth.sign_out().map_err(|e| e.to_string())
    }
}

/// The Auth0 → Firebase flow over a [`FirebaseSession`]. The free functions of this crate use
/// [`NativeFirebaseSession`].
#[derive(Debug, Clone, Default)]
pub struct Auth0Firebase<S> {
    session: S,
}

impl Auth0Firebase<NativeFirebaseSession> {
    pub fn native() -> Self {
        Self::new(NativeFirebaseSession)
    }
}

impl<S: FirebaseSession> Auth0Firebase<S> {
    pub fn new(session: S) -> Self {
        Self { session }
    }

    pub fn session(&self) -> &S {
        &self.session
    }

    /// Exchanges `credentials`' access token through `functions` and signs in to Firebase.
    /// Returns the Firebase UID (the Auth0 `sub`).
    pub async fn sign_in_firebase(
        &self,
        functions: &Functions,
        credentials: &Auth0Credentials,
        domain: &str,
    ) -> Auth0FirebaseResult<String> {
        let token = functions
            .execute(Auth0CustomTokenAction::new(
                credentials.access_token.clone(),
                domain,
            ))
            .await?;
        self.session
            .sign_in_with_custom_token(&token)
            .await
            .map_err(Auth0FirebaseError::Firebase)
    }

    /// Signs out of Firebase when a user is signed in.
    pub fn sign_out_firebase(&self) -> Auth0FirebaseResult<()> {
        if self.session.current_uid().is_none() {
            return Ok(());
        }
        self.session
            .sign_out()
            .map_err(Auth0FirebaseError::Firebase)
    }

    /// Restores the Auth0 session and makes the Firebase user match it.
    ///
    /// - No Auth0 session: signs out of Firebase and returns `Ok(None)`.
    /// - The Firebase UID equals the Auth0 `sub`: keeps the Firebase session.
    /// - Otherwise: signs out of Firebase and exchanges the token again.
    pub async fn restore(
        &self,
        auth0: &Auth0,
        functions: &Functions,
    ) -> Auth0FirebaseResult<Option<Auth0Credentials>> {
        let credentials = match auth0.restore().await {
            Ok(Some(credentials)) => credentials,
            Ok(None) => {
                self.sign_out_firebase()?;
                return Ok(None);
            }
            Err(error) => {
                let _ = self.sign_out_firebase();
                return Err(error.into());
            }
        };
        if self.session.current_uid().as_deref() == Some(credentials.user.sub.as_str()) {
            return Ok(Some(credentials));
        }
        // Never carry over a Firebase user of another Auth0 user.
        self.sign_out_firebase()?;
        self.sign_in_firebase(functions, &credentials, &auth0.config().domain)
            .await?;
        Ok(Some(credentials))
    }

    /// Runs Auth0 login (`prompt=login`), then runs `on_auth0_signin` (for example registering
    /// the user with the backend) while replacing the Firebase user.
    ///
    /// Returns `Ok(None)` when the user cancels. When the callback or the Firebase sign-in
    /// fails, the Auth0 credentials are cleared and Firebase is signed out, so no half
    /// signed-in state remains.
    pub async fn login<F, Fut, E>(
        &self,
        auth0: &Auth0,
        functions: &Functions,
        on_auth0_signin: F,
    ) -> Auth0FirebaseResult<Option<Auth0Credentials>>
    where
        F: FnOnce(&Auth0Credentials) -> Fut,
        Fut: Future<Output = Result<(), E>>,
        E: std::fmt::Display,
    {
        let credentials = match auth0.login(Some(LOGIN_PROMPT)).await {
            Ok(credentials) => credentials,
            Err(Auth0Error::Cancelled) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let domain = auth0.config().domain.clone();
        let firebase = async {
            // Drop the previous user's Firebase session before signing in.
            self.sign_out_firebase()?;
            self.sign_in_firebase(functions, &credentials, &domain)
                .await
        };
        let callback = on_auth0_signin(&credentials);
        let (callback, firebase) = futures_util::future::join(callback, firebase).await;
        let result = match (callback, firebase) {
            (Err(error), _) => Err(Auth0FirebaseError::Callback(error.to_string())),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(_)) => Ok(()),
        };
        if let Err(error) = result {
            let _ = auth0.clear().await;
            let _ = self.sign_out_firebase();
            return Err(error);
        }
        Ok(Some(credentials))
    }

    /// Signs out of Firebase and Auth0 (browser session and stored credentials).
    pub async fn logout(&self, auth0: &Auth0) -> Auth0FirebaseResult<()> {
        let firebase = self.sign_out_firebase();
        auth0.logout().await?;
        firebase
    }
}

/// [`Auth0Firebase::sign_in_firebase`] with the native Firebase SDK.
pub async fn sign_in_firebase(
    functions: &Functions,
    credentials: &Auth0Credentials,
    domain: &str,
) -> Auth0FirebaseResult<String> {
    Auth0Firebase::native()
        .sign_in_firebase(functions, credentials, domain)
        .await
}

/// [`Auth0Firebase::sign_out_firebase`] with the native Firebase SDK.
pub fn sign_out_firebase() -> Auth0FirebaseResult<()> {
    Auth0Firebase::native().sign_out_firebase()
}

/// [`Auth0Firebase::restore`] with the native Firebase SDK.
pub async fn restore(
    auth0: &Auth0,
    functions: &Functions,
) -> Auth0FirebaseResult<Option<Auth0Credentials>> {
    Auth0Firebase::native().restore(auth0, functions).await
}

/// [`Auth0Firebase::login`] with the native Firebase SDK.
pub async fn login<F, Fut, E>(
    auth0: &Auth0,
    functions: &Functions,
    on_auth0_signin: F,
) -> Auth0FirebaseResult<Option<Auth0Credentials>>
where
    F: FnOnce(&Auth0Credentials) -> Fut,
    Fut: Future<Output = Result<(), E>>,
    E: std::fmt::Display,
{
    Auth0Firebase::native()
        .login(auth0, functions, on_auth0_signin)
        .await
}

/// [`Auth0Firebase::logout`] with the native Firebase SDK.
pub async fn logout(auth0: &Auth0) -> Auth0FirebaseResult<()> {
    Auth0Firebase::native().logout(auth0).await
}

/// Firebase ID tokens of the user signed in through the custom token (UID = Auth0 `sub`),
/// for Firebase callables and other Firebase-authenticated requests.
///
/// Before the Firebase sign-in no token is returned (`Ok(None)`) instead of
/// [`AuthError::NotSignedIn`], so requests are sent without credentials like the Firebase SDK
/// does. The `auth0_custom_token` exchange that performs the sign-in needs this.
#[derive(Debug, Clone, Copy, Default)]
pub struct Auth0FirebaseAuthAdapter;

impl AuthAdapter for Auth0FirebaseAuthAdapter {
    fn access_token(&self, force_refresh: bool) -> AuthFuture<'_> {
        Box::pin(async move {
            anonymous_when_signed_out(FirebaseAuthAdapter.access_token(force_refresh).await)
        })
    }

    fn user_id(&self) -> Option<String> {
        FirebaseAuthAdapter.user_id()
    }
}

/// [`masamune_whisker_auth::Auth`] backed by [`Auth0FirebaseAuthAdapter`].
pub fn auth0_firebase_auth() -> masamune_whisker_auth::Auth {
    masamune_whisker_auth::Auth::new(Auth0FirebaseAuthAdapter)
}

/// Treats "nobody is signed in to Firebase" as "no token".
fn anonymous_when_signed_out(result: AuthResult<Option<String>>) -> AuthResult<Option<String>> {
    match result {
        Err(AuthError::NotSignedIn) => Ok(None),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_out_sends_no_token() {
        assert_eq!(
            anonymous_when_signed_out(Err(AuthError::NotSignedIn)),
            Ok(None)
        );
    }

    #[test]
    fn keeps_tokens_and_other_errors() {
        assert_eq!(
            anonymous_when_signed_out(Ok(Some("id-token".into()))),
            Ok(Some("id-token".into()))
        );
        assert_eq!(
            anonymous_when_signed_out(Err(AuthError::Other("boom".into()))),
            Err(AuthError::Other("boom".into()))
        );
    }
}
