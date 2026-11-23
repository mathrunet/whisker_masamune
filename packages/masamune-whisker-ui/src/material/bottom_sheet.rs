//! Modal bottom sheet (Flutter's `showModalBottomSheet`).
//!
//! Setting `open` to true slides the sheet up over a dimmed barrier; setting it to false slides
//! it down and unmounts it after the animation. Reversing midway continues from the current
//! position.
//!
//! Inside a [`ModalLayer`] (provided by `TabController`) the sheet is drawn by the layer's
//! [`ModalLayerHost`], so it covers the bottom navigation bar.

use std::rc::Rc;

use masamune_whisker_appearance::{palette, shape};
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::css::{
    AlignSelf, BackdropFilter, Color, Display, FlexDirection, JustifyContent, Transform,
    TransformFn,
};
use whisker::event::{ScrollEvent, TouchEvent};
use whisker::prelude::*;
use whisker_safe_area::safe_area_insets;

use crate::style::{column, fill};

/// Enter duration (ms), decelerating (easeOutCubic).
pub const SHEET_ENTER_MS: u32 = 400;
/// Exit duration (ms), accelerating (easeInCubic).
pub const SHEET_EXIT_MS: u32 = 250;

const DRAG_SLOP: f64 = 8.0;
const MIN_FLING_DISTANCE: f64 = 24.0;
const MIN_FLING_VELOCITY: f64 = 700.0;
const VELOCITY_TIMEOUT_MS: f64 = 100.0;

#[derive(Clone, Copy, PartialEq)]
enum DragDirection {
    Pending,
    Down,
    Other,
}

#[derive(Clone, Copy)]
struct SheetDrag {
    pointer_id: i64,
    start_x: f64,
    start_y: f64,
    height: f64,
    direction: DragDirection,
    last_y: f64,
    last_time: f64,
    last_motion: Option<(f64, f64)>,
}

impl SheetDrag {
    fn new(pointer_id: i64, x: f64, y: f64, time: f64, height: f64) -> Self {
        Self {
            pointer_id,
            start_x: x,
            start_y: y,
            height,
            direction: DragDirection::Pending,
            last_y: y,
            last_time: time,
            last_motion: None,
        }
    }

    fn update(&mut self, x: f64, y: f64, time: f64) -> f32 {
        let dx = (x - self.start_x).abs();
        let dy = y - self.start_y;
        if self.direction == DragDirection::Pending && dx.max(dy.abs()) >= DRAG_SLOP {
            self.direction = if dy > dx {
                DragDirection::Down
            } else {
                DragDirection::Other
            };
        }

        let elapsed = time - self.last_time;
        if y != self.last_y && elapsed > 0.0 {
            self.last_motion = Some((time, (y - self.last_y) * 1000.0 / elapsed));
            self.last_time = time;
            self.last_y = y;
        }

        if self.direction == DragDirection::Down {
            (dy / self.height).clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }

    fn should_dismiss(self, x: f64, y: f64, time: f64) -> bool {
        let distance = y - self.start_y;
        if self.direction != DragDirection::Down || distance <= (x - self.start_x).abs() {
            return false;
        }
        distance >= self.height * 0.5
            || (distance >= MIN_FLING_DISTANCE
                && self.last_motion.is_some_and(|(at, velocity)| {
                    time - at <= VELOCITY_TIMEOUT_MS && velocity > MIN_FLING_VELOCITY
                }))
    }
}

#[derive(Clone, Copy)]
struct CancelSheetDrag(Callback<()>);

#[derive(Clone)]
struct LayerEntry {
    id: u64,
    open: RwSignal<bool>,
    render: Rc<dyn Fn() -> Element>,
}

/// Layer that draws the [`ModalBottomSheet`]s of its descendants over a larger area, such as a
/// tab shell including its bottom navigation bar. Provide it with [`provide_modal_layer`] and
/// render [`ModalLayerHost`] where the sheets should cover.
#[derive(Clone, Copy)]
pub struct ModalLayer {
    entries: RwSignal<Vec<LayerEntry>>,
    next_id: RwSignal<u64>,
}

impl Default for ModalLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalLayer {
    pub fn new() -> Self {
        Self {
            entries: signal(Vec::new()),
            next_id: signal(0),
        }
    }

