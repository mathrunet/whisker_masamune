//! Top app bar (Flutter's `AppBar`): leading back button, start-aligned title and actions.

use masamune_whisker_appearance::{TextKind, bold, palette, text_with};
use whisker::css::{AlignItems, Color, JustifyContent};
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};
use whisker_router::use_navigator;
use whisker_safe_area::safe_area_insets;

use crate::style::{column, css_color, row};

/// Height of the bar below the status bar (`kToolbarHeight`).
pub const APP_BAR_HEIGHT: f32 = 56.0;

#[component]
pub fn app_bar(
    title: Signal<String>,
    subtitle: Option<Signal<String>>,
    /// Shows a back button that pops the router.
    #[prop(default = false)]
    back: bool,
    #[prop(default = palette().background)] color: Color,
    #[prop(default = palette().text)] foreground: Color,
    /// Actions on the end side.
    children: Children,
) -> Element {
    let nav = use_navigator();
    let projected = children.clone();
    let insets = safe_area_insets();
    let icon_color = computed(move || css_color(foreground));
    let style = computed(move || {
        row()
            .flex_shrink(0.0)
            .height(px(APP_BAR_HEIGHT + insets.get().top as f32))
            .padding_top(px(insets.get().top as f32))
            .padding_left(px(if back { 4.0 } else { 16.0 }))
            .padding_right(px(4))
            .background_color(color)
    });
    render! {
        View(style: style) {
            Show(when: move || back) {
                View(style: css!(padding: px(12)), on_tap: {
                    let nav = nav.clone();
                    move |_| { let _ = nav.back(); }
                }) {
                    Icon(svg: lucide::ArrowLeft, color: icon_color, size: "24")
                }
            }
            View(style: column().flex_grow(1.0).flex_shrink(1.0).justify_content(JustifyContent::Center)) {
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
#[component]
pub fn app_bar_action(
    svg: &'static str,
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
    render! {
        View(style: css!(padding: px(12)), on_tap: move |_| {
            if !disabled.is_some_and(|d| d.get_untracked()) {
                on_tap.run(());
            }
        }) {
            Icon(svg: svg, color: color, size: "24")
        }
    }
}
