//! Modal bottom sheet (Flutter's `showModalBottomSheet`).
//!
//! Setting `open` to true slides the sheet up over a dimmed barrier; setting it to false slides
//! it down and unmounts it after the animation. Reversing midway continues from the current
//! position.

use masamune_whisker_appearance::palette;
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::css::{
    AlignSelf, Color, Display, FlexDirection, JustifyContent, Transform, TransformFn,
};
use whisker::prelude::*;
use whisker_safe_area::safe_area_insets;

use crate::style::{column, fill};

/// Enter duration (ms), decelerating (easeOutCubic).
pub const SHEET_ENTER_MS: u32 = 400;
/// Exit duration (ms), accelerating (easeInCubic).
pub const SHEET_EXIT_MS: u32 = 250;

#[component]
pub fn modal_bottom_sheet(
    open: RwSignal<bool>,
    /// Closes the sheet when the barrier is tapped.
    #[prop(default = true)]
    dismissible: bool,
    /// Ignores barrier taps while true (for example during a request).
    #[prop(default = None)]
    locked: Option<ReadSignal<bool>>,
    #[prop(default = true)] drag_handle: bool,
    /// Maximum opacity of the black barrier.
    #[prop(default = 0.54)]
    barrier_opacity: f32,
    #[prop(default = palette().surface)] color: Color,
    children: Children,
) -> Element {
    // Keep the sheet mounted until the exit animation finishes.
    let mounted = signal(false);
    let enter = AnimationController::new(AnimConfig::cubic_bezier(
        SHEET_ENTER_MS,
        0.33,
        1.0,
        0.68,
        1.0,
    ));
    let exit = AnimationController::new(AnimConfig::cubic_bezier(
        SHEET_EXIT_MS,
        0.32,
        0.0,
        0.67,
        0.0,
    ));
    let enter_progress = enter.value();
    let exit_progress = exit.value();
    let progress = computed(move || {
        if open.get() {
            enter_progress.get()
        } else {
            exit_progress.get()
        }
    });
    exit.on_finish(move |finished| {
        if finished && exit_progress.get_untracked() <= 0.0 {
            mounted.set(false);
        }
    });
    {
        let enter = enter.clone();
        let exit = exit.clone();
        effect(move || {
            if open.get() {
                exit.stop();
                enter.set_value(exit_progress.get_untracked());
                mounted.set(true);
                enter.forward();
            } else {
                enter.stop();
                exit.set_value(enter_progress.get_untracked());
                exit.reverse();
            }
        });
    }
    let _keep = StoredValue::new((enter, exit));
    let locked = computed(move || locked.is_some_and(|l| l.get()));
    let children = children.clone();
    render! {
        Show(when: move || mounted.get()) {
            SheetFrame(
                open: open,
                progress: progress,
                dismissible: dismissible,
                locked: locked,
                drag_handle: drag_handle,
                barrier_opacity: barrier_opacity,
                color: color,
                content: children.clone(),
            )
        }
    }
}

#[component]
fn sheet_frame(
    open: RwSignal<bool>,
    progress: ReadSignal<f32>,
    dismissible: bool,
    locked: ReadSignal<bool>,
    drag_handle: bool,
    barrier_opacity: f32,
    color: Color,
    content: Children,
) -> Element {
    let insets = safe_area_insets();
    let sheet_style = computed(move || {
        column()
            .width(percent(100))
            .padding_top(px(12))
            .padding_left(px(24))
            .padding_right(px(24))
            .padding_bottom(px(24.0 + insets.get().bottom as f32))
            .border_top_left_radius(px(28))
            .border_top_right_radius(px(28))
            .background_color(color)
            .transform(Transform::new().push(TransformFn::TranslateY(
                percent((1.0 - progress.get()) * 100.0).into(),
            )))
    });
    let barrier_style = computed(move || {
        fill()
            .z_index(800)
            .justify_content(JustifyContent::FlexEnd)
            .display(Display::Flex)
            .flex_direction(FlexDirection::Column)
            .background_color(Color::rgba(0, 0, 0, barrier_opacity * progress.get()))
    });
    let close = move |_| {
        if dismissible && !locked.get_untracked() {
            open.set(false);
        }
    };
    let content = content.clone();
    render! {
        View(style: barrier_style) {
            View(style: css!(flex_grow: 1.0), on_tap: close) {}
            View(style: sheet_style, on_tap_catch: |_| {}) {
                Show(when: move || drag_handle) {
                    View(style: css!(
                        width: px(32),
                        height: px(4),
                        border_radius: px(2),
                        background_color: palette().outline,
                        align_self: AlignSelf::Center,
                        margin_bottom: px(22),
                    )) {}
                }
                {content()}
            }
        }
    }
}
