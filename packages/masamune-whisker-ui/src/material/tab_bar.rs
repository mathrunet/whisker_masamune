//! Swipeable tabs inside a screen (Flutter's `TabBar` + `TabBarView` sharing a `TabController`).
//!
//! [`PageTabController`] holds the selected page and the continuous scroll position.
//! [`TabBarView`] lays the pages out in a horizontal, page-snapping scroll view, so the pages
//! follow the finger and settle one page per swipe with the platform's own scrolling (nested
//! vertical scroll views keep working). [`SegmentedTabBar`] draws pill-shaped tabs whose
//! indicator follows the swipe; tapping a tab scrolls the view to that page.
//!
//! These tabs are local to a screen and do not touch the router. For bottom-navigation tabs
//! driven by the URL use [`TabController`](crate::TabController) and
//! [`TabView`](crate::TabView).

use std::rc::Rc;

use masamune_whisker_appearance::{ColorExt, TextKind, bold, palette, text_with};
use whisker::ScrollViewHandle;
use whisker::attrs::ScrollSnapStop;
use whisker::css::{
    AlignItems, BoxShadow, Color, ColorStop, FlexDirection, FontWeight, Gradient, JustifyContent,
    LinearDirection, PositionKind, Transform, TransformFn,
};
use whisker::prelude::*;

use crate::material::ink_well::InkWell;
use crate::material::semantics;
use crate::style::{column, row};

/// Selected page and scroll position shared by a [`SegmentedTabBar`] and a [`TabBarView`]
/// (Flutter's `TabController` for in-screen tabs). Create it with [`use_page_tab_controller`].
#[derive(Clone, Copy)]
pub struct PageTabController {
    length: usize,
    index: RwSignal<usize>,
    position: RwSignal<f32>,
    target: RwSignal<Option<usize>>,
    page_size: RwSignal<(f32, f32)>,
    handle: ScrollViewHandle,
}

/// Creates a [`PageTabController`] with `length` pages, starting at `initial_index`.
pub fn use_page_tab_controller(length: usize, initial_index: usize) -> PageTabController {
    assert!(length > 0, "PageTabController needs at least one page");
    let initial_index = initial_index.min(length - 1);
    PageTabController {
        length,
        index: signal(initial_index),
        position: signal(initial_index as f32),
        target: signal(None),
        page_size: signal((0.0, 0.0)),
        handle: ScrollViewHandle::new(),
    }
}

impl PageTabController {
    /// Number of pages.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Always false: a controller has at least one page.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Index of the selected page. It changes when a swipe settles or a tab is selected.
    pub fn index(&self) -> ReadSignal<usize> {
        self.index.read_only()
    }

    /// Continuous page position (`1.5` is halfway between the second and the third page).
    /// Follows the finger while swiping.
    pub fn position(&self) -> ReadSignal<f32> {
        self.position.read_only()
    }

    /// Selects page `index` and scrolls the [`TabBarView`] to it with an animation.
    pub fn select(&self, index: usize) {
        let index = index.min(self.length - 1);
        self.index.set(index);
        let (width, _) = self.page_size.get_untracked();
        if width <= 0.0 {
            // Not laid out yet: the view scrolls to the index once it knows its width.
            self.position.set(index as f32);
            return;
        }
        if (self.position.get_untracked() - index as f32).abs() < f32::EPSILON {
            return;
        }
        self.target.set(Some(index));
        self.handle.scroll_to(f64::from(width) * index as f64, true);
    }

    fn resize(&self, width: f32, height: f32) {
        if self.page_size.get_untracked() == (width, height) {
            return;
        }
        self.page_size.set((width, height));
        if width > 0.0 {
            // Keep the selected page in view after the first layout and after a resize.
            let index = self.index.get_untracked();
            self.handle
                .scroll_to(f64::from(width) * index as f64, false);
            self.position.set(index as f32);
        }
    }

