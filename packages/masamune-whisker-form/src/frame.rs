//! The frame around a field (katana_form's `InputDecoration` through `FormStyle`): label, border,
//! background, prefix / suffix and the error text under the field.

use masamune_whisker_appearance::{ColorExt, text_with};
use masamune_whisker_ui::Slot;
use masamune_whisker_ui::style::css_color;
use whisker::css::{AlignItems, Border, BorderStyle, PositionKind};
use whisker::prelude::*;
use whisker_icons::Icon;

use crate::style::{FormInputBorderStyle, FormLabelPosition, FormStyle, resolve};

/// Frame of a form field. Put the input in `children`.
///
/// The label sits on the top border (or above the frame, see [`FormLabelPosition`]); an error
/// colors the border and the label and is shown under the frame. Every built-in field uses it,
/// so custom fields wrapped in it look the same.
#[component]
pub fn form_field_frame(
    /// Label of the field (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Error shown under the frame; `None` shows nothing.
    error: Option<ReadSignal<Option<String>>>,
    /// Draws the field disabled ([`FormStyle::disabled_opacity`], disabled label color).
    #[prop(default = Signal::from(false))]
    disabled: Signal<bool>,
    /// Uses [`FormStyle::focused_border_color`] while true.
    focused: Option<ReadSignal<bool>>,
    /// Grows to fill the parent column (multi-line fields that take the rest of a page).
    #[prop(default = false)]
    expand: bool,
    /// Shown before the content (for example a currency sign).
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the content.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
    /// Called when the frame is tapped (select fields open their sheet).
    on_tap: Option<Callback<()>>,
    /// Accessibility of the frame (useful for tappable frames).
    #[prop(default = Accessibility::new().into())]
    accessibility: Signal<Accessibility>,
    children: Children,
) -> Element {
    let (children, prefix, suffix) = (children.clone(), prefix.clone(), suffix.clone());
    let style = resolve(style);
    let has_error = move || error.is_some_and(|e| e.get().is_some());
    let is_focused = move || focused.is_some_and(|f| f.get());
    let border_color = move || {
        if has_error() {
            style.error_color
        } else if is_focused() {
            style.focused_border_color.unwrap_or(style.border_color)
        } else {
            style.border_color
        }
    };
    let (content_v, content_h) = style.content_padding;
    let frame = computed(move || {
        let color = border_color();
        let css = css!(
            position: PositionKind::Relative,
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            align_items: if expand { AlignItems::Stretch } else { AlignItems::Center },
            padding: (px(content_v), px(content_h)),
            gap: px(style.gap),
            background_color: style.background_color,
        );
        let css = match style.border_style {
            FormInputBorderStyle::None => css.border_radius(px(style.radius)),
            FormInputBorderStyle::Outline => css.border_radius(px(style.radius)).border(
                Border::new()
                    .width(px(style.border_width))
                    .solid()
                    .color(color),
            ),
            FormInputBorderStyle::Underline => css
                .border_bottom_width(px(style.border_width))
                .border_bottom_style(BorderStyle::Solid)
                .border_bottom_color(color),
        };
        let css = match style.min_height {
            Some(height) => css.min_height(px(height)),
            None => css,
        };
        if expand {
            css.flex_grow(1.0).flex_basis(px(0))
        } else {
            css
        }
    });
    let label_color = move || {
        if has_error() {
            style.error_color
        } else if disabled.get() {
            style.label_color.with_alpha(0.6)
        } else {
            style.label_color
        }
    };
    let floating = style.label_position == FormLabelPosition::Floating
        && style.border_style == FormInputBorderStyle::Outline;
    let label_style = computed(move || {
        let css = text_with(style.label_kind, label_color());
        if floating {
            // Centered on the top border, with the background cutting the line behind it.
            let half = style.label_kind.line_height() / 2.0 + style.border_width;
            css.position(PositionKind::Absolute)
                .top(px(-half))
                .left(px((content_h - 4.0).max(0.0)))
                .padding((px(0), px(4)))
                .background_color(style.background_color)
        } else {
            css.padding_bottom(px(4))
        }
    });
    let has_label = label.is_some();
    let label_value = computed(move || label.map(|l| l.get()).unwrap_or_default());
    let error_value = computed(move || error.and_then(|e| e.get()).unwrap_or_default());
    let (outer_v, outer_h) = style.padding;
    let outer = computed(move || {
        let css = css!(
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            padding: (px(outer_v), px(outer_h)),
            opacity: if disabled.get() { style.disabled_opacity } else { 1.0 },
        );
        if expand {
            css.flex_grow(1.0).flex_basis(px(0))
        } else {
            css
        }
    });
    let content = css!(
        flex_grow: 1.0,
        flex_shrink: 1.0,
        flex_basis: px(0),
        min_width: px(0),
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: if expand { AlignItems::Stretch } else { AlignItems::Center },
        gap: px(style.gap),
    );
    let (error_top, error_h) = style.error_padding;
    let error_style = text_with(style.error_kind, style.error_color).padding((
        px(error_top),
        px(error_h),
        px(0),
        px(error_h),
    ));
    let label_on_top = has_label && !floating;
    let label_on_border = has_label && floating;
    render! {
        View(style: outer) {
            Show(when: move || label_on_top) {
                Text(value: label_value, style: label_style)
            }
            View(
                style: frame,
                accessibility: accessibility,
                on_tap: move |_| {
                    if let Some(on_tap) = on_tap
                        && !disabled.try_get_untracked().unwrap_or(false)
                    {
                        on_tap.run(());
                    }
                },
            ) {
                {prefix.render()}
                View(style: content.clone()) {
                    {children()}
                }
                {suffix.render()}
                Show(when: move || label_on_border) {
                    Text(value: label_value, style: label_style)
                }
            }
            Show(when: has_error) {
                Text(value: error_value, style: error_style.clone())
            }
        }
    }
}

/// The value of a select field (or its hint, faded, when empty) and the trailing icon.
#[component]
pub(crate) fn picked_value(
    value: Signal<String>,
    hint: Signal<String>,
    icon: Option<&'static str>,
    disabled: Signal<bool>,
    style: FormStyle,
) -> Element {
    let text_style = computed(move || {
        let empty = value.get().is_empty();
        let color = if disabled.get() {
            style.disabled_color
        } else if empty {
            style.hint_color
        } else {
            style.text_color
        };
        style.text_style(color).flex_grow(1.0).flex_shrink(1.0)
    });
    let shown = computed(move || {
        let value = value.get();
        if value.is_empty() { hint.get() } else { value }
    });
    let icon_color = css_color(style.icon_color);
    let icon_size = format!("{}", style.icon_size);
    render! {
        Fragment {
            Text(value: shown, max_lines: 1u32, style: text_style)
            Show(when: move || icon.is_some()) {
                Icon(
                    svg: icon.unwrap_or_default(),
                    color: icon_color.clone(),
                    size: icon_size.clone(),
                )
            }
        }
    }
}
