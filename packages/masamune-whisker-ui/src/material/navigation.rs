//! Bottom navigation (Flutter's `BottomNavigationBar`) and router-driven tabs (Flutter's
//! `TabController` and `TabBarView`).

use std::cell::Cell;
use std::rc::Rc;

use masamune_whisker_appearance::{TextKind, palette, text_with};
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::css::{AlignItems, Color, Display, FlexDirection, JustifyContent};
use whisker::prelude::*;
use whisker_icons::Icon;
use whisker_router::{RouterHandle, use_navigator, use_pathname};

use crate::material::bottom_sheet::{ModalLayer, provide_modal_layer};
use crate::material::ink_well::InkWell;
use crate::material::semantics;
use crate::style::{css_color, row};

/// One destination of a [`BottomNavigationBar`].
#[derive(Clone)]
pub struct BottomNavigationBarItem {
    /// SVG of the icon (for example a `whisker_icons::lucide` constant).
    pub icon: &'static str,
    pub label: Rc<dyn Fn() -> String>,
    /// Accessibility identifier for UI automation (iOS `accessibilityIdentifier`).
    pub identifier: Option<String>,
}

impl BottomNavigationBarItem {
    pub fn new(icon: &'static str, label: impl Fn() -> String + 'static) -> Self {
        Self {
            icon,
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

/// Row of icon + label destinations. The safe area below it is reserved by [`Scaffold`] and
/// painted with its `bottom_navigation_bar_color` (pass the same color as `background`).
///
/// Each destination is exposed as a tab labelled with its label, with its selected state and
/// the item's `identifier`.
#[component]
pub fn bottom_navigation_bar(
    items: Vec<BottomNavigationBarItem>,
    /// Index of the selected item (`None` when no item is selected).
    selected: ReadSignal<Option<usize>>,
    on_tap: Callback<usize>,
    #[prop(default = palette().bar)] background: Color,
    #[prop(default = palette().primary)] selected_color: Color,
    #[prop(default = palette().outline)] unselected_icon_color: Color,
    #[prop(default = palette().sub_text)] unselected_label_color: Color,
) -> Element {
    let bar_style = row()
        .justify_content(JustifyContent::SpaceAround)
        .background_color(background)
        .min_height(px(56));
    let indexed: Vec<(usize, BottomNavigationBarItem)> =
        items.iter().cloned().enumerate().collect();
    render! {
        View(style: bar_style) {
            ForEach(
                each: move || indexed.clone(),
                key: |(index, _): &(usize, BottomNavigationBarItem)| *index,
                children: move |item: ReadSignal<(usize, BottomNavigationBarItem)>| {
                    let (index, item) = item.get_untracked();
                    let is_selected = computed(move || selected.get() == Some(index));
                    let accessibility = {
                        let (identifier, label) = (item.identifier.clone(), item.label.clone());
                        computed(move || {
                            semantics::semantics(
                                AccessibilityRole::Tab,
                                identifier.as_deref(),
                                label(),
                                AccessibilityState::new().selected(is_selected.get()),
                            )
                        })
                    };
                    let (on, off) = (css_color(selected_color), css_color(unselected_icon_color));
                    let icon_color = computed(move || if is_selected.get() { on.clone() } else { off.clone() });
                    let label_style = computed(move || {
                        text_with(
                            TextKind::LabelSmall,
                            if is_selected.get() { selected_color } else { unselected_label_color },
                        )
                    });
                    render! {
                        InkWell(
                            style: css!(flex_grow: 1.0, padding: (px(8), px(0)), align_items: AlignItems::Center)
                                .display(Display::Flex)
                                .flex_direction(FlexDirection::Column)
                                .gap(px(2)),
                            accessibility: accessibility,
                            on_tap: Callback::new(move |()| on_tap.run(index)),
                        ) {
                            Icon(svg: item.icon, color: icon_color, size: "24")
                            Text(value: (item.label)(), style: label_style)
                        }
                    }
                },
            )
        }
    }
}

/// A tab of a [`TabController`], bound to a router group such as `"/(home)"` and the URL of
/// its root screen such as `"/home"`.
#[derive(Clone)]
pub struct TabItem {
    /// Router group the tab stands for (`"/(name)"`).
    pub group: &'static str,
    /// URL of the root screen of the tab. Selecting the tab shows this screen.
    pub root: &'static str,
    pub icon: &'static str,
    pub label: Rc<dyn Fn() -> String>,
    /// Whether a pathname belongs to this tab.
    pub matches: Rc<dyn Fn(&str) -> bool>,
    /// Accessibility identifier of the tab for UI automation (iOS `accessibilityIdentifier`).
    pub identifier: Option<String>,
}

impl TabItem {
    /// Tab for `group` (`"/(name)"`) whose root screen is `root`. Paths containing `/name`
    /// belong to it.
    pub fn new(
        group: &'static str,
        root: &'static str,
        icon: &'static str,
        label: impl Fn() -> String + 'static,
    ) -> Self {
        let segment = format!(
            "/{}",
            group
                .trim_start_matches('/')
                .trim_start_matches('(')
                .trim_end_matches(')')
        );
        Self {
            group,
            root,
            icon,
            label: Rc::new(label),
            matches: Rc::new(move |path: &str| path.contains(&segment)),
            identifier: None,
        }
    }

    /// Sets the accessibility identifier of the tab for UI automation.
    pub fn identifier(mut self, identifier: impl Into<String>) -> Self {
        self.identifier = Some(identifier.into());
        self
    }

    /// Replaces how paths are matched to this tab. The tab's `root` should match too.
    pub fn matching(mut self, matches: impl Fn(&str) -> bool + 'static) -> Self {
        self.matches = Rc::new(matches);
        self
    }
}

/// Index of the tab `path` belongs to (`None` for screens outside the tabs).
pub fn tab_of(tabs: &[TabItem], path: &str) -> Option<usize> {
    tabs.iter().position(|tab| (tab.matches)(path))
}

/// URL to navigate to when the tab at `index` is selected.
///
/// With `reset_on_tab_change` it is the tab's root, so the tab opens on its root screen whether
/// it is switched to or tapped again. Without it, it is the tab's group, which brings back the
/// screens left open in the tab (tapping the current tab again still pops it to its root).
pub fn tab_destination(
    tabs: &[TabItem],
    index: usize,
    reset_on_tab_change: bool,
) -> Option<&'static str> {
    let tab = tabs.get(index)?;
    Some(if reset_on_tab_change {
        tab.root
    } else {
        tab.group
    })
}

/// Default fade of the body when switching tabs (ms).
pub const TAB_FADE_MS: u32 = 220;

/// Options of [`use_tab_controller_with`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabOptions {
    /// Fade of the body when the tab changes (ms).
    pub fade_ms: u32,
    /// Shows the root screen of a tab whenever it is selected, dropping the screens left open
    /// in it. `false` keeps them while switching tabs.
    pub reset_on_tab_change: bool,
}

impl Default for TabOptions {
    fn default() -> Self {
        Self {
            fade_ms: TAB_FADE_MS,
            reset_on_tab_change: true,
        }
    }
}

/// Router-driven tab state shared by a [`BottomNavigationBar`] and a [`TabView`]
/// (Flutter's `TabController`).
///
/// Create it with [`use_tab_controller`] in the layout component of a route that wraps a `Switch`
/// of tab groups. The selected tab follows the current pathname. When the tab changes, the
/// `ModalBottomSheet`s of the tab screens close and the [`TabView`] fades in; coming back from a
/// screen outside the tabs (pushed on the outer stack) does not count as a switch.
#[derive(Clone, Copy)]
pub struct TabController {
    tabs: StoredValue<Vec<TabItem>>,
    nav: StoredValue<RouterHandle>,
    selected: ReadSignal<Option<usize>>,
    fade_progress: ReadSignal<f32>,
    layer: ModalLayer,
    reset_on_tab_change: bool,
}

impl TabController {
    /// Number of tabs.
    pub fn len(&self) -> usize {
        self.tabs.with(Vec::len)
    }

