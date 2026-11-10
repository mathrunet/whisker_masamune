//! Boot screen: shows a splash while the app's startup work runs, then resets the router to the
//! first screen.
//!
//! - The splash stays for at least [`BootConfig::min_display`] so it never just flashes.
//! - Failures show "retry / quit"; [`BootError::Offline`] uses the offline message.
//! - [`BootError::UpdateRequired`] shows "update / quit" and opens the store.
//! - iOS has no "quit" button (apps must not terminate themselves there): failures offer only
//!   "retry", and the update dialog stays up after opening the store.
//! - [`restart`] goes back to the boot screen as if the app had been launched again.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use masamune_whisker_ui::Overlay;
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

#[cfg(feature = "workers")]
impl From<masamune_whisker_workers::ApiError> for BootError {
    fn from(error: masamune_whisker_workers::ApiError) -> Self {
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
    pub texts: BootTexts,
    /// Store page opened by "update".
    pub store_url: Option<String>,
    /// Where the dialogs are shown.
    pub overlay: Overlay,
    /// Called with every failure (for crash reporting).
    pub on_error: Option<ErrorHandler>,
}

impl BootConfig {
    pub fn new(task: BootTask, overlay: Overlay) -> Self {
        Self {
            task,
            min_display: Duration::from_secs(1),
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

fn run(config: BootConfig, nav: RouterHandle) {
    spawn_local(async move {
        let (result, _) = tokio::join!((config.task)(), tokio::time::sleep(config.min_display));
        let texts = config.texts.clone();
        match result {
            Ok(route) => {
                BOOTED.with(|booted| booted.set(true));
                let result = nav.reset(&route);
                log::info!("boot done: route={route} result={result:?}");
            }
            Err(error) => {
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
                let retry = move || run(retry_config.clone(), nav.clone());
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
    });
}

/// Boot screen. `children` is the splash; the startup work starts when it mounts.
#[component]
pub fn boot_screen(config: BootConfig, children: Children) -> Element {
    let nav = use_navigator();
    {
        let config = config.clone();
        on_mount(move || run(config.clone(), nav.clone()));
    }
    let splash = children.clone();
    render! {
        Fragment {
            {splash()}
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
}
