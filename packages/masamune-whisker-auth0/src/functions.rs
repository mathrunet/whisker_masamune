//! Ported from `Auth0FirebaseFunctionsAdapter` / `Auth0FunctionsAction` in
//! `flutter_app_royaltybank/lib/adapter.dart`.

use masamune_whisker_functions::{FunctionRequest, FunctionsAdapter, FunctionsFuture};
use serde_json::{Map, Value};

use crate::Auth0;

/// Body key of the Auth0 tenant domain.
pub const DOMAIN_KEY: &str = "auth0_domain";
/// Body key of the Auth0 access token.
pub const ACCESS_TOKEN_KEY: &str = "auth0_access_token";

/// Wraps any [`FunctionsAdapter`] and adds `auth0_domain` and, while signed in,
/// `auth0_access_token` to every JSON object body (a missing body becomes an object).
///
/// Keys the request already sets win, so an action can send a specific token (for example the
/// token right after login). Other body types are sent unchanged. When the token cannot be
/// renewed the request is sent without it, so public reads keep working.
///
/// ```ignore
/// let functions = Functions::new(Auth0FunctionsAdapter::new(
///     FirebaseFunctionsAdapter::new(/* … */),
///     auth0.clone(),
/// ));
/// ```
pub struct Auth0FunctionsAdapter<F> {
    inner: F,
    auth0: Auth0,
}

impl<F: FunctionsAdapter> Auth0FunctionsAdapter<F> {
    pub fn new(inner: F, auth0: Auth0) -> Self {
        Self { inner, auth0 }
    }

    pub fn inner(&self) -> &F {
        &self.inner
    }
}

/// Adds the Auth0 fields to `body` without overwriting existing keys.
pub(crate) fn merge_body(
    body: Option<Value>,
    domain: &str,
    token: Option<String>,
) -> Option<Value> {
    let mut object = match body {
        None => Map::new(),
        Some(Value::Object(object)) => object,
        Some(other) => return Some(other),
    };
    object
        .entry(DOMAIN_KEY)
        .or_insert_with(|| Value::String(domain.to_owned()));
    if let Some(token) = token.filter(|t| !t.is_empty()) {
        object
            .entry(ACCESS_TOKEN_KEY)
            .or_insert(Value::String(token));
    }
    Some(Value::Object(object))
}

impl<F: FunctionsAdapter> FunctionsAdapter for Auth0FunctionsAdapter<F> {
    fn send(&self, mut request: FunctionRequest) -> FunctionsFuture<'_> {
        Box::pin(async move {
            let token = match self.auth0.access_token(false).await {
                Ok(token) => token,
                Err(_) => {
                    log::debug!("auth0: sending a request without an access token");
                    None
                }
            };
            request.body = merge_body(request.body.take(), &self.auth0.config().domain, token);
            self.inner.send(request).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merges_without_overwriting() {
        assert_eq!(
            merge_body(None, "d", Some("t".into())),
            Some(json!({"auth0_domain": "d", "auth0_access_token": "t"}))
        );
        assert_eq!(
            merge_body(
                Some(json!({"auth0_access_token": "own", "x": 1})),
                "d",
                Some("t".into())
            ),
            Some(json!({"auth0_access_token": "own", "x": 1, "auth0_domain": "d"}))
        );
        assert_eq!(
            merge_body(Some(json!({})), "d", Some(String::new())),
            Some(json!({"auth0_domain": "d"}))
        );
        assert_eq!(merge_body(Some(json!([1])), "d", None), Some(json!([1])));
    }
}
