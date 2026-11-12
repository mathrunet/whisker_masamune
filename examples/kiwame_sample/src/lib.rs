//! Minimal Whisker app for Kiwame's Maestro E2E flow.
//!
//! `KIWAME_SAMPLE_FLAVOR=test` builds use in-memory runtime adapters, so Maestro runs need no
//! backend. Every control carries an accessibility identifier, a label and a role.

pub mod backend;
pub mod config;
pub mod debugger;
pub mod pages;

use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_ai_debugger::AiDebugger;
use masamune_whisker_appearance::{AppRoot, ThemeConfig, init_theme, palette};
use masamune_whisker_functions::Functions;
use masamune_whisker_logger::{ConsoleLoggerAdapter, LoggerConfig};
use masamune_whisker_ui::{Overlay, OverlayHost};
use whisker::css::{Display, FlexDirection};
use whisker::prelude::*;
use whisker_router::{Outlet, RouteSet, Router, routes};

use crate::pages::{BootPage, DetailPage, HomePage};

/// Path of the boot screen.
pub const BOOT: &str = "/";
/// Path of the first screen after boot.
pub const HOME: &str = "/home";
/// Path of the detail screen.
pub const DETAIL: &str = "/detail";

/// App-wide state, provided by [`Root`].
#[derive(Clone, Copy)]
pub struct AppState {
    pub count: RwSignal<u32>,
    pub overlay: Overlay,
    pub functions: StoredValue<Functions>,
}

/// The [`AppState`] provided by [`Root`]. Call it in a component body, not in a handler.
pub fn use_app() -> AppState {
    use_context::<AppState>().expect("AppState is provided by Root")
}

fn routes() -> RouteSet {
    routes! {
        Stack {
            Route(path: "", component: BootPage)
            Route(path: "home", component: HomePage)
            Route(path: "detail", component: DetailPage)
        }
    }
}

/// Startup work that must run once per launch (logger, theme, adapters).
fn prepare() {
    let debugger = debugger::controller();
    // An unknown flavor is a build mistake: fail loudly instead of falling back to dev.
    let flavor = config::flavor().expect("KIWAME_SAMPLE_FLAVOR must be dev, test or prod");
    LoggerConfig::new(flavor)
        .adapter(ConsoleLoggerAdapter::new())
        .adapter(debugger.logger_adapter())
        .init()
        .expect("logger");
    init_theme(ThemeConfig::default());
    MasamuneAdapters::new().install();
    debugger.install_panic_hook();
    log::info!("kiwame_sample: flavor={flavor:?}");
}

#[whisker::main]
fn app() -> Element {
    prepare();
    // Outside the router and `AppRoot`, so the debugger survives rebuilds.
    render! {
        AiDebugger(controller: debugger::controller()) {
            Root {}
        }
    }
}

#[component]
fn root() -> Element {
    let app = AppState {
        count: signal(0),
        overlay: Overlay::new(),
        functions: StoredValue::new(backend::functions()),
    };
    provide_context(app);
    render! {
        View(style: css!(
            flex_grow: 1.0,
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            background_color: palette().background,
        )) {
            AppRoot {
                Router(routes: routes()) {
                    Outlet {}
                }
                OverlayHost(overlay: app.overlay)
            }
        }
    }
}
