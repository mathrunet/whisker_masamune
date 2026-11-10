//! Text styles with the Material 3 type scale.

use std::cell::Cell;

use whisker::css::{Color, FontWeight};
use whisker::prelude::*;

use crate::theme::palette;

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
    /// Font size in logical pixels.
    pub fn size(self) -> f32 {
        match self {
            TextKind::DisplayLarge => 57.0,
            TextKind::DisplayMedium => 45.0,
            TextKind::DisplaySmall => 36.0,
            TextKind::HeadlineLarge => 32.0,
            TextKind::HeadlineMedium => 28.0,
            TextKind::HeadlineSmall => 24.0,
            TextKind::TitleLarge => 22.0,
            TextKind::TitleMedium => 16.0,
            TextKind::TitleSmall => 14.0,
            TextKind::BodyLarge => 16.0,
            TextKind::BodyMedium => 14.0,
            TextKind::BodySmall => 12.0,
            TextKind::LabelLarge => 14.0,
            TextKind::LabelMedium => 12.0,
            TextKind::LabelSmall => 11.0,
        }
    }

    /// Default font weight.
    pub fn weight(self) -> u16 {
        match self {
            TextKind::TitleLarge
            | TextKind::TitleMedium
            | TextKind::TitleSmall
            | TextKind::LabelLarge
            | TextKind::LabelMedium
            | TextKind::LabelSmall => 500,
            _ => 400,
        }
    }
}

fn default_family() -> &'static str {
    ""
}

thread_local! {
    static FAMILY: Cell<fn() -> &'static str> = const { Cell::new(default_family) };
}

/// Sets how the default font family is chosen (for example per language). An empty name keeps
/// the platform font.
pub fn set_font_family_resolver(resolver: fn() -> &'static str) {
    FAMILY.with(|family| family.set(resolver));
}

/// The default font family for the current language.
pub fn font_family() -> &'static str {
    FAMILY.with(|family| family.get()())
}

fn style(kind: TextKind, weight: u16, color: Color) -> Css {
    let css = css!(
        font_size: px(kind.size()),
        font_weight: FontWeight::Numeric(weight),
        color: color,
    );
    match font_family() {
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
    style(kind, kind.weight(), color)
}

/// Bold (700) text style in `color`.
pub fn bold(kind: TextKind, color: Color) -> Css {
    style(kind, 700, color)
}
