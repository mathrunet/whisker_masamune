//! Indeterminate circular progress indicator (Flutter's `CircularProgressIndicator`).
//!
//! While it is shown it runs a `masamune.indicator.show|<trace_name>` trace
//! (`masamune_whisker_logger::start_trace`), so slow-operation detectors such as
//! `masamune-whisker-ai-debugger` notice spinners that stay up too long.

use masamune_whisker_appearance::palette;
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::css::Color;
use whisker::prelude::*;
use whisker_svg::Svg;

use crate::style::css_color;

#[component]
pub fn circular_progress_indicator(
    #[prop(default = 32.0)] size: f32,
    #[prop(default = palette().primary)] color: Color,
    #[prop(default = 3.0)] stroke: f32,
    /// Fixed code naming what is loading, used in the trace name.
    #[prop(default = "circular_progress_indicator".to_owned())]
    trace_name: String,
) -> Element {
    let trace =
        masamune_whisker_logger::start_trace(format!("masamune.indicator.show|{trace_name}"));
    on_cleanup(move || drop(trace));
    let controller = AnimationController::new(AnimConfig::linear(900));
    controller.repeat();
    let progress = controller.value();
    let _keep = StoredValue::new(controller);
    let color = css_color(color);
    let r = 20.0 - stroke;
    // Rotating the View with CSS also moves its layout position on Android, so only the drawing
    // inside a fixed frame rotates.
    let svg = computed(move || {
        format!(
            r##"<svg viewBox="0 0 40 40"><g transform="rotate({angle} 20 20)"><path d="M 20 {top} A {r} {r} 0 1 1 {left} 20" stroke="currentColor" stroke-width="{w}" fill="none"/></g></svg>"##,
            angle = progress.get() * 360.0,
            top = 20.0 - r,
            left = 20.0 - r,
            w = stroke,
        )
    });
    render! {
        View(style: css!(width: px(size), height: px(size), flex_shrink: 0.0)) {
            Svg(content: svg, color: color, style: css!(width: px(size), height: px(size), flex_shrink: 0.0))
        }
    }
}
