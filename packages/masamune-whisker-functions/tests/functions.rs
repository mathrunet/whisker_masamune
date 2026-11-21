use masamune_whisker_functions::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::Cell, rc::Rc, time::Duration};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::*};

#[derive(Debug, Deserialize, PartialEq)]
struct Reply {
    ok: bool,
}
struct Action;
impl FunctionsAction for Action {
    type Response = Reply;
    fn request(&self) -> FunctionsResult<FunctionRequest> {
        Ok(FunctionRequest::new("test"))
    }
}

#[tokio::test]
async fn typed_runtime_results_and_error_hook() {
    let functions = Functions::new(RuntimeFunctionsAdapter::new(|request| async move {
        assert_eq!(request.action, "test");
        Ok(json!({"ok": true}))
    }));
    assert_eq!(functions.execute(Action).await.unwrap(), Reply { ok: true });
    let invalid = Functions::new(RuntimeFunctionsAdapter::new(|_| async {
        Ok(json!({"bad": true}))
    }));
    assert!(matches!(
        invalid.execute(Action).await,
        Err(FunctionsError::Api(ApiError::Decode(_)))
    ));
    struct Fallback;
    impl FunctionsAction for Fallback {
        type Response = Reply;
        fn request(&self) -> FunctionsResult<FunctionRequest> {
            Action.request()
        }
        fn on_error(&self, _: FunctionsError) -> FunctionsResult<Reply> {
            Ok(Reply { ok: false })
        }
    }
    assert_eq!(
        invalid.execute(Fallback).await.unwrap(),
        Reply { ok: false }
    );
    let failing = Functions::new(RuntimeFunctionsAdapter::new(|_| async {
        Err(ApiError::NotSignedIn.into())
    }));
    assert_eq!(
        failing.execute(Action).await,
        Err(FunctionsError::Api(ApiError::NotSignedIn))
    );
}

#[tokio::test]
async fn rest_contract_and_non_send_auth() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/api/items/a%20b"))
        .and(query_param("q", "a & b"))
        .and(header("authorization", "Bearer fresh"))
        .and(header("x-custom", "v"))
        .and(body_json(json!({"count": 2})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([1, 2])))
        .expect(1)
        .mount(&server)
        .await;
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    let adapter = RestApiFunctionsAdapter::new(format!("{}/api", server.uri()))
        .allow_local_http()
        .with_auth(Auth::from_fn(move |_| {
            observed.set(observed.get() + 1);
            async { Ok(Some("fresh".into())) }
        }));
    let mut request = FunctionRequest::new("unused");
    request.path = Some("/items/a%20b".into());
    request.method = Method::PATCH;
    request.query.push(("q".into(), "a & b".into()));
    request
        .headers
        .insert("x-custom", HeaderValue::from_static("v"));
    request
        .headers
        .insert("authorization", HeaderValue::from_static("Bearer stale"));
    request.body = Some(json!({"count": 2}));
    assert_eq!(adapter.send(request).await.unwrap(), json!([1, 2]));
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn http_decode_empty_and_deadline_errors() {
    let server = MockServer::start().await;
    for (route, response) in [
        (
            "down",
            ResponseTemplate::new(503).set_body_string("unavailable"),
        ),
        (
            "bad",
            ResponseTemplate::new(200).set_body_string("not json"),
        ),
        ("empty", ResponseTemplate::new(204)),
        (
            "slow",
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(2))
                .set_body_json(json!({})),
        ),
    ] {
        Mock::given(path(format!("/{route}")))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    let adapter = RestApiFunctionsAdapter::new(server.uri());
    assert!(matches!(
        adapter.send(FunctionRequest::new("down")).await,
        Err(FunctionsError::Api(ApiError::Http { status: 503, .. }))
    ));
    assert!(matches!(
        adapter.send(FunctionRequest::new("bad")).await,
        Err(FunctionsError::Api(ApiError::Decode(_)))
    ));
    assert_eq!(
        adapter.send(FunctionRequest::new("empty")).await.unwrap(),
        Value::Null
    );
    let mut request = FunctionRequest::new("slow");
    request.timeout = Some(Duration::from_millis(300));
    assert_eq!(
        adapter.send(request).await,
        Err(FunctionsError::Api(ApiError::Timeout))
    );
    let adapter = adapter
        .with_auth(Auth::from_fn(|_| async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(None)
        }))
        .with_timeout(Duration::from_millis(10));
    assert_eq!(
        adapter.send(FunctionRequest::new("never-sent")).await,
        Err(FunctionsError::Api(ApiError::Timeout))
    );
}