    fn scrolled(&self, detail: &whisker::event::ScrollDetail) {
        let (width, _) = self.page_size.get_untracked();
        let Some(position) = page_position(detail.scroll_left, width, self.length) else {
            return;
        };
        self.position.set(position);
        if detail.is_dragging {
            // A swipe overrides a tab tap that is still animating.
            self.target.set(None);
            return;
        }
        match self.target.get_untracked() {
            // While a tapped tab animates, keep the tapped index instead of each page passed.
            Some(target) if (position - target as f32).abs() > 0.01 => {}
            Some(_) => self.target.set(None),
            None => {
                let index = settled_index(position, self.length);
                if self.index.get_untracked() != index {
                    self.index.set(index);
                }
            }
        }
    }
}

/// Page position for a horizontal scroll offset, or `None` before the view is laid out.
fn page_position(scroll_left: f64, page_width: f32, length: usize) -> Option<f32> {
    if page_width <= 0.0 || length == 0 {
        return None;
    }
    let position = (scroll_left / f64::from(page_width)) as f32;
    Some(position.clamp(0.0, (length - 1) as f32))
}

/// Page nearest to `position`.
fn settled_index(position: f32, length: usize) -> usize {
    (position.round().max(0.0) as usize).min(length.saturating_sub(1))
}

/// One tab of a [`SegmentedTabBar`] (Flutter's `Tab`).
#[derive(Clone)]
pub struct SegmentedTab {
    pub label: Rc<dyn Fn() -> String>,
    /// Accessibility identifier for UI automation (iOS `accessibilityIdentifier`).
    pub identifier: Option<String>,
}

impl SegmentedTab {
    pub fn new(label: impl Fn() -> String + 'static) -> Self {
        Self {
            label: Rc::new(label),
            identifier: None,
        }
    }

    /// Sets the accessibility identifier for UI automation.
    pub fn identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }
}

