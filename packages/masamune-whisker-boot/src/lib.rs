//! Boot screen: shows a splash while the app's startup work runs, then resets the router to the
//! first screen.
//!
//! - The splash stays for at least [`BootConfig::min_display`] so it never just flashes.
//! - The startup work gives up after [`BootConfig::timeout`] and is treated as
//!   [`BootError::Offline`], so a stalled request always ends in the retry dialog.
//! - While a retry runs, a progress indicator is shown over the splash.
//! - Failures show "retry / quit"; [`BootError::Offline`] uses the offline message.
//! - [`BootError::UpdateRequired`] shows "update / quit" and opens the store.
//! - iOS has no "quit" button (apps must not terminate themselves there): failures offer only
//!   "retry", and the update dialog stays up after opening the store.
//! - [`restart`] goes back to the boot screen as if the app had been launched again.

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use masamune_whisker_ui::style::column;
use masamune_whisker_ui::{CircularProgressIndicator, Overlay};
use whisker::Owner;
use whisker::css::{AlignItems, Color, PositionKind};
use whisker::prelude::*;
use whisker_router::{RouterHandle, use_navigator};

/// Why startup failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootError {
    /// The device is offline.
    Offline,
    /// Required data could not be loaded, or anything else.
    Failed(String),
    /// The installed version is too old.
    UpdateRequired,
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::Offline => f.write_str("offline"),
            BootError::Failed(message) => f.write_str(message),
            BootError::UpdateRequired => f.write_str("update required"),
        }
    }
}

impl std::error::Error for BootError {}

impl BootError {
    /// Fixed code of the error kind (safe to log: no messages, URLs or user data).
    pub fn code(&self) -> &'static str {
        match self {
            BootError::Offline => "offline",
            BootError::Failed(_) => "failed",
            BootError::UpdateRequired => "update_required",
        }
    }
}

#[cfg(feature = "functions")]
impl From<masamune_whisker_functions::ApiError> for BootError {
    fn from(error: masamune_whisker_functions::ApiError) -> Self {
        if error.is_offline() {
            BootError::Offline
        } else {
            BootError::Failed(error.to_string())
        }
    }
}

/// Fails with [`BootError::UpdateRequired`] when the required version is newer than the app.
pub fn check_version(required: i64, current: i64) -> Result<(), BootError> {
    if required > current {
        Err(BootError::UpdateRequired)
    } else {
        Ok(())
    }
}

/// Startup work. Resolves to the path of the first screen.
pub type BootTask = Rc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<String, BootError>>>>>;

/// Wraps an async closure as a [`BootTask`].
pub fn boot_task<F, Fut>(f: F) -> BootTask
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = Result<String, BootError>> + 'static,
{
    Rc::new(move || Box::pin(f()))
}

type Text = Rc<dyn Fn() -> String>;

/// Callback receiving a boot failure.
pub type ErrorHandler = Rc<dyn Fn(&BootError)>;

/// Localized texts of the boot dialogs (closures so they follow the current language).
#[derive(Clone)]
pub struct BootTexts {
    pub error_title: Text,
    pub offline: Text,
    pub failed: Text,
    pub retry: Text,
    pub quit: Text,
    pub update_title: Text,
    pub update_message: Text,
    pub update: Text,
}

impl Default for BootTexts {
    fn default() -> Self {
        fn text(value: &'static str) -> Text {
            Rc::new(move || value.to_owned())
        }
        Self {
            error_title: text("Error"),
            offline: text("Network unavailable. Please check your connection."),
            failed: text("Something went wrong. Please try again."),
            retry: text("Retry"),
            quit: text("Quit"),
            update_title: text("Update available"),
            update_message: text("Please update the app in the store and launch it again."),
            update: text("Update"),
        }
    }
}

/// What the boot screen runs and how it reports problems.
#[derive(Clone)]
pub struct BootConfig {
    pub task: BootTask,
    /// Minimum time the splash stays visible.
    pub min_display: Duration,
    /// Time after which the startup work is abandoned and treated as [`BootError::Offline`].
    /// `None` waits forever.
    pub timeout: Option<Duration>,
    pub texts: BootTexts,
    /// Store page opened by "update".
    pub store_url: Option<String>,
    /// Where the dialogs are shown.
    pub overlay: Overlay,
    /// Called with every failure (for crash reporting).
    pub on_error: Option<ErrorHandler>,
}

/// Default of [`BootConfig::timeout`].
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

impl BootConfig {
    pub fn new(task: BootTask, overlay: Overlay) -> Self {
        Self {
            task,
            min_display: Duration::from_secs(1),
            timeout: Some(DEFAULT_TIMEOUT),
            texts: BootTexts::default(),
            store_url: None,
            overlay,
            on_error: None,
        }
    }
}

