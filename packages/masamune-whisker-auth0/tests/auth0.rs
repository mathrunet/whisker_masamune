use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_auth::AuthAdapter;
use masamune_whisker_auth0::{
    Auth0, Auth0Config, Auth0Credentials, Auth0Error, Auth0MasamuneAdapter, Auth0User,
    RuntimeAuth0Adapter, auth0_auth,
};

fn config() -> Auth0Config {
    Auth0Config::new("example.jp.auth0.com", "client", "com.example.app")
}

fn expired(adapter: &RuntimeAuth0Adapter, refresh: bool) -> Auth0Credentials {
    let mut credentials = adapter.issue(&Auth0User {
        sub: "auth0|stored".into(),
        ..Default::default()
    });
    credentials.expires_at_ms = 0;
    if !refresh {
        credentials.refresh_token = None;
    }
    credentials
}

#[test]
fn config_defaults_to_offline_access_scopes() {
    assert_eq!(config().scope(), "openid profile email offline_access");
    assert_eq!(config().with_scopes(["openid"]).scope(), "openid");
}

#[tokio::test]
async fn login_caches_the_user_and_tokens() {
    let adapter = RuntimeAuth0Adapter::new();
    let auth0 = Auth0::new(config(), adapter.clone());
    assert!(!auth0.is_signed_in());
    assert_eq!(auth0.access_token(false).await, Ok(None));

    let credentials = auth0.login(Some("login")).await.unwrap();
    assert_eq!(auth0.user_id().as_deref(), Some("runtime-user"));
    assert_eq!(
        auth0.access_token(false).await.unwrap(),
        Some(credentials.access_token.clone())
    );
    // Forced refresh asks the credentials manager again.
    assert_eq!(
        AuthAdapter::access_token(&auth0, true).await.unwrap(),
        Some(credentials.access_token)
    );
    assert_eq!(
        AuthAdapter::user_id(&auth0).as_deref(),
        Some("runtime-user")
    );
    assert_eq!(adapter.login_prompts(), vec![Some("login".to_string())]);

    auth0.logout().await.unwrap();
    assert!(!auth0.is_signed_in());
    assert!(adapter.stored().is_none());
    assert_eq!(adapter.logout_count(), 1);
}

#[tokio::test]
async fn cancelled_login_keeps_the_user_signed_out() {
    let adapter = RuntimeAuth0Adapter::new().with_login_error(Auth0Error::Cancelled);
    let auth0 = Auth0::new(config(), adapter);
    assert_eq!(auth0.login(None).await, Err(Auth0Error::Cancelled));
    assert!(!auth0.is_signed_in());
}

#[tokio::test]
async fn restore_renews_expired_tokens() {
    let adapter = RuntimeAuth0Adapter::new();
    let stored = expired(&adapter, true);
    let adapter = adapter.with_stored(stored.clone());
    let auth0 = Auth0::new(config(), adapter.clone());
    let restored = auth0.restore().await.unwrap().expect("restored");
    assert_ne!(restored.access_token, stored.access_token);
    assert_eq!(auth0.user_id().as_deref(), Some("auth0|stored"));
    assert_eq!(adapter.stored(), Some(restored));
}

#[tokio::test]
async fn restore_without_a_usable_session_clears_it() {
    let adapter = RuntimeAuth0Adapter::new();
    let stored = expired(&adapter, false);
    let adapter = adapter.with_stored(stored);
    let auth0 = Auth0::new(config(), adapter.clone());
    assert_eq!(auth0.restore().await, Ok(None));
    assert!(adapter.stored().is_none());

    let empty = Auth0::new(config(), RuntimeAuth0Adapter::new());
    assert_eq!(empty.restore().await, Ok(None));
}

#[tokio::test]
async fn failed_renewal_clears_the_session_and_reports_the_error() {
    let adapter = RuntimeAuth0Adapter::new();
    let stored = expired(&adapter, true);
    let error = Auth0Error::Native {
        code: "invalid_grant".into(),
        message: "revoked".into(),
    };
    let adapter = adapter.with_stored(stored).with_renew_error(error.clone());
    let auth0 = Auth0::new(config(), adapter.clone());
    assert_eq!(auth0.restore().await, Err(error));
    assert!(adapter.stored().is_none());
    assert!(!auth0.is_signed_in());
}

#[cfg(feature = "functions")]
#[tokio::test]
async fn functions_adapter_adds_the_domain_and_token() {
    use masamune_whisker_auth0::Auth0FunctionsAdapter;
    use masamune_whisker_functions::{FunctionRequest, Functions, RuntimeFunctionsAdapter};
    use serde_json::{Value, json};

    let adapter = RuntimeAuth0Adapter::new();
    let auth0 = Auth0::new(config(), adapter);
    let bodies = Rc::new(RefCell::new(Vec::<Option<Value>>::new()));
    let seen = bodies.clone();
    let functions = Functions::new(Auth0FunctionsAdapter::new(
        RuntimeFunctionsAdapter::new(move |request| {
            seen.borrow_mut().push(request.body.clone());
            async { Ok(Value::Null) }
        }),
        auth0.clone(),
    ));

    functions
        .send(FunctionRequest::new("public"))
        .await
        .unwrap();
    let credentials = auth0.login(None).await.unwrap();
    functions
        .send(FunctionRequest::new("mine").with_body(json!({"id": 1})))
        .await
        .unwrap();

    let bodies = bodies.borrow();
    assert_eq!(
        bodies[0],
        Some(json!({"auth0_domain": "example.jp.auth0.com"}))
    );
    assert_eq!(
        bodies[1],
        Some(json!({
            "id": 1,
            "auth0_domain": "example.jp.auth0.com",
            "auth0_access_token": credentials.access_token,
        }))
    );
}

#[test]
fn primary_reads_the_installed_adapter() {
    use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};
    use whisker::{Owner, spawn_local};

    let observed = Rc::new(RefCell::new(None));
    let result = observed.clone();
    let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
    runtime.enter(|| {
        let root = Owner::new(None);
        root.with(|| {
            assert!(Auth0::primary().is_none());
            MasamuneAdapters::new()
                .adapter(Auth0MasamuneAdapter::new(
                    config(),
                    RuntimeAuth0Adapter::new(),
                ))
                .install();
            spawn_local(async move {
                let auth0 = Auth0::primary().expect("installed");
                auth0.login(None).await.unwrap();
                let auth = auth0_auth();
                *result.borrow_mut() = Some((auth.user_id(), auth.token(false).await.unwrap()));
            });
        });
        whisker::run_until_stalled();
        root.dispose();
    });
    runtime.shutdown();
    let (user_id, token) = observed.take().expect("finished");
    assert_eq!(user_id.as_deref(), Some("runtime-user"));
    assert!(token.unwrap().starts_with("runtime-access-token-"));
}