/// Pill-shaped tab bar (Flutter's `TabBar` with a rounded gradient indicator).
///
/// The tabs share the width equally. The indicator follows the swipe of the [`TabBarView`]
/// bound to the same `controller`, and tapping a tab scrolls the view to its page. Each tab is
/// exposed as a tab labelled with its label, with its selected state and its `identifier`.
///
/// ```ignore
/// let tabs = use_page_tab_controller(2, 0);
/// render! {
///     SegmentedTabBar(
///         controller: tabs,
///         tabs: vec![SegmentedTab::new(|| "Stock".into()), SegmentedTab::new(|| "Favorite".into())],
///     )
///     TabBarView(controller: tabs) {
///         TabPage(controller: tabs) { StockList() }
///         TabPage(controller: tabs) { FavoriteList() }
///     }
/// }
/// ```
#[component]
pub fn segmented_tab_bar(
    controller: PageTabController,
    tabs: Vec<SegmentedTab>,
    #[prop(default = 48.0)] height: f32,
    /// Space between the track and the indicator.
    #[prop(default = 4.0)]
    padding: f32,
    #[prop(default = 12.0)] radius: f32,
    #[prop(default = 8.0)] indicator_radius: f32,
    #[prop(default = palette().surface)] background: Color,
    /// Start color of the indicator gradient (it ends at 80% opacity).
    #[prop(default = palette().primary)]
    indicator_color: Color,
    #[prop(default = palette().on_primary)] selected_label_color: Color,
    #[prop(default = palette().text)] unselected_label_color: Color,
    /// Draws the shadows of the track and the indicator.
    #[prop(default = true)]
    elevation: bool,
) -> Element {
    let shadow_color = palette().outline;
    let count = tabs.len().max(1) as f32;
    let track_width = signal(0.0_f32);
    let position = controller.position();
    let track_style = {
        // Fill the parent like Flutter's `width: double.infinity`; otherwise a row parent
        // (the default of a View) shrinks the track to the min-content width of the tabs.
        let style = row()
            .width(percent(100))
            .height(px(height))
            .flex_shrink(0.0)
            .padding(px(padding))
            .border_radius(px(radius))
            .background_color(background)
            .position(PositionKind::Relative);
        if elevation {
            style.box_shadows([BoxShadow::outer(
                px(0),
                px(2),
                px(8),
                px(0),
                shadow_color.with_alpha(0.1),
            )])
        } else {
            style
        }
    };
    let indicator_style = computed(move || {
        let tab_width = (track_width.get() - padding * 2.0).max(0.0) / count;
        let style = css!(
            position: PositionKind::Absolute,
            top: px(padding),
            bottom: px(padding),
            left: px(padding),
            width: px(tab_width),
            border_radius: px(indicator_radius),
        )
        .background_image(Gradient::Linear {
            direction: LinearDirection::ToRight,
            stops: vec![
                ColorStop::at(indicator_color, percent(0)),
                ColorStop::at(indicator_color.with_alpha(0.8), percent(100)),
            ],
        })
        .transform(Transform::new().push(TransformFn::TranslateX(
            px(position.get() * tab_width).into(),
        )));
        if elevation {
            style.box_shadows([BoxShadow::outer(
                px(0),
                px(2),
                px(4),
                px(0),
                indicator_color.with_alpha(0.3),
            )])
        } else {
            style
        }
    });
    let indexed: Vec<(usize, SegmentedTab)> = tabs.iter().cloned().enumerate().collect();
    let element = render! {
        View(style: track_style) {
            View(style: indicator_style) {
            }
            ForEach(
                each: move || indexed.clone(),
                key: |(index, _): &(usize, SegmentedTab)| *index,
                children: move |tab: ReadSignal<(usize, SegmentedTab)>| {
                    let (index, tab) = tab.get_untracked();
                    // Highlight the label of the page nearest to the indicator, like Flutter.
                    let is_selected =
                        computed(move || settled_index(position.get(), controller.len()) == index);
                    let accessibility = {
                        let (identifier, label) = (tab.identifier.clone(), tab.label.clone());
                        computed(move || {
                            semantics::semantics(
                                AccessibilityRole::Tab,
                                identifier.as_deref(),
                                label(),
                                AccessibilityState::new().selected(is_selected.get()),
                            )
                        })
                    };
                    let label_style = computed(move || {
                        if is_selected.get() {
                            bold(TextKind::LabelLarge, selected_label_color)
                                .font_weight(FontWeight::Numeric(600))
                        } else {
                            text_with(TextKind::LabelLarge, unselected_label_color)
                                .font_weight(FontWeight::Numeric(500))
                        }
                    });
                    render! {
                        InkWell(
                            style: css!(
                                flex_grow: 1.0,
                                flex_basis: px(0),
                                align_self: whisker::css::AlignSelf::Stretch,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                            )
                            .merge(column()),
                            accessibility: accessibility,
                            on_tap: Callback::new(move |()| controller.select(index)),
                        ) {
                            Text(value: (tab.label)(), max_lines: 1u32, style: label_style)
                        }
                    }
                },
            )
        }
    };
    whisker::runtime::view::observe_layout(
        element,
        Box::new(move |layout| {
            let width = layout.geometry.border_box.width;
            if track_width.get_untracked() != width {
                track_width.set(width);
            }
        }),
    );
    element
}

