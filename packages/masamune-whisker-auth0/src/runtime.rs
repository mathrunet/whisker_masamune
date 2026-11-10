use std::cell::RefCell;
use std::rc::Rc;

use crate::types::now_ms;
use crate::{Auth0Adapter, Auth0Config, Auth0Credentials, Auth0Error, Auth0Future, Auth0User};

/// Lifetime of the access tokens [`RuntimeAuth0Adapter`] issues.
const TOKEN_LIFETIME_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Default)]
struct State {
    user: Auth0User,
    stored: Option<Auth0Credentials>,
    login_error: Option<Auth0Error>,
    renew_error: Option<Auth0Error>,
    issued: u32,
    prompts: Vec<Option<String>>,
    logouts: u32,
}

/// In-memory adapter for tests and development. Clones share the same state.
///
/// Login signs in [`RuntimeAuth0Adapter::with_user`] (`runtime-user` by default) and issues
/// `runtime-access-token-<n>` tokens valid for a day; renewal issues the next token.
#[derive(Clone)]
pub struct RuntimeAuth0Adapter(Rc<RefCell<State>>);

impl std::fmt::Debug for RuntimeAuth0Adapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RuntimeAuth0Adapter")
    }
}

impl Default for RuntimeAuth0Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeAuth0Adapter {
    pub fn new() -> Self {
        Self(Rc::new(RefCell::new(State {
            user: Auth0User {
                sub: "runtime-user".into(),
                email: Some("runtime-user@example.com".into()),
                name: Some("Runtime User".into()),
            },
            ..State::default()
        })))
    }

    /// The user the next login signs in.
    pub fn with_user(self, user: Auth0User) -> Self {
        self.0.borrow_mut().user = user;
        self
    }

    /// Starts with stored credentials (a previous session to restore).
    pub fn with_stored(self, credentials: Auth0Credentials) -> Self {
        self.0.borrow_mut().stored = Some(credentials);
        self
    }

    /// Makes logins fail with `error` (for example [`Auth0Error::Cancelled`]).
    pub fn with_login_error(self, error: Auth0Error) -> Self {
        self.set_login_error(Some(error));
        self
    }

    /// Makes renewals fail with `error`.
    pub fn with_renew_error(self, error: Auth0Error) -> Self {
        self.0.borrow_mut().renew_error = Some(error);
        self
    }

    pub fn set_login_error(&self, error: Option<Auth0Error>) {
        self.0.borrow_mut().login_error = error;
    }

    /// Credentials for `user` with a fresh token, as the adapter issues them.
    pub fn issue(&self, user: &Auth0User) -> Auth0Credentials {
        let mut state = self.0.borrow_mut();
        state.issued += 1;
        let n = state.issued;
        Auth0Credentials {
            access_token: format!("runtime-access-token-{n}"),
            id_token: format!("runtime-id-token-{n}"),
            refresh_token: Some("runtime-refresh-token".into()),
            expires_at_ms: now_ms() + TOKEN_LIFETIME_MS,
            scope: None,
            user: user.clone(),
        }
    }

    /// Currently stored credentials.
    pub fn stored(&self) -> Option<Auth0Credentials> {
        self.0.borrow().stored.clone()
    }

    /// The `prompt` of every login, in order.
    pub fn login_prompts(&self) -> Vec<Option<String>> {
        self.0.borrow().prompts.clone()
    }

    /// How many times logout ran.
    pub fn logout_count(&self) -> u32 {
        self.0.borrow().logouts
    }
}

impl Auth0Adapter for RuntimeAuth0Adapter {
    fn login<'a>(
        &'a self,
        config: &'a Auth0Config,
        prompt: Option<&'a str>,
    ) -> Auth0Future<'a, Auth0Credentials> {
        Box::pin(async move {
            let (error, user) = {
                let mut state = self.0.borrow_mut();
                state.prompts.push(prompt.map(str::to_owned));
                (state.login_error.clone(), state.user.clone())
            };
            if let Some(error) = error {
                return Err(error);
            }
            let mut credentials = self.issue(&user);
            credentials.scope = Some(config.scope());
            self.0.borrow_mut().stored = Some(credentials.clone());
            Ok(credentials)
        })
    }

    fn logout<'a>(&'a self, _config: &'a Auth0Config) -> Auth0Future<'a, ()> {
        Box::pin(async move {
            let mut state = self.0.borrow_mut();
            state.logouts += 1;
            state.stored = None;
            Ok(())
        })
    }

    fn credentials<'a>(
        &'a self,
        _config: &'a Auth0Config,
        min_ttl_secs: u32,
    ) -> Auth0Future<'a, Option<Auth0Credentials>> {
        Box::pin(async move {
            let Some(stored) = self.stored() else {
                return Ok(None);
            };
            if !stored.expires_within(now_ms(), min_ttl_secs) {
                return Ok(Some(stored));
            }
            if !stored.can_renew() {
                return Ok(None);
            }
            if let Some(error) = self.0.borrow().renew_error.clone() {
                return Err(error);
            }
            let mut renewed = self.issue(&stored.user);
            renewed.refresh_token = stored.refresh_token.clone();
            renewed.scope = stored.scope.clone();
            self.0.borrow_mut().stored = Some(renewed.clone());
            Ok(Some(renewed))
        })
    }

    fn has_valid_credentials<'a>(
        &'a self,
        _config: &'a Auth0Config,
        min_ttl_secs: u32,
    ) -> Auth0Future<'a, bool> {
        Box::pin(async move {
            Ok(self
                .stored()
                .is_some_and(|c| c.can_renew() || !c.expires_within(now_ms(), min_ttl_secs)))
        })
    }

    fn clear<'a>(&'a self, _config: &'a Auth0Config) -> Auth0Future<'a, ()> {
        Box::pin(async move {
            self.0.borrow_mut().stored = None;
            Ok(())
        })
    }
}
