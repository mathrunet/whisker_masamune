//! Buttons (Flutter's `FilledButton`, `OutlinedButton` and `TextButton`).

use masamune_whisker_appearance::{ColorExt, TextKind, bold, palette, shape};
use whisker::css::{AlignItems, Color, Display, FlexDirection, JustifyContent, TextAlign};
use whisker::prelude::*;
use whisker_svg::Svg;

use crate::material::ink_well::InkWell;
use crate::material::progress::CircularProgressIndicator;
use crate::material::semantics;
use crate::style::css_color;

/// True while the button is disabled or loading: no ink and no tap.
fn blocked(
    disabled: Option<ReadSignal<bool>>,
    loading: Option<ReadSignal<bool>>,
) -> ReadSignal<bool> {
    computed(move || disabled.is_some_and(|d| d.get()) || loading.is_some_and(|l| l.get()))
}

/// Filled, pill-shaped button.
///
/// The button is exposed to accessibility services and UI automation with `label`, the button
/// role and its disabled state; pass `identifier` to find it by id (Maestro `id:`).
#[component]
pub fn filled_button(
    label: Signal<String>,
    /// Accessibility identifier for UI automation (iOS `accessibilityIdentifier`).
    identifier: Option<String>,
    #[prop(default = palette().primary)] color: Color,
    #[prop(default = palette().on_primary)] foreground: Color,
    disabled: Option<ReadSignal<bool>>,
    loading: Option<ReadSignal<bool>>,
    /// Stretches to the full width.
    #[prop(default = false)]
    expand: bool,
    on_tap: Callback<()>,
) -> Element {
    let busy = blocked(disabled, loading);
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
            border_radius: px(shape().full),
            background_color: if off { palette().disabled } else { color },
        );
        if expand {
            style.width(percent(100))
        } else {
            style
        }
    });
    let identifier = identifier.clone();
    let accessibility = computed(move || {
        semantics::button(
            identifier.as_deref(),
            label.get(),
            disabled.is_some_and(|d| d.get()) || loading.is_some_and(|l| l.get()),
        )
    });
    render! {
        InkWell(
            style: style,
            accessibility: accessibility,
            splash_color: foreground.with_alpha(0.16),
            highlight_color: foreground.with_alpha(0.08),
            disabled: busy,
            on_tap: on_tap,
        ) {
            Show(when: move || loading.is_some_and(|l| l.get())) {
                CircularProgressIndicator(size: 16.0, color: foreground, stroke: 3.0)
            }
            Text(value: label, style: bold(TextKind::LabelLarge, foreground).text_align(TextAlign::Center))
        }
    }
}

/// Outlined, pill-shaped button with an optional leading SVG icon. `filled` paints the
/// primary color behind the outline.
///
/// Exposed to accessibility services like [`FilledButton`]; pass `identifier` for UI automation.
#[component]
pub fn outlined_button(
    label: Signal<String>,
    /// Accessibility identifier for UI automation (iOS `accessibilityIdentifier`).
    identifier: Option<String>,
    icon: Option<&'static str>,
    #[prop(default = false)] filled: bool,
    #[prop(default = palette().text)] color: Color,
    loading: Option<ReadSignal<bool>>,
    disabled: Option<ReadSignal<bool>>,
    on_tap: Callback<()>,
) -> Element {
    let busy = blocked(disabled, loading);
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
            border_radius: px(shape().full),
            border: whisker::css::Border::new().width(px(2)).solid().color(color),
            background_color: if filled { palette().primary } else { Color::rgba(0, 0, 0, 0.0) },
            opacity: if off { 0.5 } else { 1.0 },
        )
    });
    let icon_color = css_color(color);
    let is_loading = move || loading.is_some_and(|l| l.get());
    let identifier = identifier.clone();
    let accessibility = computed(move || {
        semantics::button(
            identifier.as_deref(),
            label.get(),
            disabled.is_some_and(|d| d.get()) || is_loading(),
        )
    });
    render! {
        InkWell(style: style, accessibility: accessibility, disabled: busy, on_tap: on_tap) {
            Show(when: is_loading, fallback: {
                let icon_color = icon_color.clone();
                move || match icon {
                Some(svg) => render! { Svg(content: svg, color: icon_color.clone(), style: css!(width: px(18), height: px(18))) },
                    None => render! { Fragment {} },
                }
            }) {
                CircularProgressIndicator(size: 18.0, color: color, stroke: 3.0)
            }
            Text(value: label, style: bold(TextKind::LabelLarge, color))
        }
    }
}

/// Text-only button.
///
/// Exposed to accessibility services like [`FilledButton`]; pass `identifier` for UI automation.
#[component]
pub fn text_button(
    label: Signal<String>,
    /// Accessibility identifier for UI automation (iOS `accessibilityIdentifier`).
    identifier: Option<String>,
    #[prop(default = palette().primary)] color: Color,
    on_tap: Callback<()>,
) -> Element {
    let identifier = identifier.clone();
    let accessibility =
        computed(move || semantics::button(identifier.as_deref(), label.get(), false));
    render! {
        InkWell(
            style: css!(padding: (px(8), px(12)), border_radius: px(shape().full)),
            accessibility: accessibility,
            splash_color: color.with_alpha(0.12),
            highlight_color: color.with_alpha(0.08),
            on_tap: on_tap,
        ) {
            Text(value: label, style: bold(TextKind::LabelLarge, color))
        }
    }
}
