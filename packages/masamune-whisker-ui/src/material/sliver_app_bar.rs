//! Collapsing app bars over a scroll view (Flutter's `SliverAppBar` with a `FlexibleSpaceBar`,
//! and masamune's `UniversalSliverAppBar` / `UniversalAvatarSliverAppBar`).
//!
//! Whisker has no slivers or `position: sticky`, so the bar is drawn over a
//! [`RefreshScrollView`] and laid out from its scroll offset ([`SliverAppBarLayout`]):
//!
//! - The background stays almost still (it moves up a quarter of the scroll, Flutter's
//!   `CollapseMode.parallax`) while the content scrolls over it, and fades into `bar_color`
//!   during the last [`APP_BAR_HEIGHT`] before the bar is collapsed.
//! - Collapsed, a bar of [`APP_BAR_HEIGHT`] plus the top safe area stays pinned with the
//!   leading button and the actions.
//! - [`AvatarSliverAppBarScrollView`] also has a band below the background holding an avatar
//!   (left) and actions (right). The avatar shrinks while scrolling and moves behind the bar
//!   once it is half its size, as in `UniversalAvatarSliverAppBar`.
//!
//! The background, avatar and bar ignore touches, so dragging on them scrolls the content.

use whisker::css::{Color, Overflow, PointerEvents, PositionKind, Transform, TransformFn};
use whisker::event::ScrollEvent;
use whisker::prelude::*;
use whisker_safe_area::safe_area_insets;

use masamune_whisker_appearance::{palette, shape};

use crate::material::app_bar::APP_BAR_HEIGHT;
use crate::material::refresh::{RefreshCallback, RefreshScrollView};
use crate::material::scaffold::Slot;
use crate::style::{fill, row};

/// Default height of the expanded background, without the top safe area
/// (masamune's `kDefaultExpandedHeight`).
pub const DEFAULT_EXPANDED_HEIGHT: f32 = 240.0;

/// Default height of the band below the background of [`AvatarSliverAppBarScrollView`]
/// (masamune's `kDefaultUnderBottomHeight`).
pub const DEFAULT_UNDER_BOTTOM_HEIGHT: f32 = 40.0;

/// Space added below the band (masamune's `_kAvatarAppBarBottomSpace`).
const AVATAR_BOTTOM_SPACE: f32 = 8.0;

/// Distance from the start edge to the avatar and from the end edge to the band actions.
const AVATAR_LEFT: f32 = 24.0;
const UNDER_BOTTOM_ACTIONS_RIGHT: f32 = 12.0;

/// Horizontal padding of the leading button and the actions in the bar.
const TOOLBAR_PADDING: f32 = 8.0;

/// Positions of a collapsing app bar for one scroll offset. Every value is in pixels from the
/// top of the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliverAppBarLayout {
    /// Moves the whole bar down while the content is pulled past its top (overscroll), so the
    /// bar follows the content.
    pub overlay_offset: f32,
    /// Height of the bar holding the background (clipped). Never below the collapsed height.
    pub bar_height: f32,
    /// Bottom of the bar including the band below it. Never below the collapsed height.
    pub extent: f32,
    /// Vertical offset of the background (zero or negative, the parallax).
    pub background_offset: f32,
    /// Height the background is laid out at (the fully expanded extent).
    pub background_height: f32,
    /// Opacity of the background.
    pub background_opacity: f32,
    /// Diameter of the avatar including its border.
    pub avatar_size: f32,
    /// Top of the avatar (its bottom sits on [`extent`](Self::extent)).
    pub avatar_top: f32,
    /// The avatar and the band actions are drawn in front of the bar.
    pub avatar_front: bool,
}

