use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_auth0::{
    Auth0, Auth0Config, Auth0Credentials, Auth0Error, Auth0User, RuntimeAuth0Adapter,
};
use masamune_whisker_auth0_firebase::{
    Auth0CustomTokenAction, Auth0Firebase, Auth0FirebaseError, FirebaseSession, SessionFuture,
};
use masamune_whisker_functions::{
    ApiError, FunctionRequest, Functions, FunctionsAction, FunctionsError, RuntimeFunctionsAdapter,
};
use serde_json::{Value, json};

const DOMAIN: &str = "example.jp.auth0.com";

#[derive(Default)]
struct SessionState {
    uid: Option<String>,
    sign_ins: Vec<String>,
    sign_outs: u32,
}

/// Fake Firebase Auth: a custom token `token-for:<uid>` signs in `<uid>`.
#[derive(Clone, Default)]
struct FakeSession(Rc<RefCell<SessionState>>);

impl FakeSession {
    fn signed_in(uid: &str) -> Self {
        let session = Self::default();
        session.0.borrow_mut().uid = Some(uid.into());
        session
    }
    fn uid(&self) -> Option<String> {
        self.0.borrow().uid.clone()
    }
    fn sign_ins(&self) -> Vec<String> {
        self.0.borrow().sign_ins.clone()
    }
    fn sign_outs(&self) -> u32 {
        self.0.borrow().sign_outs
    }
}

impl FirebaseSession for FakeSession {
    fn current_uid(&self) -> Option<String> {
        self.uid()
    }

    fn sign_in_with_custom_token<'a>(&'a self, token: &'a str) -> SessionFuture<'a, String> {
        Box::pin(async move {
            let uid = token
                .strip_prefix("token-for:")
                .ok_or_else(|| "invalid custom token".to_string())?
                .to_owned();
            let mut state = self.0.borrow_mut();
            state.uid = Some(uid.clone());
            state.sign_ins.push(uid.clone());
            Ok(uid)
        })
    }

    fn sign_out(&self) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        state.uid = None;
        state.sign_outs += 1;
        Ok(())
    }
}

fn user(sub: &str) -> Auth0User {
    Auth0User {
        sub: sub.into(),
        email: Some(format!("{sub}@example.com")),
        name: None,
    }
}

fn auth0(adapter: &RuntimeAuth0Adapter) -> Auth0 {
    Auth0::new(
        Auth0Config::new(DOMAIN, "client", "com.example.app"),
        adapter.clone(),
    )
}

type Requests = Rc<RefCell<Vec<FunctionRequest>>>;

/// `auth0_custom_token` that trusts the access token and answers with the user it belongs to.
fn functions(adapter: &RuntimeAuth0Adapter) -> (Functions, Requests) {
    let requests = Requests::default();
    let recorded = requests.clone();
    let adapter = adapter.clone();
    let functions = Functions::new(RuntimeFunctionsAdapter::new(move |request| {
        recorded.borrow_mut().push(request.clone());
        let stored = adapter.stored();
        async move {
            assert_eq!(request.action, "auth0_custom_token");
            let body = request.body.unwrap_or(Value::Null);
            let stored = stored.ok_or(FunctionsError::Api(ApiError::NotSignedIn))?;
            assert_eq!(body["auth0_access_token"], json!(stored.access_token));
            assert_eq!(body["auth0_domain"], json!(DOMAIN));
            Ok(json!({ "token": format!("token-for:{}", stored.user.sub) }))
        }
    }));
    (functions, requests)
}

fn stored(adapter: &RuntimeAuth0Adapter, sub: &str) -> Auth0Credentials {
    adapter.issue(&user(sub))
}

#[tokio::test]
async fn login_signs_in_to_firebase_and_runs_the_callback() {
    let adapter = RuntimeAuth0Adapter::new().with_user(user("auth0|alice"));
    let auth0 = auth0(&adapter);
    let (functions, requests) = functions(&adapter);
    let session = FakeSession::signed_in("previous-user");
    let flow = Auth0Firebase::new(session.clone());
    let called = Rc::new(RefCell::new(None));
    let seen = called.clone();

    let credentials = flow
        .login(&auth0, &functions, |credentials: &Auth0Credentials| {
            *seen.borrow_mut() = Some(credentials.user.sub.clone());
            async { Ok::<(), String>(()) }
        })
        .await
        .unwrap()
        .expect("signed in");

    assert_eq!(credentials.user.sub, "auth0|alice");
    assert_eq!(called.borrow().as_deref(), Some("auth0|alice"));
    assert_eq!(adapter.login_prompts(), vec![Some("login".to_string())]);
    assert_eq!(session.sign_outs(), 1);
    assert_eq!(session.uid().as_deref(), Some("auth0|alice"));
    assert_eq!(requests.borrow().len(), 1);
    assert_eq!(auth0.user_id().as_deref(), Some("auth0|alice"));
}

