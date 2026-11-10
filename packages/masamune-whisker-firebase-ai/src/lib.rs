//! Firebase AI Logic (Gemini Developer API).
//!
//! Sends a single request (generate or stream) through the host SDK. The caller owns the conversation
//! history, function execution, and the function-calling loop. Types are JSON close to the Gemini REST API;
//! pass a function call's `thought_signature` back unchanged in the next request.

use std::sync::atomic::{AtomicU64, Ordering};

use futures_channel::mpsc;
use futures_util::StreamExt;
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "FirebaseAi")
}

/// One turn in a conversation. `role` is `user` or `model`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Content {
    pub role: String,
    pub parts: Vec<Part>,
}

impl Content {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            parts: vec![Part::text(text)],
        }
    }

    pub fn model_text(text: impl Into<String>) -> Self {
        Self {
            role: "model".into(),
            parts: vec![Part::text(text)],
        }
    }

    /// The concatenated body text (excluding thoughts).
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter(|p| !p.thought)
            .filter_map(|p| p.text.as_deref())
            .collect()
    }
}

/// Part of a turn: exactly one of text, a function call, or a function response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub thought: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_response: Option<FunctionResponse>,
}

impl Part {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    pub fn function_response(call: &FunctionCall, response: Value) -> Self {
        let response = match response {
            Value::Object(map) => map,
            other => Map::from_iter([("result".to_string(), other)]),
        };
        Self {
            function_response: Some(FunctionResponse {
                name: call.name.clone(),
                response,
                id: call.id.clone(),
            }),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    #[serde(default)]
    pub args: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// Type of a parameter or return value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nullable: bool,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Map<String, Value>>,
}

impl Schema {
    fn of(kind: &'static str, description: Option<&str>) -> Self {
        Self {
            kind,
            description: description.map(str::to_string),
            nullable: false,
            enum_values: None,
            items: None,
            properties: None,
        }
    }

    pub fn string(description: Option<&str>) -> Self {
        Self::of("string", description)
    }

    pub fn integer(description: Option<&str>) -> Self {
        Self::of("integer", description)
    }

    pub fn number(description: Option<&str>) -> Self {
        Self::of("number", description)
    }

    pub fn boolean(description: Option<&str>) -> Self {
        Self::of("boolean", description)
    }

    pub fn enumeration(values: &[&str], description: Option<&str>) -> Self {
        Self {
            enum_values: Some(values.iter().map(|v| v.to_string()).collect()),
            ..Self::of("string", description)
        }
    }

    pub fn array(items: Schema, description: Option<&str>) -> Self {
        Self {
            items: Some(Box::new(items)),
            ..Self::of("array", description)
        }
    }
}

/// A function exposed to the model. All `parameters` are required except those listed in `optional_parameters`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: Map<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub optional_parameters: Vec<String>,
}

impl FunctionDeclaration {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: Map::new(),
            optional_parameters: Vec::new(),
        }
    }

    pub fn parameter(mut self, name: &str, schema: Schema) -> Self {
        self.parameters.insert(
            name.into(),
            serde_json::to_value(schema).unwrap_or(Value::Null),
        );
        self
    }
}

/// Whether the model may call functions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FunctionCallingMode {
    #[default]
    Auto,
    None,
    Any,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
}

/// A single request.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    /// Model name (e.g. `gemini-3.5-flash-lite`).
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<String>,
    pub contents: Vec<Content>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<FunctionDeclaration>,
    pub function_calling_mode: FunctionCallingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GenerationConfig>,
}

/// Token counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default)]
    pub prompt_token_count: u64,
    #[serde(default)]
    pub candidates_token_count: u64,
}

/// A response (or one chunk of it when streaming).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResponse {
    #[serde(default)]
    pub parts: Vec<Part>,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

impl GenerateResponse {
    pub fn function_calls(&self) -> impl Iterator<Item = &FunctionCall> {
        self.parts.iter().filter_map(|p| p.function_call.as_ref())
    }
}

/// Generates content (non-streaming; returns once the full response is ready).
pub async fn generate(request: &GenerateRequest) -> NativeResult<GenerateResponse> {
    module()
        .call_async("generate", &json!({ "request": request }))
        .await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChunkEvent {
    request_id: String,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(flatten)]
    response: GenerateResponse,
}

static NEXT_REQUEST: AtomicU64 = AtomicU64::new(1);

/// Generates content as a stream. `on_chunk` is called for each chunk; returns `Ok` once the stream completes.
///
/// Dropping the future before it returns also stops generation on the host.
pub async fn stream(
    request: &GenerateRequest,
    mut on_chunk: impl FnMut(GenerateResponse),
) -> NativeResult<()> {
    let request_id = format!("request-{}", NEXT_REQUEST.fetch_add(1, Ordering::Relaxed));
    let (tx, mut rx) = mpsc::unbounded::<ChunkEvent>();
    let watched = request_id.clone();
    let _subscription = module().subscribe("chunk", move |event: ChunkEvent| {
        if event.request_id == watched {
            let _ = tx.unbounded_send(event);
        }
    });
    let guard = CancelGuard {
        request_id: request_id.clone(),
        finished: false,
    };
    module().call::<_, bool>(
        "stream",
        &json!({ "requestId": request_id, "request": request }),
    )?;
    let mut guard = guard;
    while let Some(event) = rx.next().await {
        if let Some(error) = event.error {
            guard.finished = true;
            return Err(NativeError(error));
        }
        if event.done {
            guard.finished = true;
            return Ok(());
        }
        on_chunk(event.response);
    }
    guard.finished = true;
    Err(NativeError("native module unavailable".into()))
}

/// Stops generation on the host if dropped midway.
struct CancelGuard {
    request_id: String,
    finished: bool,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        if !self.finished {
            let _ = module().call::<_, bool>("cancel", &json!({ "requestId": self.request_id }));
        }
    }
}
