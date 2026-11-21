use std::cell::RefCell;

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};
use whisker::prelude::*;

use crate::{Link, Subscription, get_initial_link, on_link, parse_link};

/// Which links [`DeeplinkMasamuneAdapter`] accepts.
///
/// The OS only opens the app for the schemes and domains it registered, but any app can send a
/// custom scheme link, so the adapter checks again. Empty lists accept everything.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeeplinkConfig {
    /// Accepted schemes, compared case-insensitively (for example `["segue", "https"]`).
    pub schemes: Vec<String>,
    /// Accepted hosts of `http` / `https` links, compared case-insensitively (for example
    /// `["segue.day"]`).
    pub hosts: Vec<String>,
}

impl DeeplinkConfig {
    /// Parses `url` and returns it when the scheme and (for web links) the host are accepted.
    pub fn accept(&self, url: &str) -> Option<Link> {
        let link = parse_link(url)?;
        let scheme_ok = self.schemes.is_empty()
            || self
                .schemes
                .iter()
                .any(|scheme| scheme.eq_ignore_ascii_case(&link.scheme));
        let web = link.scheme == "http" || link.scheme == "https";
        let host_ok = !web
            || self.hosts.is_empty()
            || self
                .hosts
                .iter()
                .any(|host| host.eq_ignore_ascii_case(&link.host));
        (scheme_ok && host_ok).then_some(link)
    }
}

/// Receiving links for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `DeeplinkMasamuneAdapter`).
///
/// `prepare` subscribes to [`on_link`] once per runtime and `start` reads [`get_initial_link`].
/// Every link that passes [`DeeplinkConfig::accept`] is published to [`latest_link`] and kept for
/// [`take_pending`].
#[derive(Debug, Clone, Default)]
pub struct DeeplinkMasamuneAdapter {
    pub config: DeeplinkConfig,
}

impl DeeplinkMasamuneAdapter {
    pub fn new(config: DeeplinkConfig) -> Self {
        Self { config }
    }
}

whisker::runtime_local! {
    static SUBSCRIPTION: RefCell<Option<Subscription>> = RefCell::new(None);
    static LATEST: RwSignal<Option<Link>> = signal(None);
}

impl MasamuneAdapter for DeeplinkMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        SUBSCRIPTION.with(|subscription| {
            let mut subscription = subscription.borrow_mut();
            if subscription.is_none() {
                *subscription = Some(on_link(|url| deliver(&url)));
            }
        });
        Ok(())
    }

    fn start(&self) -> Option<StartFuture> {
        Some(Box::pin(async {
            if let Some(url) = get_initial_link().await {
                deliver(&url);
            }
            Ok::<(), AdapterError>(())
        }))
    }
}

/// Filters with the installed adapter's settings (accept everything when none is installed) and
/// publishes the result. Rejected links are logged without their content.
fn deliver(url: &str) {
    let config = DeeplinkMasamuneAdapter::primary()
        .map(|adapter| adapter.config.clone())
        .unwrap_or_default();
    match config.accept(url) {
        Some(link) => LATEST.with(|latest| latest.set(Some(link))),
        None => log::debug!("deeplink.rejected"),
    }
}

/// The most recent accepted link that has not been taken with [`take_pending`]. Watch it to
/// navigate when a link arrives.
pub fn latest_link() -> ReadSignal<Option<Link>> {
    LATEST.with(|latest| latest.read_only())
}

/// Takes the most recent accepted link, clearing [`latest_link`]. Returns `None` when there is
/// none or it was already taken.
pub fn take_pending() -> Option<Link> {
    LATEST.with(|latest| {
        let pending = latest.get_untracked();
        if pending.is_some() {
            latest.set(None);
        }
        pending
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_configured_schemes_and_hosts() {
        let config = DeeplinkConfig {
            schemes: vec!["segue".into(), "HTTPS".into()],
            hosts: vec!["segue.day".into()],
        };
        assert!(config.accept("segue://track/1").is_some());
        assert!(config.accept("https://SEGUE.day/app/x").is_some());
        assert!(config.accept("https://evil.example/app/x").is_none());
        assert!(config.accept("http://segue.day/app/x").is_none());
        assert!(config.accept("other://x").is_none());
        assert!(config.accept("not a link").is_none());
        let open = DeeplinkConfig::default();
        assert!(open.accept("anything://x").is_some());
        assert!(open.accept("https://example.com").is_some());
    }
}