    /// Whether there are no tabs.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Destinations for [`BottomNavigationBar`]'s `items`.
    pub fn items(&self) -> Vec<BottomNavigationBarItem> {
        self.tabs.with(|tabs| {
            tabs.iter()
                .map(|tab| BottomNavigationBarItem {
                    icon: tab.icon,
                    label: tab.label.clone(),
                    identifier: tab.identifier.clone(),
                })
                .collect()
        })
    }

    /// Index of the selected tab for [`BottomNavigationBar`]'s `selected`.
    pub fn selected(&self) -> ReadSignal<Option<usize>> {
        self.selected
    }

    /// Tap handler for [`BottomNavigationBar`]'s `on_tap`.
    pub fn on_tap(&self) -> Callback<usize> {
        let controller = *self;
        Callback::new(move |index: usize| controller.select(index))
    }

    /// Selects the tab at `index` (see [`TabOptions::reset_on_tab_change`]).
    pub fn select(&self, index: usize) {
        let destination = self
            .tabs
            .with(|tabs| tab_destination(tabs, index, self.reset_on_tab_change));
        if let Some(destination) = destination {
            let _ = self.nav.with(|nav| nav.navigate(destination));
        }
    }

    /// Layer the sheets of the tab screens are drawn in. Pass it to a
    /// [`ModalLayerHost`](crate::ModalLayerHost) in the `overlay` of the tabs'
    /// [`Scaffold`](crate::Scaffold) so the sheets cover the bar.
    pub fn modal_layer(&self) -> ModalLayer {
        self.layer
    }

