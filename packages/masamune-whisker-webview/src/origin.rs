//! Origin handling shared with the native navigation checks.
//!
//! An origin is `scheme://host[:port]` with a lowercase scheme and host and without the default
//! port (80 for `http`, 443 for `https`). The native views compute the same string from each
//! navigation URL and compare it against the list sent from Rust.

/// Normalizes `input` (an origin or any `http(s)` URL) to `scheme://host[:port]`.
///
/// Returns `None` when `input` is not an `http` / `https` URL with a host.
pub fn normalize_origin(input: &str) -> Option<String> {
    let input = input.trim();
    let (scheme, rest) = input.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let default_port = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // Drop credentials (`user:pass@`).
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = split_host_port(authority)?;
    if host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    match port {
        Some(port) if port != default_port => Some(format!("{scheme}://{host}:{port}")),
        _ => Some(format!("{scheme}://{host}")),
    }
}

/// The origin of an `http(s)` `url`, see [`normalize_origin`].
pub fn origin_of(url: &str) -> Option<String> {
    normalize_origin(url)
}

/// Whether the main frame may load `url` with `allowed_origins` (already normalized):
/// every `http(s)` URL when the list is empty, otherwise only URLs whose origin is listed.
/// Other schemes are never allowed.
pub fn is_allowed_url(url: &str, allowed_origins: &[String]) -> bool {
    let Some(origin) = origin_of(url) else {
        return false;
    };
    allowed_origins.is_empty() || allowed_origins.contains(&origin)
}

/// Normalizes every entry. An entry that cannot be normalized is kept as is so that it matches
/// nothing: a typo must not turn a restricted list into an empty one, which would allow all.
pub fn normalize_origins(origins: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::with_capacity(origins.len());
    for origin in origins {
        let origin = normalize_origin(origin).unwrap_or_else(|| origin.trim().to_string());
        if !normalized.contains(&origin) {
            normalized.push(origin);
        }
    }
    normalized
}

fn split_host_port(authority: &str) -> Option<(&str, Option<u16>)> {
    if let Some(rest) = authority.strip_prefix('[') {
        // IPv6 literal: keep the brackets, as the platforms report the host with them.
        let end = rest.find(']')?;
        let host = &authority[..end + 2];
        let after = &rest[end + 1..];
        return match after.strip_prefix(':') {
            Some(port) => Some((host, Some(parse_port(port)?))),
            None if after.is_empty() => Some((host, None)),
            None => None,
        };
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => Some((host, Some(parse_port(port)?))),
        None => Some((authority, None)),
    }
}

fn parse_port(port: &str) -> Option<u16> {
    if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    port.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_origins() {
        assert_eq!(
            normalize_origin("HTTPS://Example.COM/path?q#f").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            normalize_origin("https://example.com:443").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            normalize_origin("http://example.com:80/").as_deref(),
            Some("http://example.com")
        );
        assert_eq!(
            normalize_origin("http://example.com:8080").as_deref(),
            Some("http://example.com:8080")
        );
        assert_eq!(
            normalize_origin("https://user:pass@example.com/").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            normalize_origin("http://[::1]:3000/x").as_deref(),
            Some("http://[::1]:3000")
        );
        assert_eq!(
            normalize_origin(" https://a.b ").as_deref(),
            Some("https://a.b")
        );
    }

    #[test]
    fn rejects_non_web_origins() {
        assert_eq!(normalize_origin("file:///etc/passwd"), None);
        assert_eq!(normalize_origin("javascript:alert(1)"), None);
        assert_eq!(normalize_origin("example.com"), None);
        assert_eq!(normalize_origin("https://"), None);
        assert_eq!(normalize_origin("https://example.com:abc"), None);
        assert_eq!(normalize_origin("https://example.com:99999"), None);
    }

    #[test]
    fn checks_urls_against_the_list() {
        let allowed = normalize_origins(&["https://host.example".to_string()]);
        assert!(is_allowed_url("https://host.example/__samurai/", &allowed));
        assert!(is_allowed_url("https://HOST.example:443/a", &allowed));
        assert!(!is_allowed_url("https://host.example:8443/a", &allowed));
        assert!(!is_allowed_url("http://host.example/a", &allowed));
        assert!(!is_allowed_url("https://host.example.evil.com/", &allowed));
        assert!(!is_allowed_url(
            "https://evil.com/?https://host.example",
            &allowed
        ));
        assert!(!is_allowed_url("intent://scan", &allowed));
        assert!(is_allowed_url("https://anything.example/", &[]));
        assert!(!is_allowed_url("mailto:a@b.c", &[]));
    }

    #[test]
    fn invalid_entries_fail_closed() {
        let allowed = normalize_origins(&["not an origin".to_string()]);
        assert_eq!(allowed, ["not an origin"]);
        assert!(!is_allowed_url("https://example.com/", &allowed));
    }

    #[test]
    fn deduplicates_entries() {
        let allowed = normalize_origins(&[
            "https://a.example".to_string(),
            "https://A.example:443/".to_string(),
        ]);
        assert_eq!(allowed, ["https://a.example"]);
    }
}
