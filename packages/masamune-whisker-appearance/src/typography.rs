//! Text styles with the Material 3 type scale, configurable per app through
//! [`crate::ThemeConfig::typography`].

use std::cell::Cell;

use whisker::css::{Color, FontWeight};
use whisker::prelude::*;

use crate::theme::{palette, typography};

/// Material 3 text style roles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextKind {
    DisplayLarge,
    DisplayMedium,
    DisplaySmall,
    HeadlineLarge,
    HeadlineMedium,
    HeadlineSmall,
    TitleLarge,
    TitleMedium,
    TitleSmall,
    BodyLarge,
    BodyMedium,
    BodySmall,
    LabelLarge,
    LabelMedium,
    LabelSmall,
}

impl TextKind {
    /// Every role, largest first.
    pub const ALL: [TextKind; 15] = [
        TextKind::DisplayLarge,
        TextKind::DisplayMedium,
        TextKind::DisplaySmall,
        TextKind::HeadlineLarge,
        TextKind::HeadlineMedium,
        TextKind::HeadlineSmall,
        TextKind::TitleLarge,
        TextKind::TitleMedium,
        TextKind::TitleSmall,
        TextKind::BodyLarge,
        TextKind::BodyMedium,
        TextKind::BodySmall,
        TextKind::LabelLarge,
        TextKind::LabelMedium,
        TextKind::LabelSmall,
    ];

    /// Style of this role in the current theme.
    pub fn style(self) -> TextStyle {
        typography().style(self)
    }

    /// Font size in logical pixels.
    pub fn size(self) -> f32 {
        self.style().size
    }

    /// Default font weight.
    pub fn weight(self) -> u16 {
        self.style().weight
    }

    /// Line height in logical pixels.
    pub fn line_height(self) -> f32 {
        self.style().line_height
    }

    /// Letter spacing in logical pixels.
    pub fn letter_spacing(self) -> f32 {
        self.style().letter_spacing
    }
}

/// Metrics of one text role (a Flutter `TextStyle` without color).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    /// Font size in logical pixels.
    pub size: f32,
    /// Font weight (100 to 900).
    pub weight: u16,
    /// Line height in logical pixels.
    pub line_height: f32,
    /// Letter spacing in logical pixels.
    pub letter_spacing: f32,
    /// Font family of this role. `None` uses the default family ([`font_family`]).
    pub family: Option<&'static str>,
}

impl TextStyle {
    pub const fn new(size: f32, weight: u16, line_height: f32, letter_spacing: f32) -> Self {
        Self {
            size,
            weight,
            line_height,
            letter_spacing,
            family: None,
        }
    }

    /// This style in `family` (for example a display font for headlines only).
    pub const fn with_family(self, family: &'static str) -> Self {
        Self {
            family: Some(family),
            ..self
        }
    }
}

/// Text styles of a theme (the Masamune `TextThemeData`).
///
/// Start from [`Typography::MATERIAL`] and override roles with struct update syntax.
/// `font_size_factor` and `font_size_delta` scale every role like Flutter's
/// `TextTheme.apply(fontSizeFactor:, fontSizeDelta:)`; line heights keep their ratio to the size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    pub display_large: TextStyle,
    pub display_medium: TextStyle,
    pub display_small: TextStyle,
    pub headline_large: TextStyle,
    pub headline_medium: TextStyle,
    pub headline_small: TextStyle,
    pub title_large: TextStyle,
    pub title_medium: TextStyle,
    pub title_small: TextStyle,
    pub body_large: TextStyle,
    pub body_medium: TextStyle,
    pub body_small: TextStyle,
    pub label_large: TextStyle,
    pub label_medium: TextStyle,
    pub label_small: TextStyle,
    /// Multiplies every font size.
    pub font_size_factor: f32,
    /// Added to every font size after `font_size_factor`.
    pub font_size_delta: f32,
}

impl Typography {
    /// Material 3 type scale. Titles keep weight 500 (including `title_large`) like earlier
    /// releases of this crate.
    pub const MATERIAL: Typography = Typography {
        display_large: TextStyle::new(57.0, 400, 64.0, -0.25),
        display_medium: TextStyle::new(45.0, 400, 52.0, 0.0),
        display_small: TextStyle::new(36.0, 400, 44.0, 0.0),
        headline_large: TextStyle::new(32.0, 400, 40.0, 0.0),
        headline_medium: TextStyle::new(28.0, 400, 36.0, 0.0),
        headline_small: TextStyle::new(24.0, 400, 32.0, 0.0),
        title_large: TextStyle::new(22.0, 500, 28.0, 0.0),
        title_medium: TextStyle::new(16.0, 500, 24.0, 0.15),
        title_small: TextStyle::new(14.0, 500, 20.0, 0.1),
        body_large: TextStyle::new(16.0, 400, 24.0, 0.5),
        body_medium: TextStyle::new(14.0, 400, 20.0, 0.25),
        body_small: TextStyle::new(12.0, 400, 16.0, 0.4),
        label_large: TextStyle::new(14.0, 500, 20.0, 0.1),
        label_medium: TextStyle::new(12.0, 500, 16.0, 0.5),
        label_small: TextStyle::new(11.0, 500, 16.0, 0.5),
        font_size_factor: 1.0,
        font_size_delta: 0.0,
    };

