//! Construction options (the `AIDebuggerMasamuneAdapter` constructor and its dart-defines).

use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use futures_util::FutureExt;
use futures_util::future::LocalBoxFuture;
use masamune_whisker_logger::Flavor;

use crate::settings::{AiDebugSettings, AiDebugSettingsStore, LocalSettingsStore};
use crate::transport::{AiDebugTransport, HttpTransport};
use crate::types::{
    AiDebugAccount, AiDebugAuth, AiDebugAuthSnapshot, AiDebugCallbackResult, AiDebugCamera,
    AiDebugContextProvider, AiDebugContextSnapshot, AiDebugPurchase,
};

/// Runs a UI-thread task. The overlay installs one bound to its owner; tests pass their own.
pub type AiDebugSpawner = Rc<dyn Fn(LocalBoxFuture<'static, ()>)>;

/// Builds an [`AiDebuggerConfig`] from the app's compile-time environment:
/// `MASAMUNE_AI_DEBUGGER_PROJECT_ID`, `_ENDPOINT`, `_API_KEY` and `_ACCOUNTS`.
///
/// It must expand in the app crate, because a dependency cannot read the app's `option_env!`.
///
/// The values are read only in debug builds of an app crate compiled with the
/// `masamune_ai_debugger` cfg. Emit it from the app's `build.rs` for every flavor except prod
/// (dev and test), so production binaries never contain the API key or the accounts, even when
/// the variables are set in the build environment. Without the cfg, the debugger is disabled.
///
/// ```ignore
/// // build.rs
/// println!("cargo::rustc-check-cfg=cfg(masamune_ai_debugger)");
/// println!("cargo::rerun-if-env-changed=MY_APP_FLAVOR");
/// // Unset means prod, so only an explicit dev or test flavor opts in.
/// if matches!(std::env::var("MY_APP_FLAVOR").as_deref(), Ok("dev" | "test")) {
///     println!("cargo::rustc-cfg=masamune_ai_debugger");
/// }
///
/// // src
/// let flavor = Flavor::from_env(option_env!("MY_APP_FLAVOR"))?;
/// let config = masamune_whisker_ai_debugger::ai_debugger_config!(flavor);
/// ```
#[macro_export]
macro_rules! ai_debugger_config {
    ($flavor:expr) => {{
        #[cfg(all(debug_assertions, masamune_ai_debugger))]
        let config = $crate::AiDebuggerConfig::from_values(
            $flavor,
            ::core::option_env!("MASAMUNE_AI_DEBUGGER_PROJECT_ID"),
            ::core::option_env!("MASAMUNE_AI_DEBUGGER_ENDPOINT"),
            ::core::option_env!("MASAMUNE_AI_DEBUGGER_API_KEY"),
            ::core::option_env!("MASAMUNE_AI_DEBUGGER_ACCOUNTS"),
        );
        #[cfg(not(all(debug_assertions, masamune_ai_debugger)))]
        let config = {
            let _ = $flavor;
            $crate::AiDebuggerConfig::new($crate::__private::Flavor::Prod)
        };
        config
    }};
}

/// Everything the AI debugger needs. Only debug builds with [`Flavor::Dev`] or [`Flavor::Test`]
/// enable it.
/// Otherwise every operation does nothing and the overlay only renders its children.
#[derive(Clone)]
pub struct AiDebuggerConfig {
    pub(crate) flavor: Flavor,
    pub(crate) project_id: String,
    pub(crate) endpoint: String,
    pub(crate) api_key: String,
    pub(crate) accounts: Vec<AiDebugAccount>,
    pub(crate) max_sessions_per_hour: u32,
    pub(crate) max_screenshots: usize,
    pub(crate) report_handled_errors: bool,
    pub(crate) providers: Vec<String>,
    pub(crate) settings: AiDebugSettings,
    pub(crate) heartbeat_interval: Duration,
    pub(crate) transport: Rc<dyn AiDebugTransport>,
    pub(crate) settings_store: Rc<dyn AiDebugSettingsStore>,
    pub(crate) context_provider: Option<AiDebugContextProvider>,
    pub(crate) auth: Option<AiDebugAuth>,
    pub(crate) auth_snapshot: Option<Rc<dyn Fn() -> AiDebugCallbackResult<AiDebugAuthSnapshot>>>,
    pub(crate) purchase: Option<AiDebugPurchase>,
    pub(crate) camera: Option<AiDebugCamera>,
    pub(crate) voice_input: bool,
}

impl AiDebuggerConfig {
    /// Starts without project, endpoint or API key; set them or use [`ai_debugger_config!`].
    pub fn new(flavor: Flavor) -> Self {
        Self {
            flavor,
            project_id: String::new(),
            endpoint: String::new(),
            api_key: String::new(),
            accounts: Vec::new(),
            max_sessions_per_hour: 6,
            max_screenshots: 6,
            report_handled_errors: true,
            providers: vec!["codex".into(), "claude".into(), "gemini".into()],
            settings: AiDebugSettings::default(),
            heartbeat_interval: Duration::from_secs(30),
            transport: Rc::new(HttpTransport),
            settings_store: Rc::new(LocalSettingsStore),
            context_provider: None,
            auth: None,
            auth_snapshot: None,
            purchase: None,
            camera: None,
            voice_input: true,
        }
    }

    /// The values of [`ai_debugger_config!`]; `accounts` uses the `id:pass,…` format.
    pub fn from_values(
        flavor: Flavor,
        project_id: Option<&str>,
        endpoint: Option<&str>,
        api_key: Option<&str>,
        accounts: Option<&str>,
    ) -> Self {
        Self::new(flavor)
            .project_id(project_id.unwrap_or_default())
            .endpoint(endpoint.unwrap_or_default())
            .api_key(api_key.unwrap_or_default())
            .accounts(AiDebugAccount::parse(accounts.unwrap_or_default()))
    }

    /// Whether the debugger runs (debug builds with an explicit dev or test flavor only).
    pub fn enabled(&self) -> bool {
        cfg!(debug_assertions) && matches!(self.flavor, Flavor::Dev | Flavor::Test)
    }

    /// SamuraiAI project id (the app path with `/` replaced by `-`).
    pub fn project_id(mut self, project_id: impl Into<String>) -> Self {
        self.project_id = project_id.into();
        self
    }

    /// SamuraiAI base URL; `/api/app-debug/…` is appended.
    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Sent as `x-api-key`. It is compiled into the app, so never distribute dev builds.
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = api_key.into();
        self
    }

    /// Accounts listed by the login panel instead of the email/password form.
    pub fn accounts(mut self, accounts: Vec<AiDebugAccount>) -> Self {
        self.accounts = accounts;
        self
    }

    /// Sent when the run registers (default 6).
    pub fn max_sessions_per_hour(mut self, value: u32) -> Self {
        self.max_sessions_per_hour = value;
        self
    }

    /// Screenshots kept in the panel (default 6).
    pub fn max_screenshots(mut self, value: usize) -> Self {
        self.max_screenshots = value.max(1);
        self
    }

    /// Whether `report_error` (handled errors) creates incidents (default true). When false they
    /// are only sent as `error` breadcrumbs.
    pub fn report_handled_errors(mut self, value: bool) -> Self {
        self.report_handled_errors = value;
        self
    }

    /// Provider choices of the settings dialog (default `codex`, `claude`, `gemini`).
    pub fn providers(mut self, providers: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.providers = providers.into_iter().map(Into::into).collect();
        self
    }

    /// Initial settings, used until the user saves others.
    pub fn settings(mut self, settings: AiDebugSettings) -> Self {
        self.settings = settings;
        self
    }

    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval = interval;
        self
    }

    /// Replaces the HTTP client (another backend, tests).
    pub fn transport(mut self, transport: impl AiDebugTransport + 'static) -> Self {
        self.transport = Rc::new(transport);
        self
    }

    /// Replaces `whisker-local-store` for saved settings.
    pub fn settings_store(mut self, store: impl AiDebugSettingsStore + 'static) -> Self {
        self.settings_store = Rc::new(store);
        self
    }

    /// Supplies the page, route and selected state attached to requests and incidents. Values are
    /// redacted and truncated before sending.
    pub fn context_provider<F, Fut>(mut self, provider: F) -> Self
    where
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = Option<AiDebugContextSnapshot>> + 'static,
    {
        self.context_provider = Some(Rc::new(move || provider().boxed_local()));
        self
    }

    /// Adds the debug login panel.
    pub fn auth(mut self, auth: AiDebugAuth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Exposes the real auth state to Maestro (`ai_debugger_auth_*`). Read inside a reactive
    /// computation, so reading the app's signals there keeps the nodes current.
    pub fn auth_snapshot(
        mut self,
        snapshot: impl Fn() -> AiDebugCallbackResult<AiDebugAuthSnapshot> + 'static,
    ) -> Self {
        self.auth_snapshot = Some(Rc::new(snapshot));
        self
    }

    /// Adds the debug purchase panel.
    pub fn purchase(mut self, purchase: AiDebugPurchase) -> Self {
        self.purchase = Some(purchase);
        self
    }

    /// Adds the debug camera panel.
    pub fn camera(mut self, camera: AiDebugCamera) -> Self {
        self.camera = Some(camera);
        self
    }

    /// Whether the panel shows the voice input button (default true). The button exists only
    /// when the crate is built with the `voice` feature, which the app enables for development
    /// builds only; see the crate README.
    pub fn voice_input(mut self, enabled: bool) -> Self {
        self.voice_input = enabled;
        self
    }
}

/// Invalid configuration found when creating the controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiDebugConfigError(pub String);

impl std::fmt::Display for AiDebugConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AiDebugConfigError {}
