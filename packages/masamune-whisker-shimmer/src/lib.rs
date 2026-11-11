//! Shimmer loading placeholders (katana_ui's `ShimmerBox` / `ShimmerMultiLine`, built on the
//! `shimmer` Flutter package).
//!
//! | Flutter | Whisker |
//! |---|---|
//! | `ShimmerBox` | [`ShimmerBox`] |
//! | `ShimmerMultiLine` | [`ShimmerMultiLine`] |
//! | `ShimmerDirection` | [`ShimmerDirection`] |
//! | `Shimmer.fromColors` gradient | [`shimmer_gradient`] driven by [`use_shimmer`] |
//!
//! The highlight band is a linear gradient redrawn every frame as the box background. Hosts clip
//! backgrounds to the border radius, so rounded boxes need neither `overflow` nor transforms
//! (transforms also move the layout position on Android). Unlike Flutter's `Shimmer(child)`, which
//! is a shader mask, the band cannot shine through arbitrary children such as text: build
//! placeholders from boxes.

use masamune_whisker_appearance::{palette, shape};
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::css::{AlignSelf, Color, ColorStop, Gradient, LinearDirection};
use whisker::prelude::*;

/// Default duration of one sweep, in milliseconds (`Shimmer.period`).
pub const DEFAULT_PERIOD_MS: u32 = 1500;

/// Center of the highlight band at the start and end of a sweep, in percent of the box.
///
/// Flutter paints the gradient over a rect three times the child's size that slides from one
/// width before to one width after the child, which puts the highlight at -50%..150%.
const BAND_START: f32 = -50.0;
const BAND_END: f32 = 150.0;
/// Distance from the band center to where it fades back to the base color, in percent of the
/// box (0.15 of the three-times-wide rect).
const BAND_HALF_WIDTH: f32 = 45.0;

/// Direction the highlight band travels (`ShimmerDirection`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShimmerDirection {
    /// Left to right.
    #[default]
    Ltr,
    /// Right to left.
    Rtl,
    /// Top to bottom.
    Ttb,
    /// Bottom to top.
    Btt,
}

impl ShimmerDirection {
    /// The CSS gradient direction whose 0% is where the band starts.
    pub fn linear_direction(self) -> LinearDirection {
        match self {
            ShimmerDirection::Ltr => LinearDirection::ToRight,
            ShimmerDirection::Rtl => LinearDirection::ToLeft,
            ShimmerDirection::Ttb => LinearDirection::ToBottom,
            ShimmerDirection::Btt => LinearDirection::ToTop,
        }
    }
}

/// Default base color: the theme's text over its background at 8%, kept opaque so the box looks
/// the same on any surface.
pub fn default_base_color() -> Color {
    let palette = palette();
    alpha_blend(palette.text, palette.background, 0.08)
}

/// Default highlight color: the theme's text over its background at 22%.
pub fn default_highlight_color() -> Color {
    let palette = palette();
    alpha_blend(palette.text, palette.background, 0.22)
}

/// `foreground` drawn over `background` at `opacity` (Flutter's `Color.alphaBlend` with
/// `foreground.withValues(alpha: opacity)`), as an opaque color.
///
/// Only `rgb`/`rgba` colors can be mixed; otherwise `foreground` is returned unchanged.
pub fn alpha_blend(foreground: Color, background: Color, opacity: f32) -> Color {
    match (foreground, background) {
        (Color::Rgba(fr, fg, fb, fa), Color::Rgba(br, bg, bb, _)) => {
            let t = (fa * opacity).clamp(0.0, 1.0);
            Color::Rgba(mix(br, fr, t), mix(bg, fg, t), mix(bb, fb, t), 1.0)
        }
        (foreground, _) => foreground,
    }
}

fn mix(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgba(ar, ag, ab, aa), Color::Rgba(br, bg, bb, ba)) => Color::Rgba(
            mix(ar, br, t),
            mix(ag, bg, t),
            mix(ab, bb, t),
            aa + (ba - aa) * t,
        ),
        (a, b) => {
            if t < 0.5 {
                a
            } else {
                b
            }
        }
    }
}

