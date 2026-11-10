#![doc = include_str!("../README.md")]

//! Ported from `flutter_royaltybank_package/lib/adapters/royalty_bank_auth_adapter.dart`
//! (the Auth0 part; the Firebase exchange lives in `masamune-whisker-auth0-firebase`).

pub mod cng;

#[cfg(feature = "functions")]
mod functions;
mod native;
mod runtime;
mod types;

use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapter;
use masamune_whisker_auth::{Auth, AuthAdapter, AuthError, AuthFuture};

#[cfg(feature = "functions")]
pub use functions::{ACCESS_TOKEN_KEY, Auth0FunctionsAdapter, DOMAIN_KEY};
pub use native::NativeAuth0Adapter;
pub use runtime::RuntimeAuth0Adapter;
pub use types::{
    Auth0Config, Auth0Credentials, Auth0Error, Auth0Future, Auth0Result, Auth0User,
    DEFAULT_MIN_TTL_SECS, DEFAULT_SCOPES,
};

/// Backend of [`Auth0`]: [`NativeAuth0Adapter`] in apps, [`RuntimeAuth0Adapter`] in tests.
pub trait Auth0Adapter {
    /// Runs Universal Login and stores the credentials. `prompt` is sent as the `prompt`
    /// parameter (`Some("login")` always asks for the credentials again).
    /// Fails with [`Auth0Error::Cancelled`] when the user closes the screen.
    fn login<'a>(
        &'a self,
        config: &'a Auth0Config,
        prompt: Option<&'a str>,
    ) -> Auth0Future<'a, Auth0Credentials>;

    /// Clears the browser session and the stored credentials.
    fn logout<'a>(&'a self, config: &'a Auth0Config) -> Auth0Future<'a, ()>;

    /// The stored credentials, renewed (and stored again) with the refresh token when the
    /// access token expires within `min_ttl_secs`. `Ok(None)` when nothing usable is stored.
    fn credentials<'a>(
        &'a self,
        config: &'a Auth0Config,
        min_ttl_secs: u32,
    ) -> Auth0Future<'a, Option<Auth0Credentials>>;

    /// Whether stored credentials are valid for `min_ttl_secs` or can be renewed.
    fn has_valid_credentials<'a>(
        &'a self,
        config: &'a Auth0Config,
        min_ttl_secs: u32,
    ) -> Auth0Future<'a, bool>;

    /// Removes the stored credentials (the browser session is kept).
    fn clear<'a>(&'a self, config: &'a Auth0Config) -> Auth0Future<'a, ()>;
}

struct Inner {
    config: Auth0Config,
    adapter: Rc<dyn Auth0Adapter>,
    // Synchronous view of the signed-in user (`user_id`, `is_signed_in`); the native
    // credentials manager owns persistence.
    cached: RefCell<Option<Auth0Credentials>>,
}

/// Clonable handle of the Auth0 session. Clones share the in-memory credentials.
///
/// ```no_run
/// # async fn example() -> Result<(), masamune_whisker_auth0::Auth0Error> {
/// use masamune_whisker_auth0::{Auth0, Auth0Config, NativeAuth0Adapter};
///
/// let auth0 = Auth0::new(
///     Auth0Config::new("example.jp.auth0.com", "CLIENT_ID", "com.example.app"),
///     NativeAuth0Adapter,
/// );
/// if auth0.restore().await?.is_none() {
///     auth0.login(Some("login")).await?;
/// }
/// let token = auth0.access_token(false).await?;
/// # Ok(()) }
/// ```
#[derive(Clone)]
pub struct Auth0(Rc<Inner>);

impl std::fmt::Debug for Auth0 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Auth0")
            .field("domain", &self.0.config.domain)
            .field("signed_in", &self.is_signed_in())
            .finish()
    }
}

impl Auth0 {
    pub fn new(config: Auth0Config, adapter: impl Auth0Adapter + 'static) -> Self {
        Self::with_adapter(config, Rc::new(adapter))
    }

    pub fn with_adapter(config: Auth0Config, adapter: Rc<dyn Auth0Adapter>) -> Self {
        Self(Rc::new(Inner {
            config,
            adapter,
            cached: RefCell::new(None),
        }))
    }

    /// The handle of the installed [`Auth0MasamuneAdapter`].
    pub fn primary() -> Option<Self> {
        Auth0MasamuneAdapter::primary().map(|adapter| adapter.auth0.clone())
    }

    pub fn config(&self) -> &Auth0Config {
        &self.0.config
    }

    pub fn adapter(&self) -> &Rc<dyn Auth0Adapter> {
        &self.0.adapter
    }

    /// Credentials of the signed-in user held in memory.
    pub fn cached(&self) -> Option<Auth0Credentials> {
        self.0.cached.borrow().clone()
    }

    /// Profile of the signed-in user.
    pub fn user(&self) -> Option<Auth0User> {
        self.0.cached.borrow().as_ref().map(|c| c.user.clone())
    }

    /// The `sub` of the signed-in user.
    pub fn user_id(&self) -> Option<String> {
        self.0
            .cached
            .borrow()
            .as_ref()
            .map(|c| c.user.sub.clone())
            .filter(|sub| !sub.is_empty())
    }

