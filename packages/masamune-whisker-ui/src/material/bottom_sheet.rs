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
    AlignSelf, Color, Display, FlexDirection, JustifyContent, Transform, TransformFn,
};
use whisker::prelude::*;
use whisker_safe_area::safe_area_insets;

use crate::style::{column, fill};

/// Enter duration (ms), decelerating (easeOutCubic).
pub const SHEET_ENTER_MS: u32 = 400;
/// Exit duration (ms), accelerating (easeInCubic).
pub const SHEET_EXIT_MS: u32 = 250;

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
    /// Ignores barrier taps while true (for example during a request).
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
    let children = content.clone();
    render! {
        Show(when: move || mounted.get()) {
            SheetFrame(
                open: open,
                progress: progress,
                dismissible: dismissible,
                locked: locked,
                drag_handle: drag_handle,
                horizontal_padding: horizontal_padding,
                bottom_padding: bottom_padding,
                drag_handle_spacing: drag_handle_spacing,
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
    horizontal_padding: f32,
    bottom_padding: f32,
    drag_handle_spacing: f32,
    barrier_opacity: f32,
    color: Color,
    content: Children,
) -> Element {
    let insets = safe_area_insets();
    let radius = shape().extra_large;
    let sheet_style = computed(move || {
        column()
            .width(percent(100))
            .padding_top(px(12))
            .padding_left(px(horizontal_padding + insets.get().leading as f32))
            .padding_right(px(horizontal_padding + insets.get().trailing as f32))
            .padding_bottom(px(bottom_padding + insets.get().bottom as f32))
            .border_top_left_radius(px(radius))
            .border_top_right_radius(px(radius))
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
            View(style: css!(flex_grow: 1.0), on_tap: close) {
            }
            View(style: sheet_style, on_tap_catch: |_| {}) {
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
