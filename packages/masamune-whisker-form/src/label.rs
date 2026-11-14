//! Section header of a form (katana_form's `FormLabel`).

use masamune_whisker_appearance::{ColorExt, TextKind, palette, text_with};
use masamune_whisker_ui::Slot;
use masamune_whisker_ui::style::css_color;
use whisker::css::{AlignItems, Color};
use whisker::prelude::*;
use whisker_icons::Icon;

/// Section header: a short line, an optional icon, the label and a line to the end
/// (katana_form's `FormLabel`). `show_divider: false` drops both lines.
#[component]
pub fn form_label(
    /// Text of the header.
    label: Signal<String>,
    /// Icon before the text.
    icon: Option<&'static str>,
    /// Color of the text and the icon; defaults to the palette's `text`.
    color: Option<Color>,
    /// Color of the lines; defaults to `color` at half opacity.
    divider_color: Option<Color>,
    /// Draws the lines.
    #[prop(default = true)]
    show_divider: bool,
    /// Shown before the icon.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the line at the end.
    #[prop(default = Slot::default())]
    suffix: Slot,
) -> Element {
    let (prefix, suffix) = (prefix.clone(), suffix.clone());
    let color = color.unwrap_or_else(|| palette().text);
    let divider_color = divider_color.unwrap_or_else(|| color.with_alpha(0.5));
    let line = move |grow: bool| {
        let css = css!(height: px(1), background_color: divider_color);
        if grow {
            css.flex_grow(1.0).flex_basis(px(0))
        } else {
            css.width(px(12))
        }
    };
    let has_suffix = suffix.is_some();
    render! {
        View(
            style: css!(
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: px(4),
            ),
        ) {
            Show(when: move || show_divider) {
                View(style: line(false))
            }
            {prefix.render()}
            Show(when: move || icon.is_some()) {
                Icon(svg: icon.unwrap_or_default(), color: css_color(color), size: "12")
            }
            Text(value: label, style: text_with(TextKind::BodySmall, color))
            Show(
                when: move || show_divider,
                fallback: || render! { View(style: css!(flex_grow: 1.0)) },
            ) {
                View(style: line(true))
            }
            {suffix.render()}
            Show(when: move || show_divider && has_suffix) {
                View(style: line(false))
            }
        }
    }
}
