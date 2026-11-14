//! Secret redaction and size limits for everything sent to SamuraiAI
//! (`AIDebugController._redact` and `_AIDebugContextSanitizer`).

use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

static BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Bearer\s+[A-Za-z0-9._~+/=-]+").expect("bearer pattern"));
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(token|api[_-]?key|password|secret)\s*[:=]\s*\S+").expect("secret pattern")
});
/// OpenAI Realtime ephemeral keys, alone (`ek_…`) or as the WebSocket subprotocol
/// (`openai-insecure-api-key.…`).
static OPENAI_SUBPROTOCOL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"openai-insecure-api-key\.[^\s,;\x22']+").expect("subprotocol pattern")
});
static EPHEMERAL_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bek_[A-Za-z0-9_-]+").expect("ephemeral key pattern"));
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}").expect("email pattern")
});
static SENSITIVE_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)authorization|cookie|token|api[_-]?key|password|secret|private[_-]?key")
        .expect("sensitive key pattern")
});

/// Masks bearer tokens, `token=…`-style assignments, OpenAI ephemeral keys and email addresses.
///
/// Unlike the Flutter package, which writes a literal `$1`, the assignment keeps its key
/// (`password=[REDACTED]`).
pub fn redact(value: &str) -> String {
    let value = BEARER.replace_all(value, "Bearer [REDACTED]");
    let value = OPENAI_SUBPROTOCOL.replace_all(&value, "openai-insecure-api-key.[REDACTED]");
    let value = EPHEMERAL_KEY.replace_all(&value, "[REDACTED]");
    let value = ASSIGNMENT.replace_all(&value, "${1}=[REDACTED]");
    EMAIL.replace_all(&value, "[REDACTED_EMAIL]").into_owned()
}

/// The first `max` characters of `value`.
pub(crate) fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// Redacts and truncates a context snapshot within a total budget of 24000 characters.
pub(crate) struct Sanitizer {
    remaining: usize,
}

impl Sanitizer {
    pub(crate) fn new() -> Self {
        Self { remaining: 24000 }
    }

    pub(crate) fn text(&mut self, source: &str, max: usize) -> String {
        if self.remaining == 0 {
            return "[TRUNCATED]".into();
        }
        let redacted = redact(source);
        let text = truncate(&redacted, max.min(self.remaining));
        self.remaining -= text.chars().count();
        text
    }

    /// Sensitive keys become `[REDACTED]`, collections keep 50 entries and nesting stops at 5.
    pub(crate) fn value(&mut self, source: &Value, depth: usize, key: Option<&str>) -> Value {
        if key.is_some_and(|key| SENSITIVE_KEY.is_match(key)) {
            return "[REDACTED]".into();
        }
        match source {
            Value::Null | Value::Bool(_) | Value::Number(_) => source.clone(),
            Value::String(text) => self.text(text, 4000).into(),
            _ if depth >= 5 => self.text(&source.to_string(), 500).into(),
            Value::Object(map) => {
                let mut result = Map::new();
                for (entry_key, entry_value) in map.iter().take(50) {
                    let entry_key = self.text(entry_key, 200);
                    let value = self.value(entry_value, depth + 1, Some(&entry_key));
                    result.insert(entry_key, value);
                    if self.remaining == 0 {
                        break;
                    }
                }
                Value::Object(result)
            }
            Value::Array(items) => {
                let mut result = Vec::new();
                for item in items.iter().take(50) {
                    result.push(self.value(item, depth + 1, None));
                    if self.remaining == 0 {
                        break;
                    }
                }
                Value::Array(result)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_secrets() {
        assert_eq!(
            redact("Authorization: Bearer abc.def-123"),
            "Authorization: Bearer [REDACTED]"
        );
        assert_eq!(redact("password=hunter2 ok"), "password=[REDACTED] ok");
        assert_eq!(redact("API_KEY: xyz"), "API_KEY=[REDACTED]");
        assert_eq!(redact("mail me@example.com"), "mail [REDACTED_EMAIL]");
    }

    #[test]
    fn redacts_openai_ephemeral_keys() {
        assert_eq!(
            redact("protocols: realtime, openai-insecure-api-key.ek_abc-123_X, other"),
            "protocols: realtime, openai-insecure-api-key.[REDACTED], other"
        );
        assert_eq!(
            redact(r#"["openai-insecure-api-key.sk-proj.x/y"]"#),
            r#"["openai-insecure-api-key.[REDACTED]"]"#
        );
        assert_eq!(
            redact("key ek_68f1a2B_c-d rejected"),
            "key [REDACTED] rejected"
        );
        // Words that merely contain `ek_` are kept.
        assert_eq!(redact("week_day"), "week_day");
    }

    #[test]
    fn sanitizes_values() {
        let mut sanitizer = Sanitizer::new();
        let value = sanitizer.value(
            &json!({
                "apiToken": "secret",
                "email": "user@example.com",
                "nested": {"a": {"b": {"c": {"d": {"e": {"f": 1}}}}}},
                "list": (0..60).collect::<Vec<_>>(),
                "missing": null,
            }),
            0,
            None,
        );
        assert_eq!(value["apiToken"], "[REDACTED]");
        assert_eq!(value["email"], "[REDACTED_EMAIL]");
        assert_eq!(value["list"].as_array().unwrap().len(), 50);
        assert!(value["missing"].is_null());
        assert_eq!(
            value["nested"]["a"]["b"]["c"]["d"],
            json!(r#"{"e":{"f":1}}"#)
        );
    }

    #[test]
    fn budget_truncates() {
        let mut sanitizer = Sanitizer::new();
        let long = "x".repeat(30000);
        assert_eq!(sanitizer.text(&long, 20000).len(), 20000);
        assert_eq!(sanitizer.text(&long, 20000).len(), 4000);
        assert_eq!(sanitizer.text("y", 10), "[TRUNCATED]");
    }
}