whisker::runtime_local! {
    static BOOTED: RwSignal<bool> = signal(false);
}

/// Whether startup finished and the first screen is showing.
pub fn booted() -> ReadSignal<bool> {
    BOOTED.with(|booted| booted.read_only())
}

/// Starts over from the boot screen at `boot_path` (as if the app had been relaunched).
pub fn restart(nav: &RouterHandle, boot_path: &str) {
    BOOTED.with(|booted| booted.set(false));
    let _ = nav.reset(boot_path);
}

fn quit() {
    let _ = masamune_whisker_app_control::exit_app();
}

/// Whether the platform lets the app close itself (not on iOS).
const CAN_QUIT: bool = !cfg!(target_os = "ios");

/// Shows the update-required dialog. Without a quit button it comes back after the store opens.
fn show_update(config: BootConfig) {
    let texts = config.texts.clone();
    let store_url = config.store_url.clone();
    let open_store = move || {
        if let Some(url) = &store_url {
            let _ = masamune_whisker_app_control::open_url(url);
        }
    };
    if CAN_QUIT {
        config.overlay.confirm(
            (texts.update_title)(),
            (texts.update_message)(),
            (texts.update)(),
            (texts.quit)(),
            move || {
                open_store();
                quit();
            },
            Some(Rc::new(quit)),
        );
    } else {
        let again = config.clone();
        config.overlay.alert_then(
            (texts.update_title)(),
            (texts.update_message)(),
            (texts.update)(),
            move || {
                open_store();
                show_update(again.clone());
            },
        );
    }
}

/// State of one mounted boot screen, shared by its runs.
#[derive(Clone)]
struct BootRun {
    /// Owner of the boot screen. The startup work is spawned under it: a task spawned from the
    /// retry button would belong to the dialog and be cancelled when the dialog closes.
    owner: Option<Owner>,
    /// Cleared when the screen unmounts; results of its runs are dropped after that.
    alive: Rc<Cell<bool>>,
    /// Whether the startup work is running (prevents running it twice at once).
    running: Rc<Cell<bool>>,
    /// Whether a retry is running (shows the progress indicator).
    retrying: RwSignal<bool>,
}

/// Runs `task`, giving up after `timeout` with [`BootError::Offline`].
async fn run_task(task: &BootTask, timeout: Option<Duration>) -> Result<String, BootError> {
    let Some(timeout) = timeout else {
        return task().await;
    };
    match tokio::time::timeout(timeout, task()).await {
        Ok(result) => result,
        Err(_) => {
            log::warn!("boot: timed out after {}s", timeout.as_secs());
            Err(BootError::Offline)
        }
    }
}

fn run(config: BootConfig, nav: RouterHandle, state: BootRun, retry: bool) {
    let Some(owner) = state.owner.filter(|owner| owner.is_alive()) else {
        log::warn!("boot: screen closed, not started");
        return;
    };
    if !state.alive.get() || state.running.replace(true) {
        return;
    }
    if retry {
        log::warn!("boot: retry");
        state.retrying.set(true);
    }
    owner.with(|| {
        spawn_local(async move {
            let (result, _) = tokio::join!(
                run_task(&config.task, config.timeout),
                tokio::time::sleep(config.min_display)
            );
            if !state.alive.get() {
                // The screen was replaced (for example by a new activity); its own run decides.
                log::warn!("boot: screen closed, result ignored");
                return;
            }
            state.running.set(false);
            state.retrying.set(false);
            let texts = config.texts.clone();
            match result {
                Ok(route) => {
                    BOOTED.with(|booted| booted.set(true));
                    match nav.reset(&route) {
                        Ok(()) => log::info!("boot done: route={route}"),
                        Err(error) => {
                            log::error!("boot: reset failed: route={route} error={error:?}")
                        }
                    }
                }
                Err(error) => {
                    log::warn!("boot failed: code={}", error.code());
                    if let Some(on_error) = &config.on_error {
                        on_error(&error);
                    }
                    if error == BootError::UpdateRequired {
                        show_update(config);
                        return;
                    }
                    let message = if error == BootError::Offline {
                        (texts.offline)()
                    } else {
                        (texts.failed)()
                    };
                    let retry_config = config.clone();
                    let retry = move || run(retry_config.clone(), nav.clone(), state.clone(), true);
                    if CAN_QUIT {
                        config.overlay.confirm(
                            (texts.error_title)(),
                            message,
                            (texts.retry)(),
                            (texts.quit)(),
                            retry,
                            Some(Rc::new(quit)),
                        );
                    } else {
                        config.overlay.alert_then(
                            (texts.error_title)(),
                            message,
                            (texts.retry)(),
                            retry,
                        );
                    }
                }
            }
        })
    });
}