/// Stops `(color, position in percent)` of the band at `progress` (0..1 through a sweep).
///
/// Positions are inside 0..=100 and ascending. The colors at both ends are interpolated, so the
/// band enters and leaves the box smoothly.
fn band_stops(progress: f32, base: Color, highlight: Color) -> Vec<(Color, f32)> {
    let center = BAND_START + (BAND_END - BAND_START) * progress.clamp(0.0, 1.0);
    let (start, end) = (center - BAND_HALF_WIDTH, center + BAND_HALF_WIDTH);
    let color_at = |x: f32| {
        if x <= start || x >= end {
            base
        } else if x <= center {
            lerp_color(base, highlight, (x - start) / BAND_HALF_WIDTH)
        } else {
            lerp_color(highlight, base, (x - center) / BAND_HALF_WIDTH)
        }
    };
    let mut stops = vec![(color_at(0.0), 0.0)];
    for (x, color) in [(start, base), (center, highlight), (end, base)] {
        if x > 0.0 && x < 100.0 {
            stops.push((color, x));
        }
    }
    stops.push((color_at(100.0), 100.0));
    stops
}

/// The gradient of a shimmer box at `progress` (0..1 through a sweep).
pub fn shimmer_gradient(
    progress: f32,
    base: Color,
    highlight: Color,
    direction: ShimmerDirection,
) -> Gradient {
    Gradient::Linear {
        direction: direction.linear_direction(),
        stops: band_stops(progress, base, highlight)
            .into_iter()
            .map(|(color, position)| ColorStop::at(color, percent(position)))
            .collect(),
    }
}

/// Progress (0..1) of an endless sweep lasting `period_ms`, owned by the calling component.
/// Call in a component body. Share one signal between boxes that should shine together.
///
/// When `enabled` is false the value stays at 0.
pub fn use_shimmer(period_ms: u32, enabled: bool) -> ReadSignal<f32> {
    let controller = AnimationController::new(AnimConfig::linear(period_ms.max(1)));
    if enabled {
        controller.repeat();
    }
    let progress = controller.value();
    // The controller lives as long as the calling component.
    let _keep = StoredValue::new(controller);
    progress
}

/// A rounded placeholder box with a sweeping highlight (katana_ui's `ShimmerBox`).
///
/// Without `width` it stretches across its parent's cross axis; without `height` it grows along
/// the main axis to fill the parent.
#[component]
pub fn shimmer_box(
    /// Width in logical pixels. `None` stretches across the parent.
    #[prop(default = None)]
    width: Option<f32>,
    /// Height in logical pixels. `None` fills the remaining space of the parent.
    #[prop(default = None)]
    height: Option<f32>,
    /// Space around the box, in logical pixels.
    #[prop(default = 0.0)]
    padding: f32,
    /// Corner radius in logical pixels. Defaults to a quarter of `height`, or the theme's small
    /// shape without a height.
    #[prop(default = None)]
    border_radius: Option<f32>,
    #[prop(default = default_base_color())] base_color: Color,
    #[prop(default = default_highlight_color())] highlight_color: Color,
    #[prop(default = DEFAULT_PERIOD_MS)] period_ms: u32,
    #[prop(default = ShimmerDirection::Ltr)] direction: ShimmerDirection,
    /// False draws a still box in `base_color`.
    #[prop(default = true)]
    enabled: bool,
    /// Progress shared with other boxes (see [`use_shimmer`]). `None` runs its own sweep.
    #[prop(default = None)]
    progress: Option<ReadSignal<f32>>,
) -> Element {
    let progress = match progress {
        Some(progress) => progress,
        None if enabled => use_shimmer(period_ms, true),
        None => signal(0.0).read_only(),
    };
    let radius = border_radius.unwrap_or_else(|| height.map_or(shape().small, |h| h / 4.0));
    let outer = {
        let base = css!(padding: px(padding), flex_shrink: 0.0);
        let base = match width {
            Some(width) => base.width(px(width + padding * 2.0)),
            None => base.align_self(AlignSelf::Stretch),
        };
        match height {
            Some(height) => base.height(px(height + padding * 2.0)),
            None => base.flex_grow(1.0).flex_basis(px(0)),
        }
    };
    let inner = computed(move || {
        let style = css!(flex_grow: 1.0, border_radius: px(radius));
        if enabled {
            style.background_image(shimmer_gradient(
                progress.get(),
                base_color,
                highlight_color,
                direction,
            ))
        } else {
            style.background_color(base_color)
        }
    });
    render! {
        View(style: outer) {
            View(style: inner) {
            }
        }
    }
}

