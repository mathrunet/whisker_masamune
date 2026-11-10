#![doc = include_str!("../README.md")]

//! Cloudflare adapter inspired by
//! `masamune_functions_cloudflare/lib/adapter/cloudflare_functions_adapter.dart`.

use std::time::Duration;

pub use masamune_whisker_functions::Auth;
use masamune_whisker_functions::{
    FunctionRequest, FunctionsAdapter, FunctionsError, FunctionsFuture, RestApiFunctionsAdapter,
};

/// Routes actions to edge or region Workers, using the same bearer token provider.
#[derive(Clone)]
pub struct CloudflareFunctionsAdapter {
    edge: RestApiFunctionsAdapter,
    region: Option<RestApiFunctionsAdapter>,
    auth: Auth,
    timeout: Duration,
}

impl CloudflareFunctionsAdapter {
    /// Creates an unauthenticated edge adapter. Set authentication explicitly when required.
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            edge: RestApiFunctionsAdapter::new(endpoint),
            region: None,
            auth: Auth::none(),
            timeout: Duration::from_secs(30),
        }
    }
    pub fn with_region_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.region = Some(
            RestApiFunctionsAdapter::new(endpoint)
                .with_auth(self.auth.clone())
                .with_timeout(self.timeout),
        );
        self
    }
    pub fn with_auth(mut self, auth: Auth) -> Self {
        self.edge = self.edge.with_auth(auth.clone());
        self.region = self.region.map(|adapter| adapter.with_auth(auth.clone()));
        self.auth = auth;
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
