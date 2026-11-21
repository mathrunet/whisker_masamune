//! A small URL splitter for app links (no dependency on a full URL parser).

/// The parts of a link the app routes on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Link {
    /// The link as received.
    pub url: String,
    /// Lowercased scheme (`https`, `myapp`).
    pub scheme: String,
    /// Lowercased host without user info or port (`segue.day`, or `track` in `segue://track/1`).
    pub host: String,
    /// Percent-decoded path, starting with `/` when present (`/app/track/1`).
    pub path: String,
    /// Percent-decoded query parameters in order (`+` reads as a space).
    pub query: Vec<(String, String)>,
    /// Percent-decoded fragment without `#`.
    pub fragment: Option<String>,
}

impl Link {
    /// The first value of the query parameter `name`.
    pub fn query_value(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Path segments without empty ones (`/app/track/1` → `["app", "track", "1"]`).
    pub fn segments(&self) -> Vec<&str> {
        self.path
            .split('/')
            .filter(|segment| !segment.is_empty())
            .collect()
    }
}

/// Splits `url` into a [`Link`]. Returns `None` without a valid scheme followed by `:`.
pub fn parse_link(url: &str) -> Option<Link> {
    let url = url.trim();
    let (scheme, rest) = url.split_once(':')?;
    if scheme.is_empty()
        || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return None;
    }
    let (rest, fragment) = match rest.split_once('#') {
        Some((rest, fragment)) => (rest, Some(decode(fragment, false))),
        None => (rest, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, parse_query(query)),
        None => (rest, Vec::new()),
    };
    let (host, path) = match rest.strip_prefix("//") {
        Some(authority_and_path) => {
            let (authority, path) = match authority_and_path.find('/') {
                Some(index) => authority_and_path.split_at(index),
                None => (authority_and_path, ""),
            };
            let host = authority.rsplit('@').next().unwrap_or_default();
            let host = if host.starts_with('[') {
                host.split_inclusive(']').next().unwrap_or(host)
            } else {
                host.split(':').next().unwrap_or(host)
            };
            (host.to_ascii_lowercase(), decode(path, false))
        }
        None => (String::new(), decode(rest, false)),
    };
    Some(Link {
        url: url.to_string(),
        scheme: scheme.to_ascii_lowercase(),
        host,
        path,
        query,
        fragment,
    })
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (decode(key, true), decode(value, true)),
            None => (decode(pair, true), String::new()),
        })
        .collect()
}

/// Percent-decodes `value` (invalid escapes are kept as they are; invalid UTF-8 is replaced).
fn decode(value: &str, plus_as_space: bool) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = |b: u8| (b as char).to_digit(16);
                match (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                    (Some(high), Some(low)) => {
                        out.push((high * 16 + low) as u8);
                        index += 3;
                        continue;
                    }
                    _ => out.push(b'%'),
                }
            }
            b'+' if plus_as_space => out.push(b' '),
            byte => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_app_links() {
        let link =
            parse_link("https://Segue.day:443/app/track/a%20b?ref=share&x=1+2&flag#top").unwrap();
        assert_eq!(link.scheme, "https");
        assert_eq!(link.host, "segue.day");
        assert_eq!(link.path, "/app/track/a b");
        assert_eq!(link.segments(), ["app", "track", "a b"]);
        assert_eq!(link.query_value("ref"), Some("share"));
        assert_eq!(link.query_value("x"), Some("1 2"));
        assert_eq!(link.query_value("flag"), Some(""));
        assert_eq!(link.query_value("missing"), None);
        assert_eq!(link.fragment.as_deref(), Some("top"));
    }

    #[test]
    fn parses_custom_schemes() {
        let link = parse_link("segue://track/123?from=lp").unwrap();
        assert_eq!(link.scheme, "segue");
        assert_eq!(link.host, "track");
        assert_eq!(link.path, "/123");
        assert_eq!(link.query, vec![("from".to_string(), "lp".to_string())]);
        let opaque = parse_link("mailto:someone@example.com").unwrap();
        assert_eq!(opaque.host, "");
        assert_eq!(opaque.path, "someone@example.com");
        let user = parse_link("https://user:pass@example.com/").unwrap();
        assert_eq!(user.host, "example.com");
        let ipv6 = parse_link("http://[::1]:8080/x").unwrap();
        assert_eq!(ipv6.host, "[::1]");
    }

    #[test]
    fn rejects_invalid_links_and_keeps_bad_escapes() {
        assert_eq!(parse_link(""), None);
        assert_eq!(parse_link("/relative/path"), None);
        assert_eq!(parse_link("1abc://x"), None);
        assert_eq!(parse_link("a b://x"), None);
        assert_eq!(decode("%zz%4", false), "%zz%4");
        assert_eq!(decode("%E3%81%82", false), "あ");
        assert_eq!(decode("%", false), "%");
    }
}
