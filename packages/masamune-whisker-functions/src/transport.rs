use std::{
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::OnceLock,
};

use reqwest::{Client, Url, header::HeaderMap, redirect::Policy};

use crate::{client, client_builder};

/// Selects a client for the final URL and headers before credentials are sent.
pub fn functions_client(
    url: &Url,
    headers: &HeaderMap,
    has_managed_credentials: bool,
    allow_local_http: bool,
) -> Result<&'static Client, &'static str> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err("functions endpoint must not contain user information");
    }
    let authenticated = has_managed_credentials
        || [
            "authorization",
            "x-firebase-appcheck",
            "cookie",
            "proxy-authorization",
        ]
        .iter()
        .any(|header| headers.contains_key(*header));
    match url.scheme() {
        "http" if allow_local_http && is_loopback(url) => Ok(local_client()),
        "https" if authenticated => Ok(authenticated_client()),
        "http" if authenticated => Err("authenticated functions requests require HTTPS"),
        "http" | "https" => Ok(client()),
        _ => Err("functions endpoint must use HTTP or HTTPS"),
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host_str() {
        Some("localhost") => true,
        Some(host) => {
            if let Some(ipv6) = host
                .strip_prefix('[')
                .and_then(|host| host.strip_suffix(']'))
            {
                return ipv6.parse::<Ipv6Addr>().is_ok_and(|ip| ip.is_loopback());
            }
            host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
        }
        None => false,
    }
}

fn authenticated_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        client_builder()
            .https_only(true)
            .redirect(Policy::none())
            .build()
            .expect("authenticated functions HTTP client")
    })
}

fn local_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let loopback = [
            SocketAddr::from((Ipv4Addr::LOCALHOST, 0)),
            SocketAddr::from((Ipv6Addr::LOCALHOST, 0)),
        ];
        client_builder()
            .redirect(Policy::none())
            .no_proxy()
            .resolve_to_addrs("localhost", &loopback)
            .build()
            .expect("local functions HTTP client")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn selects_an_https_only_client_for_authenticated_https() {
        let url = Url::parse("https://example.invalid/").unwrap();
        let client = functions_client(&url, &HeaderMap::new(), true, true).unwrap();
        assert!(std::ptr::eq(client, authenticated_client()));
        let error = client.get("http://127.0.0.1:1").send().await.unwrap_err();
        assert!(error.is_builder());
    }

    #[test]
    fn accepts_only_loopback_for_authenticated_http() {
        for host in ["localhost", "127.0.0.1", "[::1]"] {
            let url = Url::parse(&format!("http://{host}:5050")).unwrap();
            assert!(functions_client(&url, &HeaderMap::new(), true, false).is_err());
            let client = functions_client(&url, &HeaderMap::new(), true, true).unwrap();
            assert!(std::ptr::eq(client, local_client()));
        }
        for endpoint in [
            "http://localhost.example.invalid",
            "http://192.0.2.1",
            "http://[::]",
            "http://dummy:dummy@localhost",
        ] {
            let url = Url::parse(endpoint).unwrap();
            assert!(functions_client(&url, &HeaderMap::new(), true, true).is_err());
        }
    }
}