/// Boot screen. `children` is the splash; the startup work starts when it mounts.
#[component]
pub fn boot_screen(config: BootConfig, children: Children) -> Element {
    let nav = use_navigator();
    let state = BootRun {
        owner: Owner::current(),
        alive: Rc::new(Cell::new(true)),
        running: Rc::new(Cell::new(false)),
        retrying: signal(false),
    };
    {
        let alive = state.alive.clone();
        on_cleanup(move || alive.set(false));
    }
    {
        let config = config.clone();
        let state = state.clone();
        on_mount(move || run(config.clone(), nav.clone(), state.clone(), false));
    }
    let retrying = state.retrying;
    let splash = children.clone();
    render! {
        View(style: css!(flex_grow: 1.0, position: PositionKind::Relative)) {
            {splash()}
            Show(when: move || retrying.get()) {
                View(style: column().merge(css!(
                    position: PositionKind::Absolute,
                    left: px(0),
                    right: px(0),
                    bottom: px(96),
                    align_items: AlignItems::Center,
                ))) {
                    CircularProgressIndicator(size: 32.0, color: Color::rgb(255, 255, 255), trace_name: "boot_retry".to_owned())
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_versions() {
        assert_eq!(check_version(2, 2), Ok(()));
        assert_eq!(check_version(1, 2), Ok(()));
        assert_eq!(check_version(3, 2), Err(BootError::UpdateRequired));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn stalled_task_times_out_as_offline() {
        let task = boot_task(std::future::pending::<Result<String, BootError>>);
        let result = run_task(&task, Some(Duration::from_secs(30))).await;
        assert_eq!(result, Err(BootError::Offline));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn finished_task_keeps_its_result() {
        let task = boot_task(|| async { Ok("/home".to_owned()) });
        let result = run_task(&task, Some(Duration::from_secs(30))).await;
        assert_eq!(result, Ok("/home".to_owned()));
    }

    #[test]
    fn error_codes_hide_messages() {
        assert_eq!(
            BootError::Failed("https://example.com".into()).code(),
            "failed"
        );
        assert_eq!(BootError::Offline.code(), "offline");
    }
}

#[cfg(test)]
mod retry_lifetime_regression {
    use super::*;
    use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};
    use whisker_router::{CompiledTree, RouteRegistry, RouteTree};

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn failed_retry_releases_indicator_after_dialog_closes() {
        check_retry(true).await;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn successful_retry_releases_indicator_after_dialog_closes() {
        check_retry(false).await;
    }

    /// Closing the dialog must not cancel its retry or leave the spinner running.
    async fn check_retry(fail: bool) {
        let runtime = RuntimeContext::new(RuntimeWakeHandle::new(|| {}));
        let (screen, state, calls) = runtime.enter(|| {
            let screen = Owner::new(None);
            let (state, calls) = screen.with(|| {
                let nav = RouterHandle::new((
                    CompiledTree::new(RouteTree::route("", "home")),
                    RouteRegistry::new().route("home", |_: &whisker_router::RouteInstance| {
                        whisker::runtime::view::create_phantom_element()
                    }),
                ));
                let calls = Rc::new(Cell::new(0));
                let count = calls.clone();
                let task = boot_task(move || {
                    let count = count.clone();
                    async move {
                        count.set(count.get() + 1);
                        tokio::time::sleep(Duration::from_millis(5)).await;
                        if fail {
                            Err(BootError::Offline)
                        } else {
                            Ok(String::new())
                        }
                    }
                });
                let overlay = Overlay {
                    dialog: signal(None),
                    busy: signal(0),
                };
                let mut config = BootConfig::new(task, overlay);
                config.min_display = Duration::ZERO;
                let state = BootRun {
                    owner: Owner::current(),
                    alive: Rc::new(Cell::new(true)),
                    running: Rc::new(Cell::new(false)),
                    retrying: signal(false),
                };
                let dialog = Owner::new(None);
                dialog.with(|| run(config, nav, state.clone(), true));
                dialog.dispose();
                whisker::run_until_stalled();
                assert_eq!(calls.get(), 1, "retry was cancelled with its dialog");
                assert!(state.running.get());
                assert!(state.retrying.get_untracked());
                (state, calls)
            });
            (screen, state, calls)
        });
        tokio::time::advance(Duration::from_millis(10)).await;
        runtime.enter(|| {
            screen.with(|| {
                whisker::run_until_stalled();
                assert_eq!(calls.get(), 1);
                assert_eq!(booted().get_untracked(), !fail);
                assert!(!state.running.get());
                assert!(!state.retrying.get_untracked(), "spinner was not cleared");
            });
            screen.dispose();
        });
    }
}