    /// Opacity of the body (`0.0` to `1.0`), animated when the tab changes.
    pub fn fade_progress(&self) -> ReadSignal<f32> {
        self.fade_progress
    }
}

/// Creates a [`TabController`] for `tabs` with the default [`TabOptions`].
pub fn use_tab_controller(tabs: Vec<TabItem>) -> TabController {
    use_tab_controller_with(tabs, TabOptions::default())
}

/// Creates a [`TabController`] for `tabs` with `options`.
pub fn use_tab_controller_with(tabs: Vec<TabItem>, options: TabOptions) -> TabController {
    debug_assert!(
        tabs.iter().all(|tab| (tab.matches)(tab.root)),
        "TabItem.root must belong to its own tab"
    );
    let nav = use_navigator();
    let pathname = use_pathname();
    let layer = provide_modal_layer();
    let fade = AnimationController::new(AnimConfig::ease_out(options.fade_ms));
    fade.set_value(1.0);
    let fade_progress = fade.value();
    let current_tab = Rc::new(Cell::new(None::<usize>));
    {
        let fade = fade.clone();
        let tabs = tabs.clone();
        effect(move || {
            let Some(tab) = tab_of(&tabs, &pathname.get()) else {
                return;
            };
            let previous = current_tab.replace(Some(tab));
            if previous.is_some_and(|previous| previous != tab) {
                layer.dismiss_all();
                fade.set_value(0.0);
                fade.forward();
            }
        });
    }
    let _keep = StoredValue::new(fade);
    let selected = {
        let tabs = tabs.clone();
        computed(move || tab_of(&tabs, &pathname.get()))
    };
    TabController {
        tabs: StoredValue::new(tabs),
        nav: StoredValue::new(nav),
        selected,
        fade_progress,
        layer,
        reset_on_tab_change: options.reset_on_tab_change,
    }
}

/// Routed body of tabs (Flutter's `TabBarView`): fades its children (usually `Outlet {}`) in
/// when the tab of `controller` changes.
///
/// ```ignore
/// let tabs = use_tab_controller(vec![
///     TabItem::new("/(home)", "/home", lucide::House, || "Home".into()),
///     TabItem::new("/(mypage)", "/mypage", lucide::User, || "My page".into()),
/// ]);
/// render! {
///     Scaffold(
///         safe_area_top: false,
///         safe_area_horizontal: false,
///         bottom_navigation_bar: move || render! {
///             BottomNavigationBar(items: tabs.items(), selected: tabs.selected(), on_tap: tabs.on_tap())
///         },
///         overlay: move || render! { ModalLayerHost(layer: tabs.modal_layer()) },
///     ) {
///         TabView(controller: tabs) { Outlet {} }
///     }
/// }
/// ```
#[component]
pub fn tab_view(controller: TabController, children: Children) -> Element {
    let fade_progress = controller.fade_progress();
    let style =
        computed(move || css!(flex_grow: 1.0, flex_basis: px(0)).opacity(fade_progress.get()));
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
    use whisker_router::{
        CompiledTree, NavError, RouteDef, RouteInstance, RouteRegistry, RouteTree, SwitchDef,
        provide_router,
    };

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

    /// Stack { Switch { (home) { Stack { home, home/:id } } (search) { Stack { search, search/:id } } } }
    fn tabbed_router() -> RouterHandle {
        let registry = ["home", "home_detail", "search", "search_detail"]
            .into_iter()
            .fold(RouteRegistry::new(), |registry, id| {
                registry.route(id, |_: &RouteInstance| {
                    whisker::runtime::view::create_phantom_element()
                })
            });
        let tree = CompiledTree::new(RouteTree::Stack(vec![RouteTree::Switch(
            SwitchDef::new("tabs", 0),
            vec![
                RouteTree::route_with(
                    RouteDef::new("(home)", "home_group"),
                    vec![RouteTree::Stack(vec![
                        RouteTree::route("home", "home"),
                        RouteTree::route("home/:id", "home_detail"),
                    ])],
                ),
                RouteTree::route_with(
                    RouteDef::new("(search)", "search_group"),
                    vec![RouteTree::Stack(vec![
                        RouteTree::route("search", "search"),
                        RouteTree::route("search/:id", "search_detail"),
                    ])],
                ),
            ],
        )]));
        RouterHandle::new((tree, registry))
    }

    fn tabs() -> Vec<TabItem> {
        vec![
            TabItem::new("/(home)", "/home", "", || "Home".into()),
            TabItem::new("/(search)", "/search", "", || "Search".into()),
        ]
    }

    /// Selects `index` the way [`TabController::select`] does.
    fn select(nav: &RouterHandle, index: usize, reset_on_tab_change: bool) {
        let destination = tab_destination(&tabs(), index, reset_on_tab_change).unwrap();
        nav.navigate(destination).unwrap();
        whisker::runtime::reactive::flush();
    }

    #[test]
    fn matches_tabs_by_group_segment() {
        let tabs = vec![
            TabItem::new("/(home)", "/home", "", || "Home".into()),
            TabItem::new("/(chat)", "/chat", "", || "Chat".into()),
            TabItem::new("/(mypage)", "/mypage", "", || "My".into())
                .matching(|p| p.starts_with("/mypage")),
        ];
        assert_eq!(tab_of(&tabs, "/home"), Some(0));
        assert_eq!(tab_of(&tabs, "/(chat)/chat/usage"), Some(1));
        assert_eq!(tab_of(&tabs, "/mypage/edit"), Some(2));
        assert_eq!(tab_of(&tabs, "/subscription"), None);
        for (index, tab) in tabs.iter().enumerate() {
            assert_eq!(tab_of(&tabs, tab.root), Some(index));
        }
    }

    #[test]
    fn tab_identifier_is_optional() {
        assert_eq!(
            TabItem::new("/(home)", "/home", "", || "Home".into()).identifier,
            None
        );
        let tab = TabItem::new("/(home)", "/home", "", || "Home".into()).identifier("tab_home");
        assert_eq!(tab.identifier.as_deref(), Some("tab_home"));
        let item = BottomNavigationBarItem::new("", || "Home".into()).identifier("tab_home");
        assert_eq!(item.identifier.as_deref(), Some("tab_home"));
    }

    #[test]
    fn destination_is_root_or_group() {
        let tabs = tabs();
        assert_eq!(tab_destination(&tabs, 1, true), Some("/search"));
        assert_eq!(tab_destination(&tabs, 1, false), Some("/(search)"));
        assert_eq!(tab_destination(&tabs, 2, true), None);
    }

    #[test]
    fn switching_back_shows_the_root_of_the_tab() {
        with_runtime(|| {
            let nav = tabbed_router();
            provide_router(nav.clone());
            let pathname = use_pathname();
            nav.navigate("/home").unwrap();
            select(&nav, 1, true);
            nav.push("/search/1").unwrap();
            select(&nav, 0, true);
            assert_eq!(pathname.get(), "/home");
            select(&nav, 1, true);
            assert_eq!(pathname.get(), "/search");
            assert!(matches!(nav.back(), Err(NavError::NothingToPop)));
        });
    }

    #[test]
    fn tapping_the_current_tab_shows_its_root() {
        with_runtime(|| {
            let nav = tabbed_router();
            provide_router(nav.clone());
            let pathname = use_pathname();
            nav.navigate("/home").unwrap();
            nav.push("/home/1").unwrap();
            nav.push("/home/2").unwrap();
            select(&nav, 0, true);
            assert_eq!(pathname.get(), "/home");
            assert!(matches!(nav.back(), Err(NavError::NothingToPop)));
        });
    }

    #[test]
    fn switching_keeps_the_screens_without_reset() {
        with_runtime(|| {
            let nav = tabbed_router();
            provide_router(nav.clone());
            let pathname = use_pathname();
            nav.navigate("/home").unwrap();
            select(&nav, 1, false);
            nav.push("/search/1").unwrap();
            select(&nav, 0, false);
            select(&nav, 1, false);
            assert_eq!(pathname.get(), "/search/1");
        });
    }
}
