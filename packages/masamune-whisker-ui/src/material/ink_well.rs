//! Touch feedback for tappable areas (Flutter's `InkWell`).
//!
//! A press shows a translucent highlight over the whole area and an ink splash that grows from
//! the touch point, both clipped to the area (round it with `border_radius` in `style`). On
//! release the ink fades out while the tap still fires as usual.
//!
//! Like Flutter's tap recognizer, the feedback starts 100 ms after the finger goes down (or on
//! release, for quicker taps) and is cancelled once the finger moves past the tap slop, so
//! touches that start a scroll do not light up.

use std::cell::Cell;
use std::rc::Rc;

use masamune_whisker_appearance::{ColorExt, palette};
use whisker::css::{Color, Overflow, PointerEvents, PositionKind};
use whisker::event::TouchEvent;
use whisker::prelude::*;
use whisker::{AnimConfig, AnimationController};

use crate::style::fill;

/// Delay before a press is shown (Flutter's `kPressTimeout`).
const PRESS_DELAY_MS: u32 = 100;
/// Time for the splash to cover the whole area.
const SPLASH_GROW_MS: u32 = 300;
/// Fade-out after release.
const SPLASH_FADE_MS: u32 = 250;
/// Highlight fade in / out.
const HIGHLIGHT_MS: u32 = 150;
/// Movement that cancels the press; the same slop the framework uses for taps.
const TAP_SLOP: f64 = 10.0;

/// Default ink color (Material's pressed state layer over `text`).
pub fn default_splash_color() -> Color {
    palette().text.with_alpha(0.12)
}

/// Default highlight color.
pub fn default_highlight_color() -> Color {
    palette().text.with_alpha(0.08)
}

/// Radius that reaches the farthest corner of a `width` x `height` box from `(x, y)`.
fn splash_radius(x: f32, y: f32, width: f32, height: f32) -> f32 {
    [(0.0, 0.0), (width, 0.0), (0.0, height), (width, height)]
        .into_iter()
        .map(|(cx, cy)| ((cx - x).powi(2) + (cy - y).powi(2)).sqrt())
        .fold(0.0, f32::max)
}

#[derive(Default)]
struct Press {
    /// Surface position where the finger went down.
    origin: Cell<(f64, f64)>,
    /// Finger is down and the press has not been cancelled.
    down: Cell<bool>,
    /// The ink is shown for the current press.
    shown: Cell<bool>,
}