    /// The configured style of `kind` before scaling.
    pub fn base(&self, kind: TextKind) -> TextStyle {
        match kind {
            TextKind::DisplayLarge => self.display_large,
            TextKind::DisplayMedium => self.display_medium,
            TextKind::DisplaySmall => self.display_small,
            TextKind::HeadlineLarge => self.headline_large,
            TextKind::HeadlineMedium => self.headline_medium,
            TextKind::HeadlineSmall => self.headline_small,
            TextKind::TitleLarge => self.title_large,
            TextKind::TitleMedium => self.title_medium,
            TextKind::TitleSmall => self.title_small,
            TextKind::BodyLarge => self.body_large,
            TextKind::BodyMedium => self.body_medium,
            TextKind::BodySmall => self.body_small,
            TextKind::LabelLarge => self.label_large,
            TextKind::LabelMedium => self.label_medium,
            TextKind::LabelSmall => self.label_small,
        }
    }

    /// The style of `kind` with `font_size_factor` and `font_size_delta` applied.
    pub fn style(&self, kind: TextKind) -> TextStyle {
        let base = self.base(kind);
        let size = base.size * self.font_size_factor + self.font_size_delta;
        let line_height = if base.size > 0.0 {
            base.line_height * size / base.size
        } else {
            base.line_height
        };
        TextStyle {
            size,
            line_height,
            ..base
        }
    }
}

impl Default for Typography {
    fn default() -> Self {
        Self::MATERIAL
    }
}

fn default_family() -> &'static str {
    ""
}

thread_local! {
    static FAMILY: Cell<fn() -> &'static str> = const { Cell::new(default_family) };
}

/// Sets how the default font family is chosen (for example per language). An empty name keeps
/// the platform font. [`crate::init_theme`] calls this when [`crate::ThemeConfig::font_family`]
/// is set.
pub fn set_font_family_resolver(resolver: fn() -> &'static str) {
    FAMILY.with(|family| family.set(resolver));
}

/// The default font family for the current language.
pub fn font_family() -> &'static str {
    FAMILY.with(|family| family.get()())
}

fn style(kind: TextKind, weight: Option<u16>, color: Color) -> Css {
    let metrics = kind.style();
    let css = css!(
        font_size: px(metrics.size),
        font_weight: FontWeight::Numeric(weight.unwrap_or(metrics.weight)),
        line_height: px(metrics.line_height),
        letter_spacing: px(metrics.letter_spacing),
        color: color,
    );
    match metrics.family.unwrap_or_else(font_family) {
        "" => css,
        family => css.font_family(family),
    }
}

/// Text style in the palette's text color.
pub fn text(kind: TextKind) -> Css {
    text_with(kind, palette().text)
}

/// Text style in `color`.
pub fn text_with(kind: TextKind, color: Color) -> Css {
    style(kind, None, color)
}

/// Bold (700) text style in `color`.
pub fn bold(kind: TextKind, color: Color) -> Css {
    style(kind, Some(700), color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_scale_matches_material_3() {
        let scale = Typography::MATERIAL;
        assert_eq!(
            scale.style(TextKind::BodyMedium),
            TextStyle::new(14.0, 400, 20.0, 0.25)
        );
        assert_eq!(
            scale.style(TextKind::DisplayLarge),
            TextStyle::new(57.0, 400, 64.0, -0.25)
        );
        assert_eq!(scale.style(TextKind::LabelLarge).weight, 500);
    }

    #[test]
    fn factor_and_delta_scale_sizes_and_line_heights() {
        let scale = Typography {
            font_size_factor: 2.0,
            font_size_delta: 1.0,
            ..Typography::MATERIAL
        };
        let body = scale.style(TextKind::BodyMedium);
        assert_eq!(body.size, 29.0);
        assert_eq!(body.line_height, 20.0 * 29.0 / 14.0);
        assert_eq!(body.letter_spacing, 0.25);
    }

    #[test]
    fn roles_can_have_their_own_family() {
        let scale = Typography {
            headline_large: Typography::MATERIAL.headline_large.with_family("Display"),
            ..Typography::MATERIAL
        };
        assert_eq!(scale.style(TextKind::HeadlineLarge).family, Some("Display"));
        assert_eq!(scale.style(TextKind::BodyMedium).family, None);
    }
}
