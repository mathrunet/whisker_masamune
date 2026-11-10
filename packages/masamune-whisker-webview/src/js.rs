//! JavaScript source helpers.

/// Name of the `CustomEvent` dispatched on `window` by [`crate::MasamuneWebViewRef::post_message`].
pub const MESSAGE_EVENT: &str = "whisker:message";

/// Encodes `value` as a double-quoted JavaScript string literal that is safe to embed in any
/// script: quotes, backslashes, control characters, U+2028 / U+2029 and `<` / `>` (so `</script>`
/// cannot end an inline script) are escaped.
pub fn js_string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003C"),
            '>' => out.push_str("\\u003E"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Script that delivers `data` to the page as
/// `window.dispatchEvent(new CustomEvent('whisker:message', { detail: data }))`.
pub fn post_message_script(data: &str) -> String {
    format!(
        "(function(){{window.dispatchEvent(new CustomEvent({event},{{detail:{data}}}));}})();",
        event = js_string_literal(MESSAGE_EVENT),
        data = js_string_literal(data),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_string_literals() {
        assert_eq!(js_string_literal(""), r#""""#);
        assert_eq!(js_string_literal("plain 日本語"), r#""plain 日本語""#);
        assert_eq!(js_string_literal(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(js_string_literal("l1\nl2\r\t"), r#""l1\nl2\r\t""#);
        assert_eq!(
            js_string_literal("\u{0}\u{1f}\u{7f}"),
            r#""\u0000\u001F\u007F""#
        );
        assert_eq!(js_string_literal("\u{2028}\u{2029}"), "\"\\u2028\\u2029\"");
        assert_eq!(
            js_string_literal("</script><script>"),
            r#""\u003C/script\u003E\u003Cscript\u003E""#
        );
        assert_eq!(js_string_literal("😀"), "\"😀\"");
    }

    #[test]
    fn escaped_literals_round_trip_as_json() {
        // A JS string literal produced here is also valid JSON, so serde_json can check it.
        for value in ["", "a\"b", "\\", "\n\u{0}\u{2028}", "{\"k\":[1,2]}", "</x>"] {
            let decoded: String = serde_json::from_str(&js_string_literal(value)).unwrap();
            assert_eq!(decoded, value);
        }
    }

    #[test]
    fn builds_the_message_event_script() {
        assert_eq!(
            post_message_script(r#"{"type":"ping"}"#),
            r#"(function(){window.dispatchEvent(new CustomEvent("whisker:message",{detail:"{\"type\":\"ping\"}"}));})();"#
        );
    }
}
