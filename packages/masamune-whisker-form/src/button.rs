//! Submit button of a form (katana_form's `FormButton`).

use masamune_whisker_appearance::{ColorExt, TextKind, palette, text_with};
use masamune_whisker_ui::style::css_color;
use masamune_whisker_ui::{CircularProgressIndicator, InkWell};
use whisker::css::{AlignItems, Color, FontWeight, JustifyContent, TextAlign};
use whisker::prelude::*;
use whisker_icons::Icon;

/// Filled button that submits a form (katana_form's `FormButton`): full width, corner radius
/// 32, 12 / 16 inside and 4 above and below by default.
///
/// While `disabled` it is drawn in the palette's `disabled` color and does not respond; while
/// `loading` it shows a spinner and does not respond. Exposed to accessibility services as a
/// button named `label`; pass `identifier` for UI automation.
#[component]
pub fn form_button(
    /// Text of the button.
    label: Signal<String>,
    /// Icon before the text.
    icon: Option<&'static str>,
    /// Accessibility identifier for UI automation (iOS `accessibilityIdentifier`).
    identifier: Option<String>,
    /// Background color.
    #[prop(default = palette().primary)]
    color: Color,
    /// Text and icon color.
    #[prop(default = palette().on_primary)]
    foreground: Color,
    /// Corner radius.
    #[prop(default = 32.0)]
    radius: f32,
    /// Space inside the button `(vertical, horizontal)`.
    #[prop(default = (12.0, 16.0))]
    padding: (f32, f32),
    /// Space above and below the button.
    #[prop(default = 4.0)]
    margin: f32,
    /// Bold text.
    #[prop(default = false)]
    bold: bool,
    /// Stretches to the full width.
    #[prop(default = true)]
    expand: bool,
    disabled: Option<ReadSignal<bool>>,
    loading: Option<ReadSignal<bool>>,
    on_tap: Callback<()>,
) -> Element {
    let off = move || disabled.is_some_and(|d| d.get());
    let busy = move || loading.is_some_and(|l| l.get());
    let (padding_v, padding_h) = padding;
    let style = computed(move || {
        let css = css!(
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            gap: px(8),
            padding: (px(padding_v), px(padding_h)),
            border_radius: px(radius),
            background_color: if off() { palette().disabled } else { color },
        );
        if expand { css.width(percent(100)) } else { css }
    });
    let foreground_now = move || {
        if off() {
            palette().on_primary.with_alpha(0.7)
        } else {
            foreground
        }
    };
    let label_style = computed(move || {
        text_with(TextKind::LabelLarge, foreground_now())
            .text_align(TextAlign::Center)
            .font_weight(if bold {
                FontWeight::Bold
            } else {
                FontWeight::Normal
            })
    });
    let icon_color = computed(move || css_color(foreground_now()));
    let identifier = identifier.clone();
    let accessibility = computed(move || {
        let accessibility = Accessibility::new()
            .label(label.get())
            .role(AccessibilityRole::Button)
            .state(AccessibilityState::new().disabled(off() || busy()));
        match identifier.as_deref() {
            Some(id) => accessibility.identifier(id),
            None => accessibility,
        }
    });
    let outer = css!(
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        padding: (px(margin), px(0)),
    );
    let outer = if expand {
        outer.width(percent(100))
    } else {
        outer
    };
    render! {
        View(style: outer.clone()) {
            InkWell(
                style: style,
                accessibility: accessibility,
                splash_color: foreground.with_alpha(0.16),
                highlight_color: foreground.with_alpha(0.08),
                disabled: computed(move || off() || busy()),
                on_tap: on_tap,
            ) {
                Show(when: busy) {
                    CircularProgressIndicator(size: 16.0, color: foreground, stroke: 3.0)
                }
                Show(when: move || icon.is_some() && !busy()) {
                    Icon(svg: icon.unwrap_or_default(), color: icon_color, size: "18")
                }
                Text(value: label, style: label_style)
            }
        }
    }
}
