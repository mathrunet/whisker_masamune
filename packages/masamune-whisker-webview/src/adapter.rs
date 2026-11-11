use std::cell::RefCell;

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter};

use crate::origin::normalize_origins;

/// Largest download payload accepted by default (28 MiB, matching the Flutter version).
pub const DEFAULT_MAX_DOWNLOAD_BYTES: u64 = 28 * 1024 * 1024;

/// App-wide defaults of [`crate::MasamuneWebView`] for
/// [`masamune_whisker_adapter::MasamuneAdapters`].
///
/// A [`crate::MasamuneWebView`] that does not set `user_agent`, `allowed_origins` or
/// `grant_media_permissions` uses the values of the installed adapter.
///
/// ```ignore
/// MasamuneAdapters::new()
///     .adapter(
///         WebViewMasamuneAdapter::new()
///             .user_agent("Mozilla/5.0 (X11; Linux x86_64) …")
///             .allowed_origins(["https://example.com"])
///             .grant_media_permissions(true)
///             .max_download_bytes(16 * 1024 * 1024),
///     )
///     .install();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebViewMasamuneAdapter {
    user_agent: Option<String>,
    allowed_origins: Vec<String>,
    grant_media_permissions: bool,
    max_download_bytes: u64,
}

impl Default for WebViewMasamuneAdapter {
    fn default() -> Self {
        Self {
            user_agent: None,
            allowed_origins: Vec::new(),
            grant_media_permissions: false,
            max_download_bytes: DEFAULT_MAX_DOWNLOAD_BYTES,
        }
    }
}

impl WebViewMasamuneAdapter {
    /// Platform user agent, every origin allowed, media permissions not granted and
    /// [`DEFAULT_MAX_DOWNLOAD_BYTES`].
    pub fn new() -> Self {
        Self::default()
    }

    /// User-Agent string sent by every web view (empty restores the platform default).
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        let user_agent = user_agent.into();
        self.user_agent = (!user_agent.is_empty()).then_some(user_agent);
        self
    }

    /// Origins (for example `https://example.com`) the main frame may navigate to. Empty allows
    /// every origin for navigation, but disables the native bridge and media access. Entries are normalized (lowercase, default ports and paths removed).
    pub fn allowed_origins(mut self, origins: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let origins: Vec<String> = origins.into_iter().map(Into::into).collect();
        self.allowed_origins = normalize_origins(&origins);
        self
    }

    /// Grants camera / microphone requests from allowed origins without asking the page again.
    pub fn grant_media_permissions(mut self, grant: bool) -> Self {
        self.grant_media_permissions = grant;
        self
    }

    /// Largest payload the app should accept for a download sent through the bridge.
    pub fn max_download_bytes(mut self, bytes: u64) -> Self {
        self.max_download_bytes = bytes;
        self
    }

    /// The configured User-Agent (`None` keeps the platform default).
    pub fn get_user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    /// The normalized allowed origins (empty allows every origin).
    pub fn get_allowed_origins(&self) -> &[String] {
        &self.allowed_origins
    }

    /// Whether camera / microphone requests are granted.
    pub fn get_grant_media_permissions(&self) -> bool {
        self.grant_media_permissions
    }

    /// Largest accepted download payload in bytes.
    pub fn get_max_download_bytes(&self) -> u64 {
        self.max_download_bytes
    }

    fn defaults(&self) -> WebViewDefaults {
        WebViewDefaults {
            user_agent: self.user_agent.clone().unwrap_or_default(),
            allowed_origins: self.allowed_origins.clone(),
            grant_media_permissions: self.grant_media_permissions,
            max_download_bytes: self.max_download_bytes,
        }
    }
}

impl MasamuneAdapter for WebViewMasamuneAdapter {
    /// Registers this adapter's values as the runtime's web view defaults. Repeating it stores
    /// the same values again.
    fn prepare(&self) -> Result<(), AdapterError> {
        if self.grant_media_permissions && self.allowed_origins.is_empty() {
            return Err("media permissions require explicit allowed origins".into());
        }
        DEFAULTS.with(|defaults| *defaults.borrow_mut() = Some(self.defaults()));
        Ok(())
    }
}

/// Defaults applied to [`crate::MasamuneWebView`] props that are not set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebViewDefaults {
    /// User-Agent string (empty keeps the platform default).
    pub user_agent: String,
    /// Normalized allowed origins (empty allows every origin).
    pub allowed_origins: Vec<String>,
    /// Whether camera / microphone requests are granted.
    pub grant_media_permissions: bool,
    /// Largest accepted download payload in bytes.
    pub max_download_bytes: u64,
}

impl Default for WebViewDefaults {
    fn default() -> Self {
        WebViewMasamuneAdapter::default().defaults()
    }
}

whisker::runtime_local! {
    static DEFAULTS: RefCell<Option<WebViewDefaults>> = RefCell::new(None);
}

/// The defaults registered by the installed [`WebViewMasamuneAdapter`], or the built-in defaults
/// when none is installed.
pub fn webview_defaults() -> WebViewDefaults {
    if WebViewMasamuneAdapter::primary().is_none() {
        return WebViewDefaults::default();
    }
    DEFAULTS
        .with(|defaults| defaults.borrow().clone())
        .unwrap_or_default()
}

/// [`WebViewDefaults::max_download_bytes`] of the installed adapter.
pub fn max_download_bytes() -> u64 {
    webview_defaults().max_download_bytes
}

#[cfg(test)]
mod security_tests {
    use super::*;
    #[test]
    fn rejects_implicit_media_trust() {
        assert!(
            WebViewMasamuneAdapter::new()
                .grant_media_permissions(true)
                .prepare()
                .is_err()
        );
    }
}
