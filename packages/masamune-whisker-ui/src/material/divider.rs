//! Dividers and spacing.

use masamune_whisker_appearance::{TextKind, bold, palette};
use whisker::css::Color;
use whisker::prelude::*;

use crate::style::row;

/// Horizontal line.
#[component]
pub fn divider(#[prop(default = palette().divider)] color: Color) -> Element {
    render! { View(style: css!(height: px(1), background_color: color)) {} }
}

/// Divider with a centered label (`Divider` + `Text` + `Divider`).
#[component]
pub fn labeled_divider(label: Signal<String>) -> Element {
    render! {
        View(style: row().gap(px(16)).padding((px(8), px(0)))) {
            View(style: css!(flex_grow: 1.0, height: px(1), background_color: palette().divider)) {}
            Text(value: label, style: bold(TextKind::LabelMedium, palette().text))
            View(style: css!(flex_grow: 1.0, height: px(1), background_color: palette().divider)) {}
        }
    }
}

/// Fixed vertical space.
#[component]
pub fn vertical_gap(size: f32) -> Element {
    render! { View(style: css!(height: px(size), flex_shrink: 0.0)) {} }
}
