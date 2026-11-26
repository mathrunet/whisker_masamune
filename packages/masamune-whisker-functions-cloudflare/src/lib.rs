#![doc = include_str!("../README.md")]

//! Cloudflare adapter inspired by
//! `masamune_functions_cloudflare/lib/adapter/cloudflare_functions_adapter.dart`.

use std::time::Duration;

pub use masamune_whisker_functions::Auth;
use masamune_whisker_functions::{
    FunctionRequest, FunctionsAdapter, FunctionsError, FunctionsFuture, RestApiFunctionsAdapter,
};

/// Routes actions to edge or region Workers, using the same bearer token and App Check providers.
#[derive(Clone)]
pub struct CloudflareFunctionsAdapter {
    edge: RestApiFunctionsAdapter,
    region: Option<RestApiFunctionsAdapter>,
    auth: Auth,
    app_check: Option<Auth>,
    timeout: Duration,
    allow_local_http: bool,
}

impl CloudflareFunctionsAdapter {
    /// Creates an unauthenticated edge adapter. Set authentication explicitly when required.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            edge: RestApiFunctionsAdapter::new(endpoint),
            region: None,
            auth: Auth::none(),
            app_check: None,
            timeout: Duration::from_secs(30),
            allow_local_http: false,
        }
    }
    pub fn with_region_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        let mut adapter = RestApiFunctionsAdapter::new(endpoint)
            .with_auth(self.auth.clone())
            .with_timeout(self.timeout);
        if let Some(app_check) = self.app_check.clone() {
            adapter = adapter.with_app_check(app_check);
        }
        self.region = Some(if self.allow_local_http {
            adapter.allow_local_http()
        } else {
            adapter
        });
        self
    }
    /// Allows HTTP to loopback endpoints for local edge and region workers.
    pub fn allow_local_http(mut self) -> Self {
        self.allow_local_http = true;
        self.edge = self.edge.allow_local_http();
        self.region = self.region.map(RestApiFunctionsAdapter::allow_local_http);
        self
    }
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.edge = self.edge.with_auth(auth.clone());
        self.region = self.region.map(|adapter| adapter.with_auth(auth.clone()));
        self.auth = auth;
        self
    }
    /// Requires a nonempty App Check token (`X-Firebase-AppCheck`) on edge and region requests.
    pub fn with_app_check(mut self, provider: Auth) -> Self {
        self.edge = self.edge.with_app_check(provider.clone());
        self.region = self
            .region
            .map(|adapter| adapter.with_app_check(provider.clone()));
        self.app_check = Some(provider);
        self
    }
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.edge = self.edge.with_timeout(timeout);
        self.region = self.region.map(|adapter| adapter.with_timeout(timeout));
        self.timeout = timeout;
        self
    }
}

impl FunctionsAdapter for CloudflareFunctionsAdapter {
    fn send(&self, request: FunctionRequest) -> FunctionsFuture<'_> {
        let adapter = match request.worker_type.as_deref() {
            None | Some("edge") => Some(&self.edge),
            Some("region") => self.region.as_ref(),
            _ => None,
        };
        match adapter {
            Some(adapter) => adapter.send(request),
            None => Box::pin(async move {
                Err(FunctionsError::Configuration(format!(
                    "worker type {:?} is unsupported or its endpoint is not configured",
                    request.worker_type
                )))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn attaches_app_check_header_to_edge_and_region_requests() {
        let edge = MockServer::start().await;
        let region = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ping"))
            .and(header("X-Firebase-AppCheck", "app-check-tok"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": "edge"})),
            )
            .mount(&edge)
            .await;
        Mock::given(method("POST"))
            .and(path("/ping"))
            .and(header("X-Firebase-AppCheck", "app-check-tok"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": "region"})),
            )
            .mount(&region)
            .await;

        let adapter = CloudflareFunctionsAdapter::new(edge.uri())
            .with_region_endpoint(region.uri())
            .allow_local_http()
            .with_app_check(Auth::fixed("app-check-tok"));

        let edge_res = adapter.send(FunctionRequest::new("ping")).await.unwrap();
        assert_eq!(edge_res["ok"], "edge");
        let region_res = adapter
            .send(FunctionRequest::new("ping").with_worker_type("region"))
            .await
            .unwrap();
        assert_eq!(region_res["ok"], "region");
    }
}
