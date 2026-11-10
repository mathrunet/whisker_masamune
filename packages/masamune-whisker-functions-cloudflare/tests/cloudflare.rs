use masamune_whisker_functions::*;
use masamune_whisker_functions_cloudflare::CloudflareFunctionsAdapter;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::*};

#[tokio::test]
async fn routes_edge_and_region_with_shared_auth() {
    let edge = MockServer::start().await;
    let region = MockServer::start().await;
    for (server, expected) in [(&edge, "edge"), (&region, "region")] {
        Mock::given(method("GET"))
            .and(path("/api/action"))
            .and(header("authorization", "Bearer t"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"worker": expected})))
            .expect(1)
            .mount(server)
            .await;
    }
    let adapter = CloudflareFunctionsAdapter::new(format!("{}/api/", edge.uri()))
        .with_auth(AuthProvider::fixed("t"))
        .with_region_endpoint(format!("{}/api", region.uri()));
    for worker in [None, Some("region")] {
        let mut request = FunctionRequest::new("action");
        request.method = Method::GET;
        request.worker_type = worker.map(str::to_owned);
        assert_eq!(
            adapter.send(request).await.unwrap(),
            json!({"worker": worker.unwrap_or("edge")})
        );
    }
    let mut request = FunctionRequest::new("action");
    request.worker_type = Some("unknown".into());
    assert!(matches!(
        adapter.send(request).await,
        Err(FunctionsError::Configuration(_))
    ));
}

#[tokio::test]
async fn rejects_missing_region_before_auth_or_network() {
    let adapter = CloudflareFunctionsAdapter::new("http://127.0.0.1:1").with_auth(
        AuthProvider::new(|_| async {
            panic!("invalid route must not acquire a token");
        }),
    );
    let mut request = FunctionRequest::new("action");
    request.worker_type = Some("region".into());
    assert!(matches!(
        adapter.send(request).await,
        Err(FunctionsError::Configuration(_))
    ));
}
