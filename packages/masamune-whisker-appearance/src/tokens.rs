//! Shape, spacing and elevation tokens of the Material 3 design system.

use whisker::css::BoxShadow;
use whisker::prelude::*;

use crate::palette::ColorExt;
use crate::theme::palette;

/// Corner radii in logical pixels (the Material 3 shape scale).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub none: f32,
    /// Chips, snackbars, text fields.
    pub extra_small: f32,
    /// Menus, small cards.
    pub small: f32,
    /// Cards.
    pub medium: f32,
    /// Navigation drawers, large FABs.
    pub large: f32,
    /// Dialogs, bottom sheets.
    pub extra_large: f32,
    /// Pill or circle: larger than any side, so the radius is clamped to half of it.
    pub full: f32,
}

impl Shape {
    pub const DEFAULT: Shape = Shape {
        none: 0.0,
        extra_small: 4.0,
        small: 8.0,
        medium: 12.0,
        large: 16.0,
        extra_large: 28.0,
        full: 999.0,
    };
}

impl Default for Shape {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Gaps and paddings in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub extra_small: f32,
    pub small: f32,
    pub medium: f32,
    pub large: f32,
    pub extra_large: f32,
}

impl Spacing {
    pub const DEFAULT: Spacing = Spacing {
        extra_small: 4.0,
        small: 8.0,
        medium: 16.0,
        large: 24.0,
        extra_large: 32.0,
    };
}

impl Default for Spacing {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// `box-shadow` of a Material 3 elevation level (0 to 5, higher values are treated as 5) in the
/// palette's shadow color. Level 0 clears the shadow. Merge it into a style:
/// `css!(..).merge(elevation(1))`.
pub fn elevation(level: u8) -> Css {
    // (key y, key blur, ambient y, ambient blur, ambient spread) from the Material 3 tokens.
    let (key_y, key_blur, ambient_y, ambient_blur, ambient_spread) = match level {
        0 => return Css::new().box_shadows([]),
        1 => (1.0, 2.0, 1.0, 3.0, 1.0),
        2 => (1.0, 2.0, 2.0, 6.0, 2.0),
        3 => (1.0, 3.0, 4.0, 8.0, 3.0),
        4 => (2.0, 3.0, 6.0, 10.0, 4.0),
        _ => (4.0, 4.0, 8.0, 12.0, 6.0),
    };
    let shadow = palette().shadow;
    Css::new().box_shadows([
        BoxShadow::outer(
            px(0),
            px(key_y),
            px(key_blur),
            px(0),
            shadow.with_alpha(0.3),
        ),
        BoxShadow::outer(
            px(0),
            px(ambient_y),
            px(ambient_blur),
            px(ambient_spread),
            shadow.with_alpha(0.15),
        ),
    ])
}