/// Underlined tab bar (Flutter's default `TabBar` with `indicatorSize: tab` and
/// `tabAlignment: fill`).
///
/// The tabs share the width equally and their labels are centered. The indicator line is drawn
/// over the divider at the bottom and follows the swipe of the [`TabBarView`] bound to the same
/// `controller`; tapping a tab scrolls the view to its page. Each tab is exposed as a tab
/// labelled with its label, with its selected state and its `identifier`.
///
/// ```ignore
/// let tabs = use_page_tab_controller(2, 0);
/// render! {
///     TabBar(
///         controller: tabs,
///         tabs: vec![SegmentedTab::new(|| "Orders".into()), SegmentedTab::new(|| "History".into())],
///     )
///     TabBarView(controller: tabs) {
///         TabPage(controller: tabs) { OrderList() }
///         TabPage(controller: tabs) { HistoryList() }
///     }
/// }
/// ```
#[component]
pub fn tab_bar(
    controller: PageTabController,
    tabs: Vec<SegmentedTab>,
    #[prop(default = 48.0)] height: f32,
    /// Thickness of the indicator line.
    #[prop(default = 2.0)]
    indicator_weight: f32,
    #[prop(default = palette().primary)] indicator_color: Color,
    #[prop(default = palette().primary)] selected_label_color: Color,
    #[prop(default = palette().sub_text)] unselected_label_color: Color,
    /// Color of the divider along the bottom of the bar.
    #[prop(default = palette().outline)]
    divider_color: Color,
    #[prop(default = 1.0)] divider_height: f32,
    #[prop(default = palette().background)] background: Color,
) -> Element {
    let count = tabs.len().max(1) as f32;
    let bar_width = signal(0.0_f32);
    let position = controller.position();
    // Fill the parent like Flutter's `TabBar`, even inside a row.
    let bar_style = row()
        .width(percent(100))
        .height(px(height))
        .flex_shrink(0.0)
        .background_color(background)
        .position(PositionKind::Relative);
    let divider_style = css!(
        position: PositionKind::Absolute,
        left: px(0),
        right: px(0),
        bottom: px(0),
        height: px(divider_height),
        background_color: divider_color,
    );
    let indicator_style = computed(move || {
        let tab_width = bar_width.get().max(0.0) / count;
        css!(
            position: PositionKind::Absolute,
            left: px(0),
            bottom: px(0),
            width: px(tab_width),
            height: px(indicator_weight),
            background_color: indicator_color,
        )
        .transform(Transform::new().push(TransformFn::TranslateX(
            px(position.get() * tab_width).into(),
        )))
    });
    let indexed: Vec<(usize, SegmentedTab)> = tabs.iter().cloned().enumerate().collect();
    let element = render! {
        View(style: bar_style) {
            // Drawn first so the indicator paints over it.
            View(style: divider_style) {
            }
            ForEach(
                each: move || indexed.clone(),
                key: |(index, _): &(usize, SegmentedTab)| *index,
                children: move |tab: ReadSignal<(usize, SegmentedTab)>| {
                    let (index, tab) = tab.get_untracked();
                    let is_selected =
                        computed(move || settled_index(position.get(), controller.len()) == index);
                    let accessibility = {
                        let (identifier, label) = (tab.identifier.clone(), tab.label.clone());
                        computed(move || {
                            semantics::semantics(
                                AccessibilityRole::Tab,
                                identifier.as_deref(),
                                label(),
                                AccessibilityState::new().selected(is_selected.get()),
                            )
                        })
                    };
                    let label_style = computed(move || {
                        let color = if is_selected.get() {
                            selected_label_color
                        } else {
                            unselected_label_color
                        };
                        bold(TextKind::TitleSmall, color)
                    });
                    render! {
                        InkWell(
                            style: css!(
                                flex_grow: 1.0,
                                flex_basis: px(0),
                                align_self: whisker::css::AlignSelf::Stretch,
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                padding_left: px(8),
                                padding_right: px(8),
                            )
                            .merge(column()),
                            accessibility: accessibility,
                            on_tap: Callback::new(move |()| controller.select(index)),
                        ) {
                            Text(value: (tab.label)(), max_lines: 1u32, style: label_style)
                        }
                    }
                },
            )
            View(style: indicator_style) {
            }
        }
    };
    whisker::runtime::view::observe_layout(
        element,
        Box::new(move |layout| {
            let width = layout.geometry.border_box.width;
            if bar_width.get_untracked() != width {
                bar_width.set(width);
            }
        }),
    );
    element
}

/// Horizontally swipeable pages (Flutter's `TabBarView`). Put one [`TabPage`] per tab inside,
/// in the same order as the tabs of the [`TabBar`] or [`SegmentedTabBar`] bound to `controller`.
///
/// It fills the remaining space of a column (`flex_grow: 1`). Swiping follows the finger and
/// settles on the nearest page, one page per swipe. The pages stay mounted, so their state and
/// scroll positions survive switching tabs.
#[component]
pub fn tab_bar_view(controller: PageTabController, children: Children) -> Element {
    let projected = children.clone();
    let element = render! {
        ScrollView(
            element_ref: controller.handle.r(),
            axis: ScrollAxis::Horizontal,
            snap: ScrollSnap::start(),
            scroll_snap_stop: ScrollSnapStop::Always,
            // A ScrollView lays its children out as a column by default, even on the
            // horizontal axis; the pages must sit side by side to be scrollable.
            style: css!(flex_grow: 1.0, flex_basis: px(0), flex_direction: FlexDirection::Row),
            on_scroll: move |event: whisker::event::ScrollEvent| controller.scrolled(&event.detail),
        ) {
            {projected()}
        }
    };
    whisker::runtime::view::observe_layout(
        element,
        Box::new(move |layout| {
            let rect = layout.geometry.border_box;
            controller.resize(rect.width, rect.height);
        }),
    );
    element
}