/// Stacked placeholder lines that shine together (katana_ui's `ShimmerMultiLine`).
#[component]
pub fn shimmer_multi_line(
    #[prop(default = 3)] line_count: usize,
    /// Gap between lines, in logical pixels.
    #[prop(default = 8.0)]
    line_space: f32,
    /// Width of each line. `None` stretches across the parent.
    #[prop(default = None)]
    width: Option<f32>,
    /// Height of each line in logical pixels.
    #[prop(default = 12.0)]
    height: f32,
    #[prop(default = None)] border_radius: Option<f32>,
    #[prop(default = default_base_color())] base_color: Color,
    #[prop(default = default_highlight_color())] highlight_color: Color,
    #[prop(default = DEFAULT_PERIOD_MS)] period_ms: u32,
    #[prop(default = ShimmerDirection::Ltr)] direction: ShimmerDirection,
    #[prop(default = true)] enabled: bool,
) -> Element {
    let progress = use_shimmer(period_ms, enabled);
    render! {
        View(style: css!(flex_shrink: 0.0)) {
            ForEach(
                each: move || (0..line_count).collect::<Vec<_>>(),
                key: |i: &usize| *i,
                children: move |i: ReadSignal<usize>| {
                    let gap = if i.get_untracked() == 0 {
                        0.0
                    } else {
                        line_space
                    };
                    render! {
                        View(style: css!(margin_top: px(gap))) {
                            ShimmerBox(
                                width: width,
                                height: Some(height),
                                border_radius: border_radius,
                                base_color: base_color,
                                highlight_color: highlight_color,
                                direction: direction,
                                enabled: enabled,
                                progress: Some(progress),
                            )
                        }
                    }
                },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Color = Color::rgb(40, 40, 40);
    const HIGHLIGHT: Color = Color::rgb(90, 90, 90);

    fn assert_ascending_in_range(stops: &[(Color, f32)]) {
        assert_eq!(stops.first().map(|s| s.1), Some(0.0));
        assert_eq!(stops.last().map(|s| s.1), Some(100.0));
        for pair in stops.windows(2) {
            assert!(pair[0].1 <= pair[1].1, "not ascending: {stops:?}");
        }
    }

    #[test]
    fn band_is_outside_at_both_ends() {
        for progress in [0.0, 1.0] {
            let stops = band_stops(progress, BASE, HIGHLIGHT);
            assert_ascending_in_range(&stops);
            assert!(stops.iter().all(|s| s.0 == BASE), "{progress}: {stops:?}");
        }
    }

    #[test]
    fn band_is_centered_halfway() {
        let stops = band_stops(0.5, BASE, HIGHLIGHT);
        assert_ascending_in_range(&stops);
        assert_eq!(
            stops,
            vec![
                (BASE, 0.0),
                (BASE, 5.0),
                (HIGHLIGHT, 50.0),
                (BASE, 95.0),
                (BASE, 100.0),
            ]
        );
    }

    #[test]
    fn edges_are_interpolated_while_entering() {
        // Center at -50 + 200 * 0.2 = -10: the band covers -55..35.
        let stops = band_stops(0.2, BASE, HIGHLIGHT);
        assert_ascending_in_range(&stops);
        assert_eq!(stops.len(), 3);
        assert_eq!(stops[1], (BASE, 35.0));
        assert_eq!(stops[0].0, lerp_color(HIGHLIGHT, BASE, 10.0 / 45.0));
    }

    #[test]
    fn directions_map_to_gradient_sides() {
        assert_eq!(
            ShimmerDirection::Ltr.linear_direction(),
            LinearDirection::ToRight
        );
        assert_eq!(
            ShimmerDirection::Rtl.linear_direction(),
            LinearDirection::ToLeft
        );
        assert_eq!(
            ShimmerDirection::Ttb.linear_direction(),
            LinearDirection::ToBottom
        );
        assert_eq!(
            ShimmerDirection::Btt.linear_direction(),
            LinearDirection::ToTop
        );
    }

    #[test]
    fn alpha_blend_is_opaque() {
        let blended = alpha_blend(Color::rgb(255, 255, 255), Color::rgb(0, 0, 0), 0.2);
        assert_eq!(blended, Color::Rgba(51, 51, 51, 1.0));
    }
}