impl SliverAppBarLayout {
    /// Computes the layout at `scroll_top` (the content's scroll offset).
    ///
    /// `expanded_height` is the background height without `top_inset` (the top safe area), and
    /// `under_bottom_height` the band below it (`0` for no band).
    pub fn compute(
        scroll_top: f32,
        expanded_height: f32,
        top_inset: f32,
        under_bottom_height: f32,
    ) -> Self {
        let band = if under_bottom_height > 0.0 {
            under_bottom_height + AVATAR_BOTTOM_SPACE
        } else {
            0.0
        };
        let expanded = expanded_height + top_inset;
        let min_extent = APP_BAR_HEIGHT + top_inset;
        let max_extent = expanded + band;
        let delta = (max_extent - min_extent).max(0.0);
        let offset = scroll_top.max(0.0);
        // FlexibleSpaceBar: parallax `-t * delta / 4`, fading linearly over the last
        // toolbar height (or the whole range when it is shorter).
        let background_offset = -offset.min(delta) / 4.0;
        let fade = APP_BAR_HEIGHT.min(delta);
        let background_opacity = if fade <= 0.0 {
            1.0
        } else {
            ((delta - offset) / fade).clamp(0.0, 1.0)
        };
        // UniversalAvatarSliverAppBar: `percent = 2 - expanded / (expanded - offset)`, `0` out of
        // `0..=1`; the diameter is `band * 2 * percent`, at least `under_bottom_height`.
        let visible = expanded - offset;
        let proportion = if visible > 0.0 {
            2.0 - expanded / visible
        } else {
            0.0
        };
        let percent = if (0.0..=1.0).contains(&proportion) {
            proportion
        } else {
            0.0
        };
        let avatar_size = (band * 2.0 * percent).max(under_bottom_height);
        let extent = (max_extent - offset).max(min_extent);
        Self {
            overlay_offset: (-scroll_top).max(0.0),
            bar_height: (expanded - offset).max(min_extent),
            extent,
            background_offset,
            background_height: max_extent,
            background_opacity,
            avatar_size,
            avatar_top: extent - avatar_size,
            avatar_front: percent >= 0.5,
        }
    }
}

/// Props shared by both scroll views.
struct SliverProps {
    expanded_height: f32,
    under_bottom_height: f32,
    bar_color: Color,
    bottom_border_color: Option<Color>,
    background: Slot,
    leading: Slot,
    actions: Slot,
    avatar: Slot,
    avatar_border_width: f32,
    avatar_border_color: Color,
    under_bottom_actions: Slot,
    on_refresh: Option<RefreshCallback>,
    style: Signal<Css>,
    content_style: Signal<Css>,
    children: Children,
}

/// Vertical scroll view under a collapsing app bar with a background (Flutter's
/// `NestedScrollView` + `SliverAppBar(flexibleSpace: FlexibleSpaceBar(background: …))`,
/// masamune's `UniversalSliverAppBar`).
///
/// Put the page content in `children`; it starts below the expanded background. Size the view
/// with `style` (for example `flex_grow: 1.0, flex_basis: px(0)`). The background extends under
/// the status bar, so use it in a `Scaffold` with `safe_area_top: false`.
///
/// ```ignore
/// render! {
///     SliverAppBarScrollView(
///         style: css!(flex_grow: 1.0, flex_basis: px(0)),
///         background: move || render! { Image(src: url, style: fill()) },
///         actions: move || render! { NotificationButton() },
///         on_refresh: move || async move { reload().await },
///     ) {
///         /* rows */
///     }
/// }
/// ```
#[component]
pub fn sliver_app_bar_scroll_view(
    /// Height of the expanded background, without the top safe area.
    #[prop(default = DEFAULT_EXPANDED_HEIGHT)]
    expanded_height: f32,
    /// Color of the bar, shown behind the background and once it has faded out.
    #[prop(default = palette().bar)]
    bar_color: Color,
    /// Draws a 1px line of this color below the bar.
    bottom_border_color: Option<Color>,
    /// Background filling the bar (an image, usually with a dark filter over it).
    #[prop(default = Slot::default())]
    background: Slot,
    /// Leading button of the bar (a close or back button).
    #[prop(default = Slot::default())]
    leading: Slot,
    /// Actions on the end side of the bar.
    #[prop(default = Slot::default())]
    actions: Slot,
    /// Called when the user pulls to refresh. `None` disables pull to refresh.
    on_refresh: Option<RefreshCallback>,
    /// Style of the view (size it here).
    #[prop(default = Css::new().into())]
    style: Signal<Css>,
    /// Style of the column holding the children.
    #[prop(default = Css::new().into())]
    content_style: Signal<Css>,
    children: Children,
) -> Element {
    build(SliverProps {
        expanded_height,
        under_bottom_height: 0.0,
        bar_color,
        bottom_border_color,
        background: background.clone(),
        leading: leading.clone(),
        actions: actions.clone(),
        avatar: Slot::default(),
        avatar_border_width: 0.0,
        avatar_border_color: palette().background,
        under_bottom_actions: Slot::default(),
        on_refresh: on_refresh.clone(),
        style,
        content_style,
        children: children.clone(),
    })
}

