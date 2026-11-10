use masamune_whisker_functions::*;
use masamune_whisker_functions_firebase::FirebaseFunctionsAdapter;
use serde_json::{Value, json};
use std::time::Duration;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::*};

#[tokio::test]
async fn callable_envelope_tokens_database_and_typed_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/project/region/run"))
        .and(header("authorization", "Bearer id-token"))
        .and(header("x-firebase-appcheck", "app-token"))
        .and(header("x-custom", "gateway"))
        .and(body_json(
            json!({"data": {"input": 7, "firestoreDatabaseId": "named"}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"result": {"answer": 42}})))
        .expect(1)
        .mount(&server)
        .await;
    let adapter =
        FirebaseFunctionsAdapter::at_endpoint(format!("{}/project/region/", server.uri()))
            .with_auth(Auth::fixed("id-token"))
            .with_app_check(Auth::fixed("app-token"))
            .with_firestore_database_id("named");
    #[derive(serde::Deserialize)]
    struct Reply {
        answer: u32,
    }
    struct Run;
    impl FunctionsAction for Run {
        type Response = Reply;
        fn request(&self) -> FunctionsResult<FunctionRequest> {
            let mut request = FunctionRequest::new("run");
            request.body = Some(json!({"input": 7}));
            request
                .headers
                .insert("authorization", HeaderValue::from_static("Bearer forged"));
            request
                .headers
                .insert("x-firebase-appcheck", HeaderValue::from_static("forged"));
            request
                .headers
                .insert("x-custom", HeaderValue::from_static("gateway"));
            Ok(request)
        }
    }
    assert_eq!(
        Functions::new(adapter).execute(Run).await.unwrap().answer,
        42
    );
}

#[tokio::test]
async fn decodes_callable_errors_on_success_and_failure_http_statuses() {
    let server = MockServer::start().await;
    let adapter = FirebaseFunctionsAdapter::at_endpoint(server.uri());
    for code in [200, 403, 500] {
        let name = format!("fail{code}");
        Mock::given(path(format!("/{name}")))
            .respond_with(ResponseTemplate::new(code).set_body_json(json!({"error": {
                "status": "PERMISSION_DENIED", "message": "denied", "details": {"reason": "owner"}
            }})))
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            adapter.send(FunctionRequest::new(name)).await,
            Err(FunctionsError::Callable(Box::new(
                CallableFailure::Remote {
                    http_status: code,
                    status: "PERMISSION_DENIED".into(),
                    message: "denied".into(),
                    details: json!({"reason": "owner"}),
                }
            )))
        );
    }
}

#[tokio::test]
async fn validates_response_shapes_and_preserves_http_errors() {
    let server = MockServer::start().await;
    let adapter = FirebaseFunctionsAdapter::at_endpoint(server.uri());
    for (route, response) in [
        (
            "null",
            ResponseTemplate::new(200).set_body_json(json!({"data": null})),
        ),
        (
            "missing",
            ResponseTemplate::new(200).set_body_json(json!({})),
        ),
        (
            "baderror",
            ResponseTemplate::new(200).set_body_json(json!({"error": {"message": "bad"}})),
        ),
        ("badjson", ResponseTemplate::new(200).set_body_string("bad")),
        (
            "down",
            ResponseTemplate::new(502).set_body_string("gateway down"),
        ),
    ] {
        Mock::given(path(format!("/{route}")))
            .respond_with(response)
            .expect(1)
            .mount(&server)
            .await;
    }
    assert_eq!(
        adapter.send(FunctionRequest::new("null")).await.unwrap(),
        Value::Null
    );
    for name in ["missing", "baderror", "badjson"] {
        assert!(matches!(
            adapter.send(FunctionRequest::new(name)).await,
            Err(FunctionsError::Api(ApiError::Decode(_)))
        ));
    }
    assert_eq!(
        adapter.send(FunctionRequest::new("down")).await,
        Err(FunctionsError::Api(ApiError::Http {
            status: 502,
            body: "gateway down".into()
        }))
    );
}

#[tokio::test]
async fn token_failures_and_invalid_requests_never_dispatch() {
    let server = MockServer::start().await;
    let adapter = FirebaseFunctionsAdapter::at_endpoint(server.uri());
    let auth_fail = adapter
        .clone()
        .with_auth(Auth::from_fn(|_| async { Err(AuthError::NotSignedIn) }));
    assert_eq!(
        auth_fail.send(FunctionRequest::new("run")).await,
        Err(FunctionsError::Api(ApiError::NotSignedIn))
    );
    let app_fail = adapter.clone().with_app_check(Auth::from_fn(|_| async {
        Err(AuthError::Other("attestation failed".into()))
    }));
    assert_eq!(
        app_fail.send(FunctionRequest::new("run")).await,
        Err(FunctionsError::Api(ApiError::Other(
            "attestation failed".into()
        )))
    );
    let app_missing = adapter.clone().with_app_check(Auth::none());
    assert!(app_missing.send(FunctionRequest::new("run")).await.is_err());
    let mut request = FunctionRequest::new("run");
    request.query.push(("lost".into(), "input".into()));
    assert!(matches!(
        adapter.send(request).await,
        Err(FunctionsError::Configuration(_))
    ));
    assert!(
        adapter
            .send(FunctionRequest::new("../other"))
            .await
            .is_err()
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn timeout_covers_token_and_http_without_retries() {
    let server = MockServer::start().await;
    Mock::given(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(json!({"result": true})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let adapter = FirebaseFunctionsAdapter::at_endpoint(server.uri());
    let mut request = FunctionRequest::new("slow");
    request.timeout = Some(Duration::from_millis(20));
    assert_eq!(
        adapter.send(request).await,
        Err(FunctionsError::Api(ApiError::Timeout))
    );
    let slow_token = adapter
        .with_app_check(Auth::from_fn(|_| async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            Ok(Some("token".into()))
        }))
        .with_timeout(Duration::from_millis(10));
    assert_eq!(
        slow_token.send(FunctionRequest::new("not-sent")).await,
        Err(FunctionsError::Api(ApiError::Timeout))
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
