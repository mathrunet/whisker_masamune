//! Small style helpers used by every widget.

use whisker::css::{AlignItems, Color, Display, FlexDirection, JustifyContent, PositionKind};
use whisker::prelude::*;

/// Absolutely positioned and stretched over the parent.
pub fn fill() -> Css {
    css!(position: PositionKind::Absolute, top: px(0), left: px(0), right: px(0), bottom: px(0))
}

/// Vertical flex container.
pub fn column() -> Css {
    css!(display: Display::Flex, flex_direction: FlexDirection::Column)
}

/// Horizontal flex container with vertically centered children.
pub fn row() -> Css {
    css!(display: Display::Flex, flex_direction: FlexDirection::Row, align_items: AlignItems::Center)
}

/// Vertical flex container that centers its children on both axes.
pub fn center() -> Css {
    css!(
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
    )
}

/// `#RRGGBB` (or `#RRGGBBAA` when translucent) for SVG and icon colors.
pub fn css_color(color: Color) -> String {
    let mut css = String::new();
    let _ = whisker::css::ToCss::to_css(&color, &mut css);
    if css.starts_with('#') {
        return css;
    }
    let inner = css
        .trim_start_matches("rgba(")
        .trim_start_matches("rgb(")
        .trim_end_matches(')');
    let parts: Vec<f32> = inner
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    match parts.as_slice() {
        [r, g, b] => format!("#{:02X}{:02X}{:02X}", *r as u8, *g as u8, *b as u8),
        [r, g, b, a] => format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            *r as u8,
            *g as u8,
            *b as u8,
            (a * 255.0).round() as u8
        ),
        _ => css,
    }
}