    /// Closes every open sheet in the layer (with its exit animation).
    pub fn dismiss_all(&self) {
        for entry in self.entries.get_untracked() {
            if entry.open.get_untracked() {
                entry.open.set(false);
            }
        }
    }

    fn register(&self, open: RwSignal<bool>, render: Rc<dyn Fn() -> Element>) -> u64 {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        self.entries
            .update(|entries| entries.push(LayerEntry { id, open, render }));
        id
    }

    fn unregister(&self, id: u64) {
        self.entries
            .update(|entries| entries.retain(|entry| entry.id != id));
    }
}

/// Creates a [`ModalLayer`] and provides it as context.
pub fn provide_modal_layer() -> ModalLayer {
    let layer = ModalLayer::new();
    provide_context(layer);
    layer
}

/// Draws the sheets of `layer`. Place it where they should cover (the sheets fill its parent).
#[component]
pub fn modal_layer_host(layer: ModalLayer) -> Element {
    let entries = layer.entries;
    render! {
        ForEach(
            each: move || entries.get(),
            key: |entry: &LayerEntry| entry.id,
            children: move |entry: ReadSignal<LayerEntry>| (entry.get_untracked().render)(),
        )
    }
}

#[component]
pub fn modal_bottom_sheet(
    open: RwSignal<bool>,
    /// Closes the sheet when the barrier is tapped.
    #[prop(default = true)]
    dismissible: bool,
    /// Ignores barrier taps and drag dismissal while true (for example during a request).
    #[prop(default = None)]
    locked: Option<ReadSignal<bool>>,
    #[prop(default = true)] drag_handle: bool,
    /// Horizontal content padding, in addition to the system safe area.
    #[prop(default = 24.0)]
    horizontal_padding: f32,
    /// Bottom content padding, in addition to the system safe area.
    #[prop(default = 24.0)]
    bottom_padding: f32,
    /// Space between the drag handle and the content.
    #[prop(default = 22.0)]
    drag_handle_spacing: f32,
    /// Maximum opacity of the black barrier.
    #[prop(default = 0.54)]
    barrier_opacity: f32,
    #[prop(default = palette().surface)] color: Color,
    /// Blur radius (logical pixels) applied to whatever is behind the sheet surface, like
    /// Flutter's `BackdropFilter` under a translucent sheet. Pair it with a translucent `color`.
    /// `0.0` (the default) disables the blur.
    #[prop(default = 0.0)]
    backdrop_blur: f32,
    children: Children,
) -> Element {
    let children = children.clone();
    let sheet = move || {
        let children = children.clone();
        let locked = computed(move || locked.is_some_and(|l| l.get()));
        render! {
            SheetState(
                open: open,
                dismissible: dismissible,
                locked: locked,
                drag_handle: drag_handle,
                horizontal_padding: horizontal_padding,
                bottom_padding: bottom_padding,
                drag_handle_spacing: drag_handle_spacing,
                barrier_opacity: barrier_opacity,
                color: color,
                backdrop_blur: backdrop_blur,
                content: children.clone(),
            )
        }
    };
    let Some(layer) = use_context::<ModalLayer>() else {
        return sheet();
    };
    let id = layer.register(open, Rc::new(sheet));
    on_cleanup(move || layer.unregister(id));
    render! { Fragment {} }
}

/// A vertical scroll view inside a [`ModalBottomSheet`].
///
/// An initial offset above zero leaves the entire gesture with the scroll view. A downward drag
/// can move the sheet when the viewport is already at the top as the gesture starts.
#[component]
pub fn modal_bottom_sheet_scroll_view(
    #[prop(default = Css::new())] style: Css,
    /// Receives the viewport's native scroll events.
    on_scroll: Option<Callback<ScrollEvent>>,
    /// Receives touches that begin in the viewport.
    on_touch_start: Option<Callback<TouchEvent>>,
    children: Children,
) -> Element {
    let cancel_drag = use_context::<CancelSheetDrag>();
    let scroll_top = StoredValue::new(0.0);
    let content = children.clone();
    render! {
        ScrollView(
            style: style.clone(),
            on_capture_touchstart: move |_: TouchEvent| {
                if scroll_top.get() > 0.0 && let Some(cancel_drag) = cancel_drag {
                    cancel_drag.0.run(());
                }
            },
            on_touchstart: move |event: TouchEvent| {
                if let Some(on_touch_start) = on_touch_start {
                    on_touch_start.run(event);
                }
            },
            on_scroll: move |event: ScrollEvent| {
                scroll_top.set(event.detail.scroll_top);
                if let Some(on_scroll) = on_scroll {
                    on_scroll.run(event);
                }
            },
        ) {
            {content()}
        }
    }
}

#[component]
fn sheet_state(
    open: RwSignal<bool>,
    dismissible: bool,
    locked: ReadSignal<bool>,
    drag_handle: bool,
    horizontal_padding: f32,
    bottom_padding: f32,
    drag_handle_spacing: f32,
    barrier_opacity: f32,
    color: Color,
    backdrop_blur: f32,
    content: Children,
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
    let dragged = signal(0.0_f32);
    let current_progress =
        move || (enter_progress.get_untracked() - dragged.get_untracked()).max(0.0);
    let progress = computed(move || {
        if open.get() {
            (enter_progress.get() - dragged.get()).max(0.0)
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
                exit.set_value(current_progress());
                dragged.set(0.0);
                exit.reverse();
            }
        });
    }
    let restore = {
        let enter = enter.clone();
        Callback::new(move |()| {
            if open.get_untracked() && dragged.get_untracked() > 0.0 {
                enter.set_value(current_progress());
                dragged.set(0.0);
                enter.forward();
            }
        })
    };
    let _keep = StoredValue::new((enter, exit));
    let children = content.clone();
    render! {
        Show(when: move || mounted.get()) {
            SheetFrame(
                open: open,
                progress: progress,
                dragged: dragged,
                restore: restore,
                dismissible: dismissible,
                locked: locked,
                drag_handle: drag_handle,
                horizontal_padding: horizontal_padding,
                bottom_padding: bottom_padding,
                drag_handle_spacing: drag_handle_spacing,
                barrier_opacity: barrier_opacity,
                color: color,
                backdrop_blur: backdrop_blur,
                content: children.clone(),
            )
        }
    }
}