/// Tappable area with ink feedback. Put the content in its children and its layout (size,
/// padding, background, `border_radius`, ...) in `style`; the ink is clipped to that box.
///
/// `accessibility` is applied to the tappable box, so identifiers and labels keep working for UI
/// automation. While `disabled` is true there is no feedback and `on_tap` / `on_long_press` do
/// not fire.
#[component]
pub fn ink_well(
    #[prop(default = Css::new().into())] style: Signal<Css>,
    /// Accessibility of the tappable box (identifier, label, role, state).
    #[prop(default = Accessibility::new().into())]
    accessibility: Signal<Accessibility>,
    disabled: Option<ReadSignal<bool>>,
    /// Color of the splash; defaults to [`default_splash_color`].
    splash_color: Option<Color>,
    /// Color of the highlight; defaults to [`default_highlight_color`].
    highlight_color: Option<Color>,
    on_tap: Option<Callback<()>>,
    on_long_press: Option<Callback<()>>,
    children: Children,
) -> Element {
    let projected = children.clone();
    let splash_color = splash_color.unwrap_or_else(default_splash_color);
    let highlight_color = highlight_color.unwrap_or_else(default_highlight_color);
    let is_disabled = move || disabled.is_some_and(|d| d.get_untracked());

    let delay = AnimationController::new(AnimConfig::linear(PRESS_DELAY_MS));
    let grow = AnimationController::new(AnimConfig::ease_out(SPLASH_GROW_MS));
    let fade = AnimationController::new(AnimConfig::linear(SPLASH_FADE_MS));
    let highlight = AnimationController::new(AnimConfig::ease_out(HIGHLIGHT_MS));
    let center = RwSignal::new((0.0_f32, 0.0_f32));
    let radius = RwSignal::new(0.0_f32);
    // Geometry of the pending press, taken from the touchstart event.
    let pending = RwSignal::new((0.0_f32, 0.0_f32, 0.0_f32));
    let press = Rc::new(Press::default());

    let show = {
        let (grow, fade, highlight, press) = (
            grow.clone(),
            fade.clone(),
            highlight.clone(),
            Rc::clone(&press),
        );
        Rc::new(move || {
            if press.shown.replace(true) {
                return;
            }
            let (x, y, r) = pending.get_untracked();
            center.set((x, y));
            radius.set(r);
            grow.set_value(0.0);
            grow.forward();
            fade.set_value(1.0);
            highlight.forward();
        })
    };
    let release = {
        let (fade, highlight, delay, press) = (
            fade.clone(),
            highlight.clone(),
            delay.clone(),
            Rc::clone(&press),
        );
        Rc::new(move || {
            press.down.set(false);
            delay.stop();
            if press.shown.replace(false) {
                fade.animate_to(0.0);
                highlight.reverse();
            }
        })
    };
    delay.on_finish({
        let (show, press) = (Rc::clone(&show), Rc::clone(&press));
        move |finished| {
            if finished && press.down.get() {
                show();
            }
        }
    });

    let on_down = {
        let (delay, press) = (delay.clone(), Rc::clone(&press));
        move |event: TouchEvent| {
            if is_disabled() {
                return;
            }
            let (x, y, r) = match (event.offset, event.size) {
                (Some(offset), Some(size)) => {
                    let (x, y) = (offset.x as f32, offset.y as f32);
                    (
                        x,
                        y,
                        splash_radius(x, y, size.width as f32, size.height as f32),
                    )
                }
                // No laid-out box: show the highlight only.
                _ => (0.0, 0.0, 0.0),
            };
            pending.set((x, y, r));
            press.origin.set((event.detail.x, event.detail.y));
            press.down.set(true);
            press.shown.set(false);
            delay.set_value(0.0);
            delay.forward();
        }
    };
    let on_move = {
        let (release, press) = (Rc::clone(&release), Rc::clone(&press));
        move |event: TouchEvent| {
            if !press.down.get() {
                return;
            }
            let (x, y) = press.origin.get();
            let (dx, dy) = (event.detail.x - x, event.detail.y - y);
            if dx * dx + dy * dy > TAP_SLOP * TAP_SLOP {
                release();
            }
        }
    };
    let on_up = {
        let (show, release, press) = (Rc::clone(&show), Rc::clone(&release), Rc::clone(&press));
        move |_: TouchEvent| {
            // A tap released before the press delay still flashes once.
            if press.down.get() {
                show();
            }
            release();
        }
    };
    let on_cancel = {
        let release = Rc::clone(&release);
        move |_: TouchEvent| release()
    };

    let box_style = computed(move || style.get().overflow(Overflow::Hidden));
    let highlight_value = highlight.value();
    let highlight_style = computed(move || {
        fill()
            .background_color(highlight_color)
            .opacity(highlight_value.get())
            .pointer_events(PointerEvents::None)
    });
    let (grow_value, fade_value) = (grow.value(), fade.value());
    let splash_style = computed(move || {
        let (x, y) = center.get();
        let r = radius.get() * grow_value.get();
        css!(
            position: PositionKind::Absolute,
            left: px(x - r),
            top: px(y - r),
            width: px(r * 2.0),
            height: px(r * 2.0),
            border_radius: px(r),
            background_color: splash_color,
            opacity: fade_value.get(),
            pointer_events: PointerEvents::None,
        )
    });
    // The controllers live as long as the widget.
    let _keep = StoredValue::new((delay, grow, fade, highlight));

    render! {
        View(
            style: box_style,
            accessibility: accessibility,
            on_touchstart: on_down,
            on_touchmove: on_move,
            on_touchend: on_up,
            on_touchcancel: on_cancel,
            on_tap: move |_| {
                if let Some(on_tap) = on_tap && !is_disabled() {
                    on_tap.run(());
                }
            },
            on_longpress: move |_| {
                if let Some(on_long_press) = on_long_press && !is_disabled() {
                    on_long_press.run(());
                }
            },
        ) {
            {projected()}
            View(style: highlight_style)
            View(style: splash_style)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_reaches_the_farthest_corner() {
        assert_eq!(splash_radius(0.0, 0.0, 30.0, 40.0), 50.0);
        assert_eq!(splash_radius(30.0, 40.0, 30.0, 40.0), 50.0);
        assert_eq!(splash_radius(15.0, 20.0, 30.0, 40.0), 25.0);
    }
}