#[tokio::test]
async fn posts_json_with_bearer_token_and_encoded_segments() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/delete_user"))
        .and(header("authorization", "Bearer t"))
        .and(body_json(json!({"userId": "u"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/turso/database/u%201/user"))
        .and(query_param("where", "[1]"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": []})))
        .mount(&server)
        .await;
    let functions = Functions::new(
        RestApiFunctionsAdapter::new(format!("{}/", server.uri()))
            .allow_local_http()
            .with_auth(Auth::fixed("t")),
    );
    let value = functions
        .send(FunctionRequest::new("/delete_user").with_body(json!({"userId": "u"})))
        .await
        .unwrap();
    assert_eq!(value, json!({"ok": true}));
    let request = FunctionRequest::new("turso")
        .with_segments(&["turso", "database", "u 1", "user"])
        .with_method(Method::GET)
        .with_query("where", "[1]");
    assert_eq!(request.path.as_deref(), Some("turso/database/u%201/user"));
    assert_eq!(functions.send(request).await.unwrap(), json!({"data": []}));
}

#[tokio::test]
async fn rest_credentials_require_https_or_an_explicit_loopback() {
    let server = MockServer::start().await;
    let adapter = RestApiFunctionsAdapter::new(server.uri());
    for name in [
        "authorization",
        "x-firebase-appcheck",
        "cookie",
        "proxy-authorization",
    ] {
        let mut request = FunctionRequest::new("run");
        request
            .headers
            .insert(name, HeaderValue::from_static("dummy"));
        assert!(matches!(
            adapter.send(request).await,
            Err(FunctionsError::Configuration(_))
        ));
    }
    assert!(matches!(
        adapter
            .with_auth(Auth::fixed("dummy"))
            .send(FunctionRequest::new("run"))
            .await,
        Err(FunctionsError::Configuration(_))
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn authenticated_rest_stops_at_redirect_response() {
    let server = MockServer::start().await;
    let destination = MockServer::start().await;
    Mock::given(path("/run"))
        .and(header("authorization", "Bearer dummy"))
        .respond_with(ResponseTemplate::new(307).insert_header("location", destination.uri()))
        .expect(1)
        .mount(&server)
        .await;
    let adapter = RestApiFunctionsAdapter::new(server.uri())
        .allow_local_http()
        .with_auth(Auth::fixed("dummy"));
    assert!(matches!(
        adapter.send(FunctionRequest::new("run")).await,
        Err(FunctionsError::Api(ApiError::Http { status: 307, .. }))
    ));
    assert!(destination.received_requests().await.unwrap().is_empty());
}

#[test]
fn error_formatting_uses_only_fixed_text_and_http_status() {
    let sentinel = "dummy private value";
    let http: FunctionsError = ApiError::Http {
        status: 503,
        body: sentinel.into(),
    }
    .into();
    let callable = FunctionsError::from(CallableFailure::Remote {
        http_status: 403,
        status: sentinel.into(),
        message: sentinel.into(),
        details: json!({ "private": sentinel }),
    });
    let network: FunctionsError =
        ApiError::Network(format!("https://example.invalid/{sentinel}")).into();
    let decode: FunctionsError = ApiError::Decode(sentinel.into()).into();
    let other: FunctionsError = ApiError::Other(sentinel.into()).into();
    let configuration = FunctionsError::Configuration(sentinel.into());
    for (error, expected) in [
        (http, "http 503"),
        (callable, "callable error (http 403)"),
        (network, "network error"),
        (decode, "decode error"),
        (other, "request failed"),
        (configuration, "functions configuration error"),
    ] {
        assert_eq!(error.to_string(), expected);
        assert_eq!(format!("{error:?}"), expected);
    }
}

#[test]
fn flattens_errors_for_backend_crates() {
    let remote = FunctionsError::from(CallableFailure::Remote {
        http_status: 404,
        status: "NOT_FOUND".into(),
        message: "missing".into(),
        details: Value::Null,
    });
    assert!(ApiError::from(remote).is_not_found());
    assert_eq!(
        ApiError::from(FunctionsError::from(AuthError::NotSignedIn)),
        ApiError::NotSignedIn
    );
    assert_eq!(
        ApiError::from(FunctionsError::Configuration("x".into())),
        ApiError::Other("x".into())
    );
}
