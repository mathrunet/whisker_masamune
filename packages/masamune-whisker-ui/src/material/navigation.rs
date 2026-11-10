//! Bottom navigation (Flutter's `BottomNavigationBar`) and a tab layout driven by the router.

use std::cell::Cell;
use std::rc::Rc;

use masamune_whisker_appearance::{TextKind, palette, text_with};
use whisker::AnimConfig;
use whisker::AnimationController;
use whisker::css::{AlignItems, Color, Display, FlexDirection, JustifyContent};
use whisker::prelude::*;
use whisker_icons::Icon;
use whisker_router::{Outlet, use_navigator, use_pathname};

use crate::material::scaffold::Scaffold;
use crate::style::{css_color, row};

/// One destination of a [`BottomNavigationBar`].
#[derive(Clone)]
pub struct BottomNavigationBarItem {
    /// SVG of the icon (for example a `whisker_icons::lucide` constant).
    pub icon: &'static str,
    pub label: Rc<dyn Fn() -> String>,
}

impl BottomNavigationBarItem {
    pub fn new(icon: &'static str, label: impl Fn() -> String + 'static) -> Self {
        Self {
            icon,
            label: Rc::new(label),
        }
    }
}

/// Row of icon + label destinations. The safe area below it is reserved by [`Scaffold`].
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
                    let (on, off) = (css_color(selected_color), css_color(unselected_icon_color));
                    let icon_color = computed(move || if is_selected.get() { on.clone() } else { off.clone() });
                    let label_style = computed(move || {
                        text_with(
                            TextKind::LabelSmall,
                            if is_selected.get() { selected_color } else { unselected_label_color },
                        )
                    });
                    render! {
                        View(
                            style: css!(flex_grow: 1.0, padding: (px(8), px(0)), align_items: AlignItems::Center)
                                .display(Display::Flex)
                                .flex_direction(FlexDirection::Column)
                                .gap(px(2)),
                            on_tap: move |_| on_tap.run(index),
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

/// A tab of [`TabScaffold`], bound to a router group such as `"/(home)"`.
#[derive(Clone)]
pub struct TabItem {
    /// Router group navigated to when the tab is tapped.
    pub group: &'static str,
    pub icon: &'static str,
    pub label: Rc<dyn Fn() -> String>,
    /// Whether a pathname belongs to this tab.
    pub matches: Rc<dyn Fn(&str) -> bool>,
}

impl TabItem {
    /// Tab for `group` (`"/(name)"`). Paths containing `/name` belong to it.
    pub fn new(
        group: &'static str,
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
            icon,
            label: Rc::new(label),
            matches: Rc::new(move |path: &str| path.contains(&segment)),
        }
    }

    /// Replaces how paths are matched to this tab.
    pub fn matching(mut self, matches: impl Fn(&str) -> bool + 'static) -> Self {
        self.matches = Rc::new(matches);
        self
    }
}

/// Index of the tab `path` belongs to (`None` for screens outside the tabs).
pub fn tab_of(tabs: &[TabItem], path: &str) -> Option<usize> {
    tabs.iter().position(|tab| (tab.matches)(path))
}

/// Default fade of the body when switching tabs (ms).
pub const TAB_FADE_MS: u32 = 220;

/// Layout route for tabs: the routed body above a [`BottomNavigationBar`].
///
/// The body fades in only when the tab changes; coming back from a screen outside the tabs
/// (pushed on the outer stack) does not count as a switch. The bar hides while the keyboard is
/// open. Use it as the component of a `Route` that wraps a `Switch` of tab groups (see the
/// crate README); the router patch in whisker_masamune keeps the tabs mounted.
#[component]
pub fn tab_scaffold(tabs: Vec<TabItem>, #[prop(default = TAB_FADE_MS)] fade_ms: u32) -> Element {
    let nav = use_navigator();
    let pathname = use_pathname();
    let fade = AnimationController::new(AnimConfig::ease_out(fade_ms));
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
    let body_style =
        computed(move || css!(flex_grow: 1.0, flex_basis: px(0)).opacity(fade_progress.get()));
    let items: Vec<BottomNavigationBarItem> = tabs
        .iter()
        .map(|tab| BottomNavigationBarItem {
            icon: tab.icon,
            label: tab.label.clone(),
        })
        .collect();
    let groups: Vec<&'static str> = tabs.iter().map(|tab| tab.group).collect();
    let on_tap = Callback::new(move |index: usize| {
        if let Some(group) = groups.get(index) {
            let _ = nav.navigate(group);
        }
    });
    let bar = move || {
        let items = items.clone();
        render! {
            BottomNavigationBar(items: items, selected: selected, on_tap: on_tap)
        }
    };
    render! {
        Scaffold(safe_area_top: false, bottom_navigation_bar: bar) {
            View(style: body_style) {
                Outlet {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_tabs_by_group_segment() {
        let tabs = vec![
            TabItem::new("/(home)", "", || "Home".into()),
            TabItem::new("/(chat)", "", || "Chat".into()),
            TabItem::new("/(mypage)", "", || "My".into()).matching(|p| p.starts_with("/mypage")),
        ];
        assert_eq!(tab_of(&tabs, "/home"), Some(0));
        assert_eq!(tab_of(&tabs, "/(chat)/chat/usage"), Some(1));
        assert_eq!(tab_of(&tabs, "/mypage/edit"), Some(2));
        assert_eq!(tab_of(&tabs, "/subscription"), None);
    }
}
