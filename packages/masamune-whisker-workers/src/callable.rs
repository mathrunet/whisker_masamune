use std::time::Duration;

use serde_json::{Value, json};

use crate::auth::AuthProvider;
use crate::error::{ApiError, ApiResult};
use crate::{client, read_json};

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
        let url = format!(
            "https://{}-{}.cloudfunctions.net/{name}",
            self.region, self.project_id
        );
        let mut request = client()
            .post(url)
            .json(&json!({ "data": data }))
            .timeout(self.timeout);
        if let Some(token) = self.auth.token(false).await? {
            request = request.bearer_auth(token);
        }
        if self.app_check {
            match masamune_whisker_firebase_app_check::token(false).await {
                Ok(token) => request = request.header("X-Firebase-AppCheck", token),
                Err(error) => log::warn!("app check token unavailable: {error}"),
            }
        }
        let response = read_json(request.send().await?).await?;
        match response.get("error") {
            Some(error) => Err(ApiError::Other(error.to_string())),
            None => Ok(response.get("result").cloned().unwrap_or(Value::Null)),
        }
    }
}
