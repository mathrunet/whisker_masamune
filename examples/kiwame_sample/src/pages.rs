//! Screens. Identifiers used by the Maestro flows:
//!
//! | Screen | Identifier | Element |
//! |---|---|---|
//! | `/home` | `home_app_bar` | app bar title "Home" |
//! | `/home` | `home_count` | counter text "Count: N" |
//! | `/home` | `home_increment` | "Increment" button |
//! | `/home` | `home_go_detail` | "Detail" button |
//! | `/detail` | `detail_app_bar` | app bar title "Detail" |
//! | `/detail` | `detail_message` | detail text |
//! | `/detail` | `detail_back` | "Back" button |

use masamune_whisker_appearance::{TextKind, text};
use masamune_whisker_boot::{BootConfig, BootError, BootScreen, boot_task};
use masamune_whisker_ui::{AppBar, FilledButton, OutlinedButton, Scaffold, TextButton};
use whisker::css::{AlignItems, Display, FlexDirection, JustifyContent};
use whisker::prelude::*;
use whisker_router::use_navigator;

use crate::backend::LoadCount;
use crate::{DETAIL, HOME, use_app};

fn body_style() -> Css {
    css!(
        flex_grow: 1.0,
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        gap: px(16),
        padding: px(24),
    )
}

/// Text exposed to UI automation with `identifier` and its own value as the label.
fn text_semantics(
    identifier: &'static str,
    value: ReadSignal<String>,
) -> ReadSignal<Accessibility> {
    computed(move || {
        Accessibility::new()
            .identifier(identifier)
            .label(value.get())
            .role(AccessibilityRole::Text)
    })
}

/// Boot screen: loads the counter through the flavor's backend, then opens `/home`.
#[component]
pub fn boot_page() -> Element {
    let app = use_app();
    let task = boot_task(move || async move {
        let functions = app.functions.get();
        match functions.execute(LoadCount).await {
            Ok(response) => app.count.set(response.count),
            // The counter is not essential: start from zero instead of blocking the app.
            Err(_) => log::warn!("load_count failed: code=backend_unavailable"),
        }
        Ok::<_, BootError>(HOME.to_owned())
    });
    let config = BootConfig::new(task, app.overlay);
    render! {
        BootScreen(config: config) {
            Scaffold {
                View(style: body_style()) {
                    Text(value: "Kiwame Sample", style: text(TextKind::HeadlineMedium))
                }
            }
        }
    }
}

/// Counter screen.
#[component]
pub fn home_page() -> Element {
    let app = use_app();
    let nav = use_navigator();
    let count = app.count;
    let label = computed(move || format!("Count: {}", count.get()));
    let go_detail = Callback::new(move |_: ()| {
        let _ = nav.navigate(DETAIL);
    });
    render! {
        Scaffold(app_bar: move || render! { AppBar(title: "Home", identifier: "home_app_bar") {} }) {
            View(style: body_style()) {
                View(accessibility: text_semantics("home_count", label)) {
                    Text(value: label, style: text(TextKind::HeadlineMedium))
                }
                FilledButton(
                    label: "Increment",
                    identifier: "home_increment",
                    on_tap: move |_| count.update(|count| *count += 1),
                )
                OutlinedButton(
                    label: "Detail",
                    identifier: "home_go_detail",
                    on_tap: go_detail,
                )            }
        }
    }
}

/// Detail screen.
#[component]
pub fn detail_page() -> Element {
    let nav = use_navigator();
    let message = computed(|| "This is the detail screen.".to_owned());
    let back = Callback::new(move |_: ()| {
        let _ = nav.back();
    });
    render! {
        Scaffold(app_bar: move || render! { AppBar(title: "Detail", identifier: "detail_app_bar") {} }) {
            View(style: body_style()) {
                View(accessibility: text_semantics("detail_message", message)) {
                    Text(value: message, style: text(TextKind::BodyLarge))
                }
                TextButton(
                    label: "Back",
                    identifier: "detail_back",
                    on_tap: back,
                )
            }
        }
    }
}
