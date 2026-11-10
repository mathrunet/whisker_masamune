#![doc = include_str!("../README.md")]

//! Firebase adapter inspired by `katana_auth_firebase/lib/adapter/firebase_auth_adapter.dart`.

pub use masamune_whisker_auth::{Auth, AuthAdapter, AuthError, AuthFuture, AuthResult};
use whisker_firebase::auth::Auth as FirebaseAuth;

/// ID tokens and the UID of the signed-in Firebase user.
#[derive(Clone, Copy, Debug, Default)]
pub struct FirebaseAuthAdapter;

impl AuthAdapter for FirebaseAuthAdapter {
    /// Fails with [`AuthError::NotSignedIn`] when nobody is signed in.
    fn access_token(&self, force_refresh: bool) -> AuthFuture<'_> {
        Box::pin(async move {
            let auth = FirebaseAuth::instance().map_err(|e| AuthError::Other(e.to_string()))?;
            let user = auth
                .current_user()
                .map_err(|e| AuthError::Other(e.to_string()))?
                .ok_or(AuthError::NotSignedIn)?;
            user.id_token(force_refresh)
                .await
                .map(Some)
                .map_err(|e| AuthError::Other(e.to_string()))
        })
    }

    fn user_id(&self) -> Option<String> {
        current_uid()
    }
}

/// [`Auth`] backed by [`FirebaseAuthAdapter`].
pub fn firebase_auth() -> Auth {
    Auth::new(FirebaseAuthAdapter)
}

/// UID of the signed-in Firebase user.
pub fn current_uid() -> Option<String> {
    FirebaseAuth::instance()
        .ok()?
        .current_user()
        .ok()?
        .map(|u| u.uid)
}