#[tokio::test]
async fn cancelled_login_changes_nothing() {
    let adapter = RuntimeAuth0Adapter::new().with_login_error(Auth0Error::Cancelled);
    let auth0 = auth0(&adapter);
    let (functions, requests) = functions(&adapter);
    let session = FakeSession::signed_in("someone");
    let result = Auth0Firebase::new(session.clone())
        .login(&auth0, &functions, |_: &Auth0Credentials| async {
            Ok::<(), String>(())
        })
        .await;
    assert_eq!(result, Ok(None));
    assert_eq!(session.uid().as_deref(), Some("someone"));
    assert!(requests.borrow().is_empty());
    assert!(!auth0.is_signed_in());
}

#[tokio::test]
async fn failing_callback_clears_both_sessions() {
    let adapter = RuntimeAuth0Adapter::new().with_user(user("auth0|bob"));
    let auth0 = auth0(&adapter);
    let (functions, _) = functions(&adapter);
    let session = FakeSession::default();
    let result = Auth0Firebase::new(session.clone())
        .login(&auth0, &functions, |_: &Auth0Credentials| async {
            Err("status 500")
        })
        .await;
    assert_eq!(
        result,
        Err(Auth0FirebaseError::Callback("status 500".into()))
    );
    assert_eq!(session.uid(), None);
    assert!(adapter.stored().is_none());
    assert!(!auth0.is_signed_in());
}

#[tokio::test]
async fn restore_keeps_a_matching_firebase_user() {
    let adapter = RuntimeAuth0Adapter::new();
    let previous = stored(&adapter, "auth0|carol");
    let adapter = adapter.with_stored(previous);
    let auth0 = auth0(&adapter);
    let (functions, requests) = functions(&adapter);
    let session = FakeSession::signed_in("auth0|carol");
    let restored = Auth0Firebase::new(session.clone())
        .restore(&auth0, &functions)
        .await
        .unwrap()
        .expect("restored");
    assert_eq!(restored.user.sub, "auth0|carol");
    assert_eq!(session.sign_outs(), 0);
    assert!(requests.borrow().is_empty());
    assert!(auth0.is_signed_in());
}

#[tokio::test]
async fn restore_exchanges_again_for_another_firebase_user() {
    let adapter = RuntimeAuth0Adapter::new();
    let previous = stored(&adapter, "auth0|dave");
    let adapter = adapter.with_stored(previous);
    let auth0 = auth0(&adapter);
    let (functions, requests) = functions(&adapter);
    let session = FakeSession::signed_in("anonymous-uid");
    Auth0Firebase::new(session.clone())
        .restore(&auth0, &functions)
        .await
        .unwrap()
        .expect("restored");
    assert_eq!(session.sign_outs(), 1);
    assert_eq!(session.sign_ins(), vec!["auth0|dave".to_string()]);
    assert_eq!(requests.borrow().len(), 1);
}

#[tokio::test]
async fn restore_without_auth0_signs_out_of_firebase() {
    let adapter = RuntimeAuth0Adapter::new();
    let auth0 = auth0(&adapter);
    let (functions, _) = functions(&adapter);
    let session = FakeSession::signed_in("stale");
    let restored = Auth0Firebase::new(session.clone())
        .restore(&auth0, &functions)
        .await
        .unwrap();
    assert!(restored.is_none());
    assert_eq!(session.uid(), None);
}

#[tokio::test]
async fn logout_clears_both_sessions() {
    let adapter = RuntimeAuth0Adapter::new();
    let auth0 = auth0(&adapter);
    let (functions, _) = functions(&adapter);
    let session = FakeSession::default();
    let flow = Auth0Firebase::new(session.clone());
    flow.login(&auth0, &functions, |_: &Auth0Credentials| async {
        Ok::<(), String>(())
    })
    .await
    .unwrap();
    flow.logout(&auth0).await.unwrap();
    assert_eq!(session.uid(), None);
    assert_eq!(adapter.logout_count(), 1);
    assert!(!auth0.is_signed_in());
}

#[test]
fn custom_token_action_contract() {
    let action = Auth0CustomTokenAction::new("access", DOMAIN);
    let request = action.request().unwrap();
    assert_eq!(request.action, "auth0_custom_token");
    assert_eq!(
        request.body,
        Some(json!({"auth0_access_token": "access", "auth0_domain": DOMAIN}))
    );
    assert_eq!(action.decode(json!({"token": "t"})).unwrap(), "t");
    assert_eq!(action.decode(json!("t")).unwrap(), "t");
    assert!(action.decode(json!({"token": ""})).is_err());
    assert!(action.decode(Value::Null).is_err());
    assert!(!format!("{action:?}").contains("access\""));
}
