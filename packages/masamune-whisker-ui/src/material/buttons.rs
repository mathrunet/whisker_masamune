//! Buttons (Flutter's `FilledButton`, `OutlinedButton` and `TextButton`).

use masamune_whisker_appearance::{TextKind, bold, palette};
use whisker::css::{AlignItems, Color, Display, FlexDirection, JustifyContent, TextAlign};
use whisker::prelude::*;
use whisker_svg::Svg;

use crate::material::progress::CircularProgressIndicator;
use crate::style::css_color;

fn blocked(disabled: Option<ReadSignal<bool>>, loading: Option<ReadSignal<bool>>) -> bool {
    disabled.is_some_and(|d| d.get_untracked()) || loading.is_some_and(|l| l.get_untracked())
}

/// Filled, pill-shaped button.
#[component]
pub fn filled_button(
    label: Signal<String>,
    #[prop(default = palette().primary)] color: Color,
    #[prop(default = palette().on_primary)] foreground: Color,
    disabled: Option<ReadSignal<bool>>,
    loading: Option<ReadSignal<bool>>,
    /// Stretches to the full width.
    #[prop(default = false)]
    expand: bool,
    on_tap: Callback<()>,
) -> Element {
    let style = computed(move || {
        let off = disabled.is_some_and(|d| d.get());
        let style = css!(
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            gap: px(8),
            min_height: px(40),
            padding: (px(8), px(24)),
            border_radius: px(24),
            background_color: if off { palette().disabled } else { color },
        );
        if expand {
            style.width(percent(100))
        } else {
            style
        }
    });
    render! {
        View(style: style, on_tap: move |_| {
            if !blocked(disabled, loading) {
                on_tap.run(());
            }
        }) {
            Show(when: move || loading.is_some_and(|l| l.get())) {
                CircularProgressIndicator(size: 16.0, color: foreground, stroke: 3.0)
            }
            Text(value: label, style: bold(TextKind::LabelLarge, foreground).text_align(TextAlign::Center))
        }
    }
}

/// Outlined, pill-shaped button with an optional leading SVG icon. `filled` paints the
/// primary color behind the outline.
#[component]
pub fn outlined_button(
    label: Signal<String>,
    icon: Option<&'static str>,
    #[prop(default = false)] filled: bool,
    #[prop(default = palette().text)] color: Color,
    loading: Option<ReadSignal<bool>>,
    disabled: Option<ReadSignal<bool>>,
    on_tap: Callback<()>,
) -> Element {
    let style = computed(move || {
        let off = disabled.is_some_and(|d| d.get());
        css!(
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            gap: px(8),
            height: px(40),
            padding: (px(0), px(24)),
            border_radius: px(32),
            border: whisker::css::Border::new().width(px(2)).solid().color(color),
            background_color: if filled { palette().primary } else { Color::rgba(0, 0, 0, 0.0) },
            opacity: if off { 0.5 } else { 1.0 },
        )
    });
    let icon_color = css_color(color);
    let is_loading = move || loading.is_some_and(|l| l.get());
    render! {
        View(style: style, on_tap: move |_| {
            if !blocked(disabled, loading) {
                on_tap.run(());
            }
        }) {
            Show(when: is_loading, fallback: move || match icon {
                Some(svg) => render! { Svg(content: svg, color: icon_color.clone(), style: css!(width: px(18), height: px(18))) },
                None => render! { Fragment {} },
            }) {
                CircularProgressIndicator(size: 18.0, color: color, stroke: 3.0)
            }
            Text(value: label, style: bold(TextKind::LabelLarge, color))
        }
    }
}

/// Text-only button.
#[component]
pub fn text_button(
    label: Signal<String>,
    #[prop(default = palette().primary)] color: Color,
    on_tap: Callback<()>,
) -> Element {
    render! {
        View(style: css!(padding: (px(8), px(12))), on_tap: move |_| on_tap.run(())) {
            Text(value: label, style: bold(TextKind::LabelLarge, color))
        }
    }
}
