//! Top app bar (Flutter's `AppBar`): leading back button, start-aligned title and actions.
//!
//! Inside a full-screen page ([`Fullscreen`](super::fullscreen::Fullscreen)) the leading button
//! is a close button (X) by default, like Flutter's `fullscreenDialog`.

use masamune_whisker_appearance::{TextKind, bold, palette, shape, text_with};
use whisker::css::{AlignItems, Color, JustifyContent};
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};
use whisker_router::use_navigator;
use whisker_safe_area::safe_area_insets;

use super::fullscreen::use_fullscreen;
use super::ink_well::InkWell;
use super::scaffold::BarInsets;
use super::semantics;
use crate::style::{column, css_color, row};

/// Height of the bar below the status bar (`kToolbarHeight`).
pub const APP_BAR_HEIGHT: f32 = 56.0;

/// The title is exposed as a header labelled with `title`. Pass `identifier` to find it from UI
/// automation; the back button is then identified as `<identifier>_back` (labelled "Back").
/// With `close`, the leading button uses an X icon and `<identifier>_close` instead.
///
/// Inside a full-screen page ([`Fullscreen`](super::fullscreen::Fullscreen)) the close button is
/// shown by default (even with `back`); pass `hide_leading` to show no leading button.
#[component]
pub fn app_bar(
    title: Signal<String>,
    /// Accessibility identifier of the title (iOS `accessibilityIdentifier`).
    identifier: Option<String>,
    subtitle: Option<Signal<String>>,
    /// Shows a back button that pops the router.
    #[prop(default = false)]
    back: bool,
    /// Shows a close button that pops the router, taking precedence over `back`.
    #[prop(default = false)]
    close: bool,
    /// Localized accessibility label for the close button (defaults to "Close").
    close_label: Option<String>,
    /// Hides the leading button, including the default close button of a full-screen page.
    #[prop(default = false)]
    hide_leading: bool,
    #[prop(default = palette().background)] color: Color,
    #[prop(default = palette().text)] foreground: Color,
    /// Actions on the end side.
    children: Children,
) -> Element {
    let nav = use_navigator();
    // A full-screen page closes instead of going back, so its leading button is always an X.
    let fullscreen = use_fullscreen();
    let close = !hide_leading && (close || fullscreen);
    let back = !hide_leading && back;
    let projected = children.clone();
    let insets = safe_area_insets();
    // Inside a scaffold only the side insets it has not used yet; alone, the whole safe area.
    let side = use_context::<BarInsets>().unwrap_or_else(|| BarInsets {
        left: computed(move || insets.get().leading as f32),
        right: computed(move || insets.get().trailing as f32),
    });
    let icon_color = computed(move || css_color(foreground));
    let style = computed(move || {
        row()
            .flex_shrink(0.0)
            .height(px(APP_BAR_HEIGHT + insets.get().top as f32))
            .padding_top(px(insets.get().top as f32))
            // Landscape iPhones have side insets (notch / Dynamic Island).
            .padding_left(px(
                (if back || close { 4.0 } else { 16.0 }) + side.left.get()
            ))
            .padding_right(px(4.0 + side.right.get()))
            .background_color(color)
    });
    let back_accessibility = semantics::button(
        identifier
            .as_ref()
            .map(|id| format!("{id}_{}", if close { "close" } else { "back" }))
            .as_deref(),
        if close {
            close_label.as_deref().unwrap_or("Close")
        } else {
            "Back"
        },
        false,
    );
    let identifier = identifier.clone();
    let title_accessibility = computed(move || {
        semantics::semantics(
            AccessibilityRole::Header,
            identifier.as_deref(),
            title.get(),
            AccessibilityState::new(),
        )
    });
    render! {
        View(style: style) {
            Show(when: move || back || close) {
                InkWell(
                    style: css!(padding: px(12), border_radius: px(shape().full)),
                    accessibility: back_accessibility.clone(),
                    on_tap: {
                        let nav = nav.clone();
                        Callback::new(move |()| {
                            let _ = nav.back();
                        })
                    },
                ) {
                    Icon(
                        svg: if close { lucide::X } else { lucide::ArrowLeft },
                        color: icon_color,
                        size: "24",
                    )
                }
            }
            View(
                style: column()
                    .flex_grow(1.0)
                    .flex_shrink(1.0)
                    .justify_content(JustifyContent::Center),
                accessibility: title_accessibility,
            ) {
                Text(value: title, max_lines: 1u32, style: bold(TextKind::TitleLarge, foreground))
                Show(when: move || subtitle.is_some()) {
                    Text(
                        value: computed(move || subtitle.map(|s| s.get()).unwrap_or_default()),
                        style: text_with(TextKind::BodySmall, palette().sub_text),
                    )
                }
            }
            View(style: row().align_items(AlignItems::Center)) {
                {projected()}
            }
        }
    }
}

/// Icon button for the end side of an [`AppBar`].
///
/// It has no visible text, so pass `label` for screen readers and UI automation (Maestro `text:`)
/// and `identifier` for Maestro `id:`. It always has the button role and its disabled state.
#[component]
pub fn app_bar_action(
    svg: &'static str,
    /// Accessibility identifier for UI automation (iOS `accessibilityIdentifier`).
    identifier: Option<String>,
    /// Accessibility label (the action has no visible text).
    label: Option<String>,
    #[prop(default = None)] disabled: Option<ReadSignal<bool>>,
    on_tap: Callback<()>,
) -> Element {
    let (enabled_color, disabled_color) =
        (css_color(palette().text), css_color(palette().disabled));
    let color = computed(move || {
        if disabled.is_some_and(|d| d.get()) {
            disabled_color.clone()
        } else {
            enabled_color.clone()
        }
    });
    let (identifier, label) = (identifier.clone(), label.clone());
    let accessibility = computed(move || {
        let state = AccessibilityState::new().disabled(disabled.is_some_and(|d| d.get()));
        let accessibility = Accessibility::new()
            .role(AccessibilityRole::Button)
            .state(state);
        let accessibility = match &label {
            Some(label) => accessibility.label(label.clone()),
            None => accessibility,
        };
        match &identifier {
            Some(identifier) => accessibility.identifier(identifier.clone()),
            None => accessibility,
        }
    });
    render! {
        InkWell(
            style: css!(padding: px(12), border_radius: px(shape().full)),
            accessibility: accessibility,
            disabled_option: disabled,
            on_tap: on_tap,
        ) {
            Icon(svg: svg, color: color, size: "24")
        }
    }
}