/// [`SliverAppBarScrollView`] with a band below the background holding an avatar and actions
/// (masamune's `UniversalAvatarSliverAppBar`).
///
/// The avatar is a circle of twice the band height (`under_bottom_height` + 8) straddling the
/// bottom of the background. While scrolling it shrinks down to `under_bottom_height` and moves
/// behind the bar once it is half its size; the band actions follow it.
#[component]
pub fn avatar_sliver_app_bar_scroll_view(
    /// Height of the expanded background, without the top safe area.
    #[prop(default = DEFAULT_EXPANDED_HEIGHT)]
    expanded_height: f32,
    /// Height of the band below the background (`underBottomHeight`).
    #[prop(default = DEFAULT_UNDER_BOTTOM_HEIGHT)]
    under_bottom_height: f32,
    /// Color of the bar, shown behind the background and once it has faded out.
    #[prop(default = palette().bar)]
    bar_color: Color,
    /// Draws a 1px line of this color below the bar.
    bottom_border_color: Option<Color>,
    /// Background filling the bar (an image, usually with a dark filter over it).
    #[prop(default = Slot::default())]
    background: Slot,
    /// Leading button of the bar (a close or back button).
    #[prop(default = Slot::default())]
    leading: Slot,
    /// Actions on the end side of the bar.
    #[prop(default = Slot::default())]
    actions: Slot,
    /// Avatar image, clipped to a circle. Empty for no avatar.
    #[prop(default = Slot::default())]
    avatar: Slot,
    /// Width of the ring around the avatar.
    #[prop(default = 4.0)]
    avatar_border_width: f32,
    /// Color of the ring around the avatar (the scaffold background by default).
    #[prop(default = palette().background)]
    avatar_border_color: Color,
    /// Actions on the end side of the band (`underBottomActions`).
    #[prop(default = Slot::default())]
    under_bottom_actions: Slot,
    /// Called when the user pulls to refresh. `None` disables pull to refresh.
    on_refresh: Option<RefreshCallback>,
    /// Style of the view (size it here).
    #[prop(default = Css::new().into())]
    style: Signal<Css>,
    /// Style of the column holding the children.
    #[prop(default = Css::new().into())]
    content_style: Signal<Css>,
    children: Children,
) -> Element {
    build(SliverProps {
        expanded_height,
        under_bottom_height,
        bar_color,
        bottom_border_color,
        background: background.clone(),
        leading: leading.clone(),
        actions: actions.clone(),
        avatar: avatar.clone(),
        avatar_border_width,
        avatar_border_color,
        under_bottom_actions: under_bottom_actions.clone(),
        on_refresh: on_refresh.clone(),
        style,
        content_style,
        children: children.clone(),
    })
}