#[component]
fn sheet_frame(
    open: RwSignal<bool>,
    progress: ReadSignal<f32>,
    dragged: RwSignal<f32>,
    restore: Callback<()>,
    dismissible: bool,
    locked: ReadSignal<bool>,
    drag_handle: bool,
    horizontal_padding: f32,
    bottom_padding: f32,
    drag_handle_spacing: f32,
    barrier_opacity: f32,
    color: Color,
    backdrop_blur: f32,
    content: Children,
) -> Element {
    let insets = safe_area_insets();
    let radius = shape().extra_large;
    let sheet_style = computed(move || {
        sheet_surface_style(color, backdrop_blur)
            .width(percent(100))
            .padding_top(px(12))
            .padding_left(px(horizontal_padding + insets.get().leading as f32))
            .padding_right(px(horizontal_padding + insets.get().trailing as f32))
            .padding_bottom(px(bottom_padding + insets.get().bottom as f32))
            .border_top_left_radius(px(radius))
            .border_top_right_radius(px(radius))
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
    let drag = StoredValue::new(None::<SheetDrag>);
    let cancel_drag = Callback::new(move |()| {
        drag.set(None);
        restore.run(());
    });
    provide_context(CancelSheetDrag(cancel_drag));
    effect(move || {
        if locked.get() || !open.get() {
            cancel_drag.run(());
        }
    });
    let start_drag = move |event: TouchEvent| {
        cancel_drag.run(());
        if !open.get_untracked() || locked.get_untracked() || event.touches.len() > 1 {
            return;
        }
        if let Some(size) = event.size.filter(|size| size.height > 0.0) {
            drag.set(Some(SheetDrag::new(
                event.pointer_id,
                event.detail.x,
                event.detail.y,
                event.timestamp,
                size.height,
            )));
        }
    };
    let move_drag = move |event: TouchEvent| {
        if event.touches.len() > 1 {
            cancel_drag.run(());
            return;
        }
        if let Some(mut gesture) = drag
            .get()
            .filter(|gesture| gesture.pointer_id == event.pointer_id)
        {
            dragged.set(gesture.update(event.detail.x, event.detail.y, event.timestamp));
            drag.set(Some(gesture));
        }
    };
    let end_drag = move |event: TouchEvent| {
        let Some(mut gesture) = drag
            .get()
            .filter(|gesture| gesture.pointer_id == event.pointer_id)
        else {
            return;
        };
        let x = event.detail.x;
        let y = event.detail.y;
        let time = event.timestamp;
        dragged.set(gesture.update(x, y, time));
        if open.get_untracked() && !locked.get_untracked() && gesture.should_dismiss(x, y, time) {
            drag.set(None);
            open.set(false);
        } else {
            cancel_drag.run(());
        }
    };
    let content = content.clone();
    render! {
        View(style: barrier_style) {
            View(style: css!(flex_grow: 1.0), on_tap: close) {
            }
            View(
                style: sheet_style,
                on_tap_catch: |_| {},
                on_capture_touchstart: start_drag,
                on_capture_touchmove: move_drag,
                on_capture_touchend: end_drag,
                on_capture_touchcancel: move |_| cancel_drag.run(()),
            ) {
                Show(when: move || drag_handle) {
                    View(
                        style: css!(
                            width: px(32),
                            height: px(4),
                            border_radius: px(2),
                            background_color: palette().outline,
                            align_self: AlignSelf::Center,
                            margin_bottom: px(drag_handle_spacing),
                        ),
                    ) {
                    }
                }
                {content()}
            }
        }
    }
}

/// Background of the sheet surface: `color`, plus a backdrop blur when `backdrop_blur > 0`.
fn sheet_surface_style(color: Color, backdrop_blur: f32) -> Css {
    let style = column().background_color(color);
    if backdrop_blur > 0.0 {
        style.backdrop_filter(BackdropFilter::blur(px(backdrop_blur)))
    } else {
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downward_drag_moves_and_dismisses_the_sheet_at_half_height() {
        let mut drag = SheetDrag::new(1, 40.0, 100.0, 0.0, 300.0);
        assert_eq!(drag.update(44.0, 220.0, 400.0), 0.4);
        assert!(!drag.should_dismiss(44.0, 220.0, 400.0));
        assert_eq!(drag.update(44.0, 250.0, 600.0), 0.5);
        assert!(drag.should_dismiss(44.0, 250.0, 600.0));
    }

    #[test]
    fn recent_downward_fling_can_dismiss_before_half_height() {
        let mut drag = SheetDrag::new(1, 40.0, 100.0, 0.0, 300.0);
        drag.update(40.0, 134.0, 40.0);
        drag.update(40.0, 134.0, 50.0);
        assert!(drag.should_dismiss(40.0, 134.0, 50.0));
        assert!(!drag.should_dismiss(40.0, 134.0, 200.0));
    }

    #[test]
    fn ignores_taps_upward_swipes_and_swipes_that_start_horizontally() {
        let mut tap = SheetDrag::new(1, 40.0, 100.0, 0.0, 300.0);
        assert_eq!(tap.update(41.0, 102.0, 5.0), 0.0);
        assert!(!tap.should_dismiss(41.0, 102.0, 5.0));

        let mut upward = tap;
        assert_eq!(upward.update(40.0, 70.0, 30.0), 0.0);
        assert!(!upward.should_dismiss(40.0, 70.0, 30.0));

        let mut horizontal = tap;
        assert_eq!(horizontal.update(70.0, 102.0, 30.0), 0.0);
        assert_eq!(horizontal.update(75.0, 290.0, 300.0), 0.0);
        assert!(!horizontal.should_dismiss(75.0, 290.0, 300.0));
    }

    #[test]
    fn dragging_back_reduces_the_sheet_offset_and_does_not_dismiss() {
        let mut drag = SheetDrag::new(1, 40.0, 100.0, 0.0, 300.0);
        assert_eq!(drag.update(40.0, 300.0, 100.0), 2.0 / 3.0);
        assert_eq!(drag.update(40.0, 130.0, 150.0), 0.1);
        assert!(!drag.should_dismiss(40.0, 130.0, 150.0));
    }

    #[test]
    fn sheet_surface_has_no_backdrop_blur_by_default() {
        let style = sheet_surface_style(Color::rgba(0, 0, 0, 0.45), 0.0).to_string();
        assert!(!style.contains("backdrop-filter"), "{style}");
    }

    #[test]
    fn sheet_surface_blurs_the_backdrop_when_requested() {
        let style = sheet_surface_style(Color::rgba(0, 0, 0, 0.45), 4.0).to_string();
        assert!(style.contains("backdrop-filter: blur(4px)"), "{style}");
    }
}
