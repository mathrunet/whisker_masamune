//! Data access, with the adapter chosen by flavor.

use masamune_whisker_functions::{
    FunctionRequest, Functions, FunctionsAction, FunctionsResult, RestApiFunctionsAdapter,
    RuntimeFunctionsAdapter,
};
use serde::Deserialize;
use serde_json::json;

/// Loads the saved counter value.
pub struct LoadCount;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct CountResponse {
    pub count: u32,
}

impl FunctionsAction for LoadCount {
    type Response = CountResponse;

    fn request(&self) -> FunctionsResult<FunctionRequest> {
        Ok(FunctionRequest::new("load_count"))
    }
}

/// In-memory backend used by the test flavor: every launch starts from zero.
pub fn runtime_adapter() -> RuntimeFunctionsAdapter {
    RuntimeFunctionsAdapter::new(|request: FunctionRequest| async move {
        match request.action.as_str() {
            "load_count" => Ok(json!({ "count": 0 })),
            _ => Ok(serde_json::Value::Null),
        }
    })
}

/// Backend of this build: the runtime adapter in the test flavor, the REST API otherwise.
pub fn functions() -> Functions {
    if crate::config::uses_runtime_adapters() {
        Functions::new(runtime_adapter())
    } else {
        Functions::new(RestApiFunctionsAdapter::new(crate::config::API_ENDPOINT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runtime_adapter_starts_from_zero() {
        let functions = Functions::new(runtime_adapter());
        let response = functions.execute(LoadCount).await.unwrap();
        assert_eq!(response, CountResponse { count: 0 });
    }
}