fn build(props: SliverProps) -> Element {
    let SliverProps {
        expanded_height,
        under_bottom_height,
        bar_color,
        bottom_border_color,
        background,
        leading,
        actions,
        avatar,
        avatar_border_width,
        avatar_border_color,
        under_bottom_actions,
        on_refresh,
        style,
        content_style,
        children,
    } = props;
    let insets = safe_area_insets();
    let scroll_top = RwSignal::new(0.0_f32);
    let on_scroll = Callback::new(move |event: ScrollEvent| {
        let _ = scroll_top.try_set(event.detail.scroll_top as f32);
    });
    let layout = computed(move || {
        SliverAppBarLayout::compute(
            scroll_top.get(),
            expanded_height,
            insets.get().top as f32,
            under_bottom_height,
        )
    });
    let root_style = computed(move || {
        css!(position: PositionKind::Relative, overflow: Overflow::Hidden).merge(style.get())
    });
    let scroll_style = computed(move || fill().z_index(0));
    // The content starts below the expanded background and its band; the transparent space
    // shows the bar (drawn over it) and the scaffold background under the band.
    let content_style = computed(move || {
        let top = SliverAppBarLayout::compute(
            0.0,
            expanded_height,
            insets.get().top as f32,
            under_bottom_height,
        )
        .background_height;
        Css::new().padding_top(px(top)).merge(content_style.get())
    });
    let bar_style = computed(move || {
        let layout = layout.get();
        let style = css!(
            position: PositionKind::Absolute,
            top: px(layout.overlay_offset),
            left: px(0),
            right: px(0),
            height: px(layout.bar_height),
            overflow: Overflow::Hidden,
            background_color: bar_color,
        )
        .z_index(2)
        .pointer_events(PointerEvents::None);
        match bottom_border_color {
            Some(color) => style.border_bottom(Border::new().width(px(1)).solid().color(color)),
            None => style,
        }
    });
    let background_style = computed(move || {
        let layout = layout.get();
        css!(
            position: PositionKind::Absolute,
            top: px(0),
            left: px(0),
            right: px(0),
            height: px(layout.background_height),
            opacity: layout.background_opacity,
        )
        .transform(
            Transform::new().push(TransformFn::TranslateY(px(layout.background_offset).into())),
        )
    });
    let avatar_style = computed(move || {
        let layout = layout.get();
        css!(
            position: PositionKind::Absolute,
            left: px(AVATAR_LEFT),
            top: px(layout.avatar_top + layout.overlay_offset),
            width: px(layout.avatar_size),
            height: px(layout.avatar_size),
            padding: px(avatar_border_width),
            border_radius: px(shape().full),
            overflow: Overflow::Hidden,
            background_color: avatar_border_color,
        )
        .z_index(if layout.avatar_front { 3 } else { 1 })
        .pointer_events(PointerEvents::None)
    });
    let avatar_inner_style = css!(
        width: percent(100),
        height: percent(100),
        border_radius: px(shape().full),
        overflow: Overflow::Hidden,
    );
    let under_bottom_style = computed(move || {
        let layout = layout.get();
        row()
            .position(PositionKind::Absolute)
            .right(px(UNDER_BOTTOM_ACTIONS_RIGHT))
            .top(px(
                layout.extent - under_bottom_height + layout.overlay_offset
            ))
            .height(px(under_bottom_height))
            .z_index(if layout.avatar_front { 3 } else { 1 })
    });
    let toolbar_style = move |left: bool| {
        computed(move || {
            let style = row()
                .position(PositionKind::Absolute)
                .top(px(insets.get().top as f32 + layout.get().overlay_offset))
                .height(px(APP_BAR_HEIGHT))
                .z_index(4);
            if left {
                style
                    .left(px(insets.get().leading as f32))
                    .padding_left(px(TOOLBAR_PADDING))
            } else {
                style
                    .right(px(insets.get().trailing as f32))
                    .padding_right(px(TOOLBAR_PADDING))
            }
        })
    };
    let leading_style = toolbar_style(true);
    let actions_style = toolbar_style(false);
    let has_under_bottom_actions = under_bottom_actions.is_some() && under_bottom_height > 0.0;
    let projected = children.clone();
    render! {
        View(style: root_style) {
            RefreshScrollView(
                style: scroll_style,
                content_style: content_style,
                on_refresh_option: on_refresh,
                on_scroll: on_scroll,
            ) {
                {projected()}
            }
            View(style: bar_style) {
                {background.is_some().then(|| render! {
                    View(style: background_style) {
                        {background.render()}
                    }
                })}
            }
            {avatar.is_some().then(|| render! {
                View(style: avatar_style) {
                    View(style: avatar_inner_style.clone()) {
                        {avatar.render()}
                    }
                }
            })}
            {has_under_bottom_actions.then(|| render! {
                View(style: under_bottom_style) {
                    {under_bottom_actions.render()}
                }
            })}
            {leading.is_some().then(|| render! {
                View(style: leading_style) {
                    {leading.render()}
                }
            })}
            {actions.is_some().then(|| render! {
                View(style: actions_style) {
                    {actions.render()}
                }
            })}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPANDED: f32 = 240.0;
    const INSET: f32 = 0.0;
    const BAND: f32 = DEFAULT_UNDER_BOTTOM_HEIGHT;

    fn at(scroll_top: f32) -> SliverAppBarLayout {
        SliverAppBarLayout::compute(scroll_top, EXPANDED, INSET, BAND)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn expanded_at_top() {
        let layout = at(0.0);
        assert_eq!(layout.bar_height, 240.0);
        assert_eq!(layout.extent, 288.0);
        assert_eq!(layout.background_height, 288.0);
        assert_eq!(layout.background_offset, 0.0);
        assert_eq!(layout.background_opacity, 1.0);
        assert_eq!(layout.avatar_size, 96.0);
        assert_eq!(layout.avatar_top, 192.0);
        assert!(layout.avatar_front);
        assert_eq!(layout.overlay_offset, 0.0);
    }

    #[test]
    fn avatar_shrinks_like_flutter() {
        assert!(close(at(40.0).avatar_size, 76.8));
        let half = at(80.0);
        assert!(close(half.avatar_size, 48.0));
        assert!(half.avatar_front);
        let behind = at(81.0);
        assert!(!behind.avatar_front);
        assert_eq!(at(120.0).avatar_size, BAND);
        assert_eq!(at(1000.0).avatar_size, BAND);
    }

    #[test]
    fn background_parallax_and_fade() {
        // delta = 288 - 56 = 232.
        let layout = at(100.0);
        assert_eq!(layout.background_offset, -25.0);
        assert_eq!(layout.background_opacity, 1.0);
        assert_eq!(layout.bar_height, 140.0);
        assert_eq!(layout.extent, 188.0);
        assert!(close(at(232.0 - 28.0).background_opacity, 0.5));
        let collapsed = at(232.0);
        assert_eq!(collapsed.background_opacity, 0.0);
        assert_eq!(collapsed.background_offset, -58.0);
        assert_eq!(collapsed.bar_height, APP_BAR_HEIGHT);
        assert_eq!(collapsed.extent, APP_BAR_HEIGHT);
        let further = at(500.0);
        assert_eq!(further.background_offset, -58.0);
        assert_eq!(further.bar_height, APP_BAR_HEIGHT);
    }

    #[test]
    fn follows_overscroll() {
        let layout = at(-30.0);
        assert_eq!(layout.overlay_offset, 30.0);
        assert_eq!(layout.bar_height, 240.0);
        assert_eq!(layout.avatar_size, 96.0);
    }

    #[test]
    fn safe_area_extends_the_bar() {
        let layout = SliverAppBarLayout::compute(0.0, EXPANDED, 40.0, BAND);
        assert_eq!(layout.bar_height, 280.0);
        assert_eq!(layout.extent, 328.0);
        let collapsed = SliverAppBarLayout::compute(1000.0, EXPANDED, 40.0, BAND);
        assert_eq!(collapsed.bar_height, APP_BAR_HEIGHT + 40.0);
    }

    #[test]
    fn without_band() {
        let layout = SliverAppBarLayout::compute(0.0, EXPANDED, 0.0, 0.0);
        assert_eq!(layout.extent, 240.0);
        assert_eq!(layout.background_height, 240.0);
        let collapsed = SliverAppBarLayout::compute(184.0, EXPANDED, 0.0, 0.0);
        assert_eq!(collapsed.background_opacity, 0.0);
        assert_eq!(collapsed.bar_height, APP_BAR_HEIGHT);
    }
}
