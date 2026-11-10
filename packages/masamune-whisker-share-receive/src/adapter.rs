use std::cell::RefCell;

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};
use whisker::prelude::*;

use crate::{SharedPayload, Subscription, get_initial_share, on_share};

/// Image extensions accepted by default.
pub const DEFAULT_IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "gif", "webp"];

/// Longest text accepted by default, in characters.
pub const DEFAULT_MAX_TEXT_LEN: usize = 10_000;

/// Which shares [`ShareReceiveMasamuneAdapter`] keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareReceiveConfig {
    /// Accepted image file extensions, without the dot (compared case-insensitively). Other
    /// files are dropped from [`SharedPayload::image_paths`].
    pub image_extensions: Vec<String>,
    /// Longest accepted text in characters. Longer text is dropped with a warning.
    pub max_text_len: usize,
    /// Text starting with one of these is dropped (for example the app's own `myapp://` links).
    pub ignore_prefixes: Vec<String>,
}

impl Default for ShareReceiveConfig {
    fn default() -> Self {
        Self {
            image_extensions: DEFAULT_IMAGE_EXTENSIONS.map(String::from).into(),
            max_text_len: DEFAULT_MAX_TEXT_LEN,
            ignore_prefixes: Vec::new(),
        }
    }
}

/// Keeps the parts of `payload` that `config` accepts; `None` when nothing is left.
///
/// - Text that is empty or only whitespace is dropped.
/// - Text starting with one of `ignore_prefixes` is dropped.
/// - Text longer than `max_text_len` characters is dropped and logged with `log::warn!` (the
///   text itself is not logged).
/// - Images whose extension is not in `image_extensions` are dropped.
pub fn filter_payload(
    payload: SharedPayload,
    config: &ShareReceiveConfig,
) -> Option<SharedPayload> {
    let text = payload.text.filter(|text| {
        if text.trim().is_empty() {
            return false;
        }
        if config
            .ignore_prefixes
            .iter()
            .any(|prefix| !prefix.is_empty() && text.starts_with(prefix.as_str()))
        {
            return false;
        }
        let length = text.chars().count();
        if length > config.max_text_len {
            log::warn!(
                "share_receive.text_too_long: {length} > {}",
                config.max_text_len
            );
            return false;
        }
        true
    });
    let image_paths: Vec<String> = payload
        .image_paths
        .into_iter()
        .filter(|path| has_extension(path, &config.image_extensions))
        .collect();
    let filtered = SharedPayload { text, image_paths };
    (!filtered.is_empty()).then_some(filtered)
}

fn has_extension(path: &str, extensions: &[String]) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty()
        && extensions.iter().any(|wanted| {
            wanted
                .trim_start_matches('.')
                .eq_ignore_ascii_case(extension)
        })
}

/// Receiving shares for [`masamune_whisker_adapter::MasamuneAdapters`] (the counterpart of
/// Flutter's `receive_sharing_intent` setup).
///
/// `prepare` subscribes to [`on_share`] once per runtime and `start` reads
/// [`get_initial_share`]. Every share that passes [`filter_payload`] with the installed
/// [`ShareReceiveConfig`] is published to [`latest_share`] and kept for [`take_pending`].
///
/// The intent filters, launch mode and the iOS Share Extension come from the
/// [`crate::cng::MasamuneShareReceive`] CNG plugin.
#[derive(Debug, Clone, Default)]
pub struct ShareReceiveMasamuneAdapter {
    pub config: ShareReceiveConfig,
}

impl ShareReceiveMasamuneAdapter {
    pub fn new(config: ShareReceiveConfig) -> Self {
        Self { config }
    }
}

whisker::runtime_local! {
    static SUBSCRIPTION: RefCell<Option<Subscription>> = RefCell::new(None);
    static LATEST: RwSignal<Option<SharedPayload>> = signal(None);
}

impl MasamuneAdapter for ShareReceiveMasamuneAdapter {
    fn prepare(&self) -> Result<(), AdapterError> {
        SUBSCRIPTION.with(|subscription| {
            let mut subscription = subscription.borrow_mut();
            if subscription.is_none() {
                *subscription = Some(on_share(deliver));
            }
        });
        Ok(())
    }

    fn start(&self) -> Option<StartFuture> {
        Some(Box::pin(async {
            if let Some(payload) = get_initial_share().await {
                deliver(payload);
            }
            Ok::<(), AdapterError>(())
        }))
    }
}

/// Filters with the installed adapter's settings (the defaults when none is installed) and
/// publishes the result.
fn deliver(payload: SharedPayload) {
    let config = ShareReceiveMasamuneAdapter::primary()
        .map(|adapter| adapter.config.clone())
        .unwrap_or_default();
    if let Some(payload) = filter_payload(payload, &config) {
        LATEST.with(|latest| latest.set(Some(payload)));
    }
}

/// The most recent accepted share that has not been taken with [`take_pending`]. Watch it to
/// react to shares (for example to open a compose screen).
pub fn latest_share() -> ReadSignal<Option<SharedPayload>> {
    LATEST.with(|latest| latest.read_only())
}

/// Takes the most recent accepted share, clearing [`latest_share`]. Returns `None` when there
/// is none or it was already taken.
pub fn take_pending() -> Option<SharedPayload> {
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

    fn payload(text: Option<&str>, images: &[&str]) -> SharedPayload {
        SharedPayload {
            text: text.map(String::from),
            image_paths: images.iter().map(|path| path.to_string()).collect(),
        }
    }

    #[test]
    fn keeps_text_and_matching_images() {
        let config = ShareReceiveConfig::default();
        assert_eq!(
            filter_payload(
                payload(
                    Some("https://example.com"),
                    &["/c/a.JPG", "/c/b.webp", "/c/c.pdf", "/c/noext", "/c/.png"]
                ),
                &config
            ),
            Some(payload(
                Some("https://example.com"),
                &["/c/a.JPG", "/c/b.webp"]
            ))
        );
    }

    #[test]
    fn drops_blank_and_prefixed_text() {
        let config = ShareReceiveConfig {
            ignore_prefixes: vec!["samurai://".into(), String::new()],
            ..Default::default()
        };
        assert_eq!(filter_payload(payload(Some(" \n"), &[]), &config), None);
        assert_eq!(filter_payload(payload(Some(""), &[]), &config), None);
        assert_eq!(
            filter_payload(payload(Some("samurai://open"), &["/c/a.png"]), &config),
            Some(payload(None, &["/c/a.png"]))
        );
        assert_eq!(
            filter_payload(payload(Some("open samurai://"), &[]), &config),
            Some(payload(Some("open samurai://"), &[]))
        );
    }

    #[test]
    fn drops_text_over_the_limit_in_characters() {
        let config = ShareReceiveConfig {
            max_text_len: 3,
            ..Default::default()
        };
        assert_eq!(
            filter_payload(payload(Some("あいう"), &[]), &config),
            Some(payload(Some("あいう"), &[]))
        );
        assert_eq!(filter_payload(payload(Some("abcd"), &[]), &config), None);
    }

    #[test]
    fn custom_extensions_and_empty_results() {
        let config = ShareReceiveConfig {
            image_extensions: vec![".HEIC".into()],
            ..Default::default()
        };
        assert_eq!(
            filter_payload(payload(None, &["/c/a.heic", "/c/b.png"]), &config),
            Some(payload(None, &["/c/a.heic"]))
        );
        assert_eq!(filter_payload(payload(None, &["/c/b.png"]), &config), None);
        assert_eq!(filter_payload(SharedPayload::default(), &config), None);
    }
}
