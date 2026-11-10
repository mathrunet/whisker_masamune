#![doc = include_str!("../README.md")]

//! Firebase callable adapter inspired by
//! `katana_functions_firebase/lib/adapter/firebase_functions_adapter.dart`.

use std::time::Duration;

use masamune_whisker_functions::{
    FunctionRequest, FunctionsAdapter, FunctionsError, FunctionsFuture,
};
pub use masamune_whisker_workers::{AuthProvider, CallableClient, CallableFailure};
use serde_json::Value;

/// Firebase HTTPS callable adapter. Native Firebase providers are opt-in.
#[derive(Clone)]
pub struct FirebaseFunctionsAdapter {
    client: CallableClient,
    firestore_database_id: Option<String>,
}

impl FirebaseFunctionsAdapter {
    /// Creates an unauthenticated client without App Check. Select providers explicitly.
    pub fn new(project: &str, region: &str) -> Self {
        Self::from_client(CallableClient::new(project, region))
    }
    /// Uses an explicit base URL, e.g. `http://localhost:5001/project/region`.
    pub fn at_endpoint(endpoint: impl Into<String>) -> Self {
        Self::from_client(CallableClient::at_endpoint(endpoint))
    }
    pub fn from_client(client: CallableClient) -> Self {
        Self {
            client,
            firestore_database_id: None,
        }
    }
    pub fn with_auth(mut self, auth: AuthProvider) -> Self {
        self.client.auth = auth;
        self
    }
    /// Requires a nonempty App Check token for every call; failure prevents HTTP dispatch.
    pub fn with_app_check(mut self, provider: AuthProvider) -> Self {
        self.client.app_check = Some(provider);
        self
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.client.timeout = timeout;
        self
    }
    pub fn with_firestore_database_id(mut self, id: impl Into<String>) -> Self {
        self.firestore_database_id = Some(id.into());
        self
    }
    /// Uses the signed-in Firebase user and native App Check. Activate App Check at boot.
    /// Currently limited-use App Check tokens are not supported by the native module.
    #[cfg(feature = "firebase")]
    pub fn with_firebase_providers(self) -> Self {
        self.with_auth(AuthProvider::firebase())
            .with_app_check(masamune_whisker_workers::firebase_app_check_provider())
    }
}

impl FunctionsAdapter for FirebaseFunctionsAdapter {
    fn send(&self, request: FunctionRequest) -> FunctionsFuture<'_> {
        Box::pin(async move {
            // Callable operations use the action name and POST, regardless of REST metadata.
            // Reject REST-only overrides rather than silently dropping application input.
            if request.path.is_some()
                || !request.query.is_empty()
                || request.method != masamune_whisker_functions::Method::POST
            {
                return Err(FunctionsError::Configuration(
                    "Firebase callable actions require POST and cannot set a REST path or query"
                        .into(),
                ));
            }
            let mut data = request.body.unwrap_or_else(|| serde_json::json!({}));
            if let Some(id) = &self.firestore_database_id {
                data.as_object_mut()
                    .ok_or_else(|| {
                        FunctionsError::Configuration(
                            "firestoreDatabaseId requires an object input".into(),
                        )
                    })?
                    .insert("firestoreDatabaseId".into(), Value::String(id.clone()));
            }
            self.client
                .call(&request.action, data, request.headers, request.timeout)
                .await
                .map_err(Into::into)
        })
    }
}