/// One page of a [`TabBarView`]: sized to the view and laid out as a column. Put a vertical
/// `ScrollView` inside when the content is taller than the page.
#[component]
pub fn tab_page(controller: PageTabController, children: Children) -> Element {
    let page_size = controller.page_size;
    let style = computed(move || {
        let (width, height) = page_size.get();
        column()
            .width(px(width))
            .height(px(height))
            .flex_shrink(0.0)
    });
    let projected = children.clone();
    render! {
        View(style: style) {
            {projected()}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker::runtime::reactive::Owner;

    fn with_runtime<F: FnOnce() -> T, T>(f: F) -> T {
        let runtime =
            whisker::runtime::RuntimeContext::new(whisker::runtime::RuntimeWakeHandle::new(|| {}));
        runtime.enter(|| {
            let owner = Owner::new(None);
            let out = owner.with(f);
            owner.dispose();
            out
        })
    }

    fn detail(scroll_left: f64, is_dragging: bool) -> whisker::event::ScrollDetail {
        let mut detail = whisker::event::ScrollDetail::default();
        detail.scroll_left = scroll_left;
        detail.is_dragging = is_dragging;
        detail
    }

    #[test]
    fn page_position_follows_the_offset() {
        assert_eq!(page_position(0.0, 0.0, 3), None);
        assert_eq!(page_position(150.0, 300.0, 3), Some(0.5));
        assert_eq!(page_position(-40.0, 300.0, 3), Some(0.0));
        assert_eq!(page_position(900.0, 300.0, 3), Some(2.0));
    }

    #[test]
    fn settled_index_rounds_to_the_nearest_page() {
        assert_eq!(settled_index(0.4, 3), 0);
        assert_eq!(settled_index(0.6, 3), 1);
        assert_eq!(settled_index(5.0, 3), 2);
    }

    #[test]
    fn initial_index_is_clamped() {
        with_runtime(|| {
            let controller = use_page_tab_controller(3, 7);
            assert_eq!(controller.index().get_untracked(), 2);
            assert_eq!(controller.position().get_untracked(), 2.0);
        });
    }

    #[test]
    fn swipe_moves_the_position_and_settles_the_index() {
        with_runtime(|| {
            let controller = use_page_tab_controller(3, 0);
            controller.resize(300.0, 600.0);
            controller.scrolled(&detail(210.0, true));
            assert_eq!(controller.position().get_untracked(), 0.7);
            assert_eq!(controller.index().get_untracked(), 0);
            controller.scrolled(&detail(300.0, false));
            assert_eq!(controller.index().get_untracked(), 1);
        });
    }

    #[test]
    fn tapped_tab_keeps_its_index_while_animating() {
        with_runtime(|| {
            let controller = use_page_tab_controller(3, 0);
            controller.resize(300.0, 600.0);
            controller.select(2);
            assert_eq!(controller.index().get_untracked(), 2);
            controller.scrolled(&detail(300.0, false));
            assert_eq!(controller.index().get_untracked(), 2);
            controller.scrolled(&detail(600.0, false));
            assert_eq!(controller.index().get_untracked(), 2);
            assert_eq!(controller.target.get_untracked(), None);
        });
    }

    #[test]
    fn select_before_layout_only_moves_the_position() {
        with_runtime(|| {
            let controller = use_page_tab_controller(3, 0);
            controller.select(1);
            assert_eq!(controller.index().get_untracked(), 1);
            assert_eq!(controller.position().get_untracked(), 1.0);
            assert_eq!(controller.target.get_untracked(), None);
        });
    }
}
