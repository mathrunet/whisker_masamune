use masamune_whisker_bridge::{NativeError, NativeModule};
use serde_json::json;

use crate::{Auth0Adapter, Auth0Config, Auth0Credentials, Auth0Error, Auth0Future};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Auth0")
}

/// Auth0.swift (iOS) and Auth0.Android with their credentials managers.
///
/// Login runs Universal Login in the system browser (`ASWebAuthenticationSession` / Custom
/// Tabs) and stores the credentials; logout clears the browser session and the stored
/// credentials.
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeAuth0Adapter;

/// Maps a native rejection (`cancelled`, `no-credentials` or `<code>: <message>`).
pub(crate) fn map_error(error: NativeError) -> Auth0Error {
    let message = error.0;
    match message.as_str() {
        "cancelled" => return Auth0Error::Cancelled,
        "no-credentials" => return Auth0Error::NoCredentials,
        _ => {}
    }
    if let Some((code, rest)) = message.split_once(": ")
        && !code.is_empty()
        && !code.contains(char::is_whitespace)
    {
        return Auth0Error::Native {
            code: code.to_owned(),
            message: rest.to_owned(),
        };
    }
    Auth0Error::Native {
        code: "unknown".into(),
        message,
    }
}

fn account(config: &Auth0Config) -> serde_json::Value {
    json!({
        "domain": config.domain,
        "clientId": config.client_id,
        "scheme": config.scheme,
        "scope": config.scope(),
    })
}

impl Auth0Adapter for NativeAuth0Adapter {
    fn login<'a>(
        &'a self,
        config: &'a Auth0Config,
        prompt: Option<&'a str>,
    ) -> Auth0Future<'a, Auth0Credentials> {
        Box::pin(async move {
            let mut args = account(config);
            args["prompt"] = prompt.map_or(serde_json::Value::Null, |p| p.into());
            module().call_async("login", &args).await.map_err(map_error)
        })
    }

    fn logout<'a>(&'a self, config: &'a Auth0Config) -> Auth0Future<'a, ()> {
        Box::pin(async move {
            module()
                .call_async::<_, Option<bool>>("logout", &account(config))
                .await
                .map(|_| ())
                .map_err(map_error)
        })
    }

    fn credentials<'a>(
        &'a self,
        config: &'a Auth0Config,
        min_ttl_secs: u32,
    ) -> Auth0Future<'a, Option<Auth0Credentials>> {
        Box::pin(async move {
            let mut args = account(config);
            args["minTtl"] = min_ttl_secs.into();
            match module().call_async("credentials", &args).await {
                Ok(credentials) => Ok(credentials),
                Err(error) => match map_error(error) {
                    Auth0Error::NoCredentials => Ok(None),
                    other => Err(other),
                },
            }
        })
    }

    fn has_valid_credentials<'a>(
        &'a self,
        config: &'a Auth0Config,
        min_ttl_secs: u32,
    ) -> Auth0Future<'a, bool> {
        Box::pin(async move {
            let mut args = account(config);
            args["minTtl"] = min_ttl_secs.into();
            module()
                .call_async("hasValidCredentials", &args)
                .await
                .map_err(map_error)
        })
    }

    fn clear<'a>(&'a self, config: &'a Auth0Config) -> Auth0Future<'a, ()> {
        Box::pin(async move {
            module()
                .call_async::<_, Option<bool>>("clear", &account(config))
                .await
                .map(|_| ())
                .map_err(map_error)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_native_errors() {
        assert_eq!(
            map_error(NativeError("cancelled".into())),
            Auth0Error::Cancelled
        );
        assert_eq!(
            map_error(NativeError("no-credentials".into())),
            Auth0Error::NoCredentials
        );
        assert_eq!(
            map_error(NativeError("a0.invalid_grant: expired".into())),
            Auth0Error::Native {
                code: "a0.invalid_grant".into(),
                message: "expired".into()
            }
        );
        assert_eq!(
            map_error(NativeError("network is down".into())),
            Auth0Error::Native {
                code: "unknown".into(),
                message: "network is down".into()
            }
        );
    }
}
