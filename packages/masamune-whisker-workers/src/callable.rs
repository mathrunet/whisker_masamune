use std::time::Duration;

use serde_json::Value;

use crate::auth::AuthProvider;
use crate::error::{ApiError, ApiResult};
use crate::{CallableClient, CallableFailure, HeaderMap};

/// Firebase HTTPS callable functions (`https://{region}-{project}.cloudfunctions.net/{name}`).
#[derive(Clone)]
pub struct Callable {
    pub project_id: String,
    pub region: String,
    /// Sends `X-Firebase-AppCheck` (functions with `enforceAppCheck: true` need it).
    pub app_check: bool,
    pub auth: AuthProvider,
    pub timeout: Duration,
}

impl Callable {
    /// Callable functions of `project_id` in `region`, authenticated as the Firebase user and
    /// sending App Check tokens.
    pub fn new(project_id: impl Into<String>, region: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            region: region.into(),
            app_check: true,
            auth: AuthProvider::firebase(),
            timeout: Duration::from_secs(60),
        }
    }

    /// Calls `name` with `data` and returns its `result`.
    pub async fn call(&self, name: &str, data: Value) -> ApiResult<Value> {
        let mut client = CallableClient::new(&self.project_id, &self.region);
        client.auth = self.auth.clone();
        client.timeout = self.timeout;
        if self.app_check {
            client.app_check = Some(firebase_app_check_provider());
        }
        client
            .call(name, data, HeaderMap::new(), None)
            .await
            .map_err(|error| match error {
                CallableFailure::Transport(error) => error,
                other => ApiError::Other(other.to_string()),
            })
    }
}

/// Native App Check provider shared by the legacy client and Functions adapter.
/// Activate the App Check module before using this provider.
pub fn firebase_app_check_provider() -> AuthProvider {
    AuthProvider::new(|force| async move {
        masamune_whisker_firebase_app_check::token(force)
            .await
            .map(Some)
            .map_err(|error| ApiError::Other(error.to_string()))
    })
}