    pub fn is_signed_in(&self) -> bool {
        self.0.cached.borrow().is_some()
    }

    fn set_cached(&self, credentials: Option<Auth0Credentials>) {
        *self.0.cached.borrow_mut() = credentials;
    }

    /// Runs Universal Login. `Err(Auth0Error::Cancelled)` when the user closes the screen.
    pub async fn login(&self, prompt: Option<&str>) -> Auth0Result<Auth0Credentials> {
        let credentials = self.0.adapter.login(&self.0.config, prompt).await?;
        self.set_cached(Some(credentials.clone()));
        Ok(credentials)
    }

    /// Restores the stored session (renewing a token that expires within a minute).
    ///
    /// `Ok(None)` when nothing usable is stored. When renewal fails the stored credentials are
    /// cleared and the error is returned, so the user signs in again.
    pub async fn restore(&self) -> Auth0Result<Option<Auth0Credentials>> {
        let restored = async {
            if !self
                .0
                .adapter
                .has_valid_credentials(&self.0.config, 0)
                .await?
            {
                return Ok(None);
            }
            self.0
                .adapter
                .credentials(&self.0.config, DEFAULT_MIN_TTL_SECS)
                .await
        }
        .await;
        match restored {
            Ok(Some(credentials)) => {
                self.set_cached(Some(credentials.clone()));
                Ok(Some(credentials))
            }
            Ok(None) => {
                self.clear().await?;
                Ok(None)
            }
            Err(error) => {
                // The error is more useful to the caller than a failure to clear.
                let _ = self.clear().await;
                Err(error)
            }
        }
    }

    /// Access token of the signed-in user, renewed when it has expired or `force_refresh`
    /// is set. `Ok(None)` when signed out or the session cannot be renewed.
    pub async fn access_token(&self, force_refresh: bool) -> Auth0Result<Option<String>> {
        let Some(cached) = self.cached() else {
            return Ok(None);
        };
        if !force_refresh && !cached.expires_within(types::now_ms(), 0) {
            return Ok(Some(cached.access_token));
        }
        if !cached.can_renew() {
            return Ok(None);
        }
        let renewed = self
            .0
            .adapter
            .credentials(&self.0.config, DEFAULT_MIN_TTL_SECS)
            .await?;
        let token = renewed.as_ref().map(|c| c.access_token.clone());
        self.set_cached(renewed);
        Ok(token)
    }

    /// Clears the browser session and the stored credentials. The local session is cleared
    /// even when the browser step fails; that error is returned afterwards.
    pub async fn logout(&self) -> Auth0Result<()> {
        let result = self.0.adapter.logout(&self.0.config).await;
        let cleared = self.clear().await;
        result.and(cleared)
    }

    /// Forgets the session on this device without opening the browser.
    pub async fn clear(&self) -> Auth0Result<()> {
        self.set_cached(None);
        self.0.adapter.clear(&self.0.config).await
    }
}

/// Access tokens and `sub` of the Auth0 session (`Authorization: Bearer <Auth0 access token>`).
impl AuthAdapter for Auth0 {
    fn access_token(&self, force_refresh: bool) -> AuthFuture<'_> {
        Box::pin(async move {
            Auth0::access_token(self, force_refresh)
                .await
                .map_err(|e| AuthError::Other(e.to_string()))
        })
    }

    fn user_id(&self) -> Option<String> {
        Auth0::user_id(self)
    }
}

/// Resolves [`Auth0::primary`] on every call, so it works before the adapters are installed.
struct PrimaryAuth0;

impl AuthAdapter for PrimaryAuth0 {
    fn access_token(&self, force_refresh: bool) -> AuthFuture<'_> {
        Box::pin(async move {
            match Auth0::primary() {
                Some(auth0) => AuthAdapter::access_token(&auth0, force_refresh).await,
                None => Ok(None),
            }
        })
    }

    fn user_id(&self) -> Option<String> {
        Auth0::primary()?.user_id()
    }
}

/// [`Auth`] backed by the installed [`Auth0MasamuneAdapter`] (no token while none is installed).
pub fn auth0_auth() -> Auth {
    Auth::new(PrimaryAuth0)
}

/// The app's Auth0 session for [`masamune_whisker_adapter::MasamuneAdapters`];
/// read it with [`Auth0::primary`].
pub struct Auth0MasamuneAdapter {
    auth0: Auth0,
}

impl Auth0MasamuneAdapter {
    pub fn new(config: Auth0Config, adapter: impl Auth0Adapter + 'static) -> Self {
        Self {
            auth0: Auth0::new(config, adapter),
        }
    }

    /// Uses [`NativeAuth0Adapter`].
    pub fn native(config: Auth0Config) -> Self {
        Self::new(config, NativeAuth0Adapter)
    }

    /// Wraps an existing handle.
    pub fn from_auth0(auth0: Auth0) -> Self {
        Self { auth0 }
    }

    pub fn auth0(&self) -> &Auth0 {
        &self.auth0
    }
}

impl MasamuneAdapter for Auth0MasamuneAdapter {}
