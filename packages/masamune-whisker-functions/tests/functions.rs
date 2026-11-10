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
    let adapter = RestApiFunctionsAdapter::new(format!("{}/api", server.uri())).with_auth(
        AuthProvider::new(move |_| {
            observed.set(observed.get() + 1);
            async { Ok(Some("fresh".into())) }
        }),
    );
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
                .set_delay(Duration::from_millis(200))
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
    request.timeout = Some(Duration::from_millis(20));
    assert_eq!(
        adapter.send(request).await,
        Err(FunctionsError::Api(ApiError::Timeout))
    );
    let adapter = adapter
        .with_auth(AuthProvider::new(|_| async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(None)
        }))
        .with_timeout(Duration::from_millis(10));
    assert_eq!(
        adapter.send(FunctionRequest::new("never-sent")).await,
        Err(FunctionsError::Api(ApiError::Timeout))
    );
}
