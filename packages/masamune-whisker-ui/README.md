# masamune-whisker-ui

Material-style widgets for [Whisker](https://whisker.rs) apps. Colors and text styles come from
`masamune-whisker-appearance`, so every widget follows the app palette and theme mode.

| Flutter | masamune-whisker-ui |
|---|---|
| `Scaffold` | `Scaffold`, `SafeArea`, `use_scaffold_insets` |
| `AppBar` | `AppBar`, `AppBarAction` |
| `BottomNavigationBar` | `BottomNavigationBar` |
| `TabController` / `TabBarView` | `TabController` (`use_tab_controller`, `TabItem`) / `TabView` |
| `TabBar` + `TabBarView` (in-screen, swipeable) | `TabBar` (underlined) or `SegmentedTabBar` (pill) with `SegmentedTab` + `TabBarView` / `TabPage` (`use_page_tab_controller`) |
| `showModalBottomSheet` | `ModalBottomSheet` |
| `showDialog` / `AlertDialog` | `Overlay` + `OverlayHost` |
| `CircularProgressIndicator` | `CircularProgressIndicator` |
| `RefreshIndicator` / `UniversalListView.onRefresh` | `RefreshScrollView` (`RefreshCallback`) |
| `FilledButton` / `OutlinedButton` / `TextButton` | same names |
| `Divider` | `Divider`, `LabeledDivider` |
| `FadeUpwardsPageTransitionsBuilder` | `fade_upwards_transition()` (`FadeUpwardsTransition`) for a route's `transition:` |
| `TransitionQuery.fullscreen` / `fullscreenDialog` | `fullscreen_transition()` on the route + `Fullscreen` in the page (`use_fullscreen()`) |

More widgets (ListTile, Card, Switch, Checkbox, TextField, Chip, SnackBar, Drawer,
NavigationRail) will be added under `src/material/`, one file per widget family.

## Scaffold and the keyboard

Apps run edge-to-edge, so the scaffold reserves the safe area and the keyboard itself:
the body shrinks when the keyboard opens, `bottom_sheet` rises with it, and
`bottom_navigation_bar` stays behind it. Nested scaffolds do not reserve space twice.
Safe areas take the color of the bar next to them: `AppBar` paints the top one with its own
color, and the bottom one is painted with `bottom_navigation_bar_color` (default: the
`BottomNavigationBar` background) or `bottom_sheet_color` (default: transparent).
The body is padded by the side safe areas (landscape notches and cutouts); pass
`safe_area_horizontal: false` to draw it edge to edge and pad the content yourself with
`use_scaffold_insets` or `SafeArea`.
To show a page as a sheet that rises with the keyboard, put it in `bottom_sheet` and round it
with `bottom_sheet_radius`; the sheet shrinks once the body is gone, so wrap long content in a
`ScrollView`.

```rust
render! {
    Scaffold(app_bar: move || render! { AppBar(title: "Settings", back: true) {} }) {
        // body
    }
}
```

## Full-screen pages

Declare a full-screen page (Flutter's `TransitionQuery.fullscreen`) in two places: open its route
with `fullscreen_transition()` (the page rises from below while fading in) and wrap the page in
`Fullscreen`. Every `AppBar` inside then shows a close button (X, `<identifier>_close`) at the top
start by default, also while the page is loading and even with `back: true`; pass
`hide_leading: true` to show no leading button. Custom headers can follow the same rule with
`use_fullscreen()`.

```rust
// routes
Route(path: "items/:id", component: ItemPage, transition: fullscreen_transition())

// page
render! {
    Fullscreen {
        Scaffold(app_bar: move || render! { AppBar(title: "") {} }) {
            // body
        }
    }
}
```

## Touch feedback (InkWell)

`InkWell` is Flutter's `InkWell`: pressing it shows a highlight and an ink splash that grows from
the touch point and fades out on release, clipped to the box (round it with `border_radius` in
`style`). `FilledButton`, `OutlinedButton`, `TextButton`, `AppBar` buttons, `AppBarAction`,
`BottomNavigationBar`, `TabBar`, `SegmentedTabBar` and the dialog buttons already use it. Use it
instead of `View(on_tap: ..)` for your own tappable rows, tiles and buttons.

```rust
render! {
    InkWell(
        style: row().padding(px(16)).border_radius(px(shape().medium)),
        accessibility: Accessibility::new().identifier("open_item").label("Open").role(AccessibilityRole::Button),
        on_tap: Callback::new(move |()| open()),
    ) {
        Text(value: "Open")
    }
}
```

Like Flutter, the feedback starts 100 ms after the finger goes down (or on release for quick taps)
and is cancelled when the finger moves more than the tap slop, so starting a scroll does not
light up rows. Colors default to `default_splash_color()` / `default_highlight_color()` (the
theme's `text` color, translucent); pass `splash_color` / `highlight_color` on colored
backgrounds. `disabled` turns off both the feedback and the callbacks. The splash origin comes from
`TouchEvent::offset` / `TouchEvent::size`, which needs whisker 0.17 from the mathrunet/whisker fork
main (whiskerrs/whisker#829).

## Tabs

Tabs are built like in Flutter: a `TabController` drives a `BottomNavigationBar` passed to the
`bottom_navigation_bar` of a `Scaffold` and a `TabView` that shows the routed body. Use them in
the layout component of a route that wraps a `Switch` of tab groups.

- Each `TabItem` names the router group of the tab and the URL of its root screen.
- Selecting a tab always shows its root screen, whether it is switched to from another tab or
  tapped again. Pass `TabOptions { reset_on_tab_change: false, .. }` to
  `use_tab_controller_with` to keep the screens left open in a tab while switching.
- The body fades only when the tab changes (not when returning from a screen pushed on the outer
  stack), and the bar hides while the keyboard is open (the `Scaffold` keeps
  `bottom_navigation_bar` behind the keyboard).
- A `ModalBottomSheet` on a tab screen covers the bottom navigation bar (the `TabController`
  provides a `ModalLayer`; draw it with `ModalLayerHost` in the `overlay`) and closes when the
  tab changes.
- Tab screens are not padded by the side safe areas (`safe_area_horizontal: false`), so each
  screen's own `Scaffold` decides whether its background runs edge to edge in landscape.

```rust
fn tabs() -> Vec<TabItem> {
    vec![
        TabItem::new("/(home)", "/home", lucide::House, || "Home".into()),
        TabItem::new("/(mypage)", "/mypage", lucide::User, || "My page".into()),
    ]
}

#[component]
fn tabs_layout() -> Element {
    let tabs = use_tab_controller(tabs());
    render! {
        Scaffold(
            safe_area_top: false,
            safe_area_horizontal: false,
            bottom_navigation_bar: move || render! {
                BottomNavigationBar(items: tabs.items(), selected: tabs.selected(), on_tap: tabs.on_tap())
            },
            overlay: move || render! { ModalLayerHost(layer: tabs.modal_layer()) },
        ) {
            TabView(controller: tabs) { Outlet {} }
        }
    }
}

routes! {
    Stack {
        Route(component: TabsLayout) {
            Switch {
                Stack(path: "(home)") { Route(path: "home", component: Home, transition: RouteTransition::fade()) }
                Stack(path: "(mypage)") { Route(path: "mypage", component: MyPage, transition: RouteTransition::fade()) }
            }
        }
        Route(path: "settings", component: Settings)
    }
}
```

To open a tab from a screen, navigate to the tab's root URL (`nav.navigate("/home")`), or call
`select(index)` on the `TabController`.

Switching tabs inside a `Stack` needs the `whisker-router` fix in the
[mathrunet/whisker](https://github.com/mathrunet/whisker) fork, otherwise the whole tab layout
is recreated on every switch:

```toml
[patch.crates-io]
whisker-router = { git = "https://github.com/mathrunet/whisker", branch = "main" }
```

## Swipeable tabs inside a screen

`SegmentedTabBar` + `TabBarView` are Flutter's `TabBar` + `TabBarView` for tabs inside one
screen (no router). Pages follow the finger and settle one page per swipe with the platform's
scrolling; the pill indicator follows the swipe, and tapping a tab scrolls to its page. The
pages stay mounted, so their state survives switching tabs.

```rust
let tabs = use_page_tab_controller(3, 0);
render! {
    View(style: css!(flex_grow: 1.0, display: Display::Flex, flex_direction: FlexDirection::Column)) {
        SegmentedTabBar(
            controller: tabs,
            tabs: vec![
                SegmentedTab::new(|| "Stock".into()).identifier("home_tab_stock"),
                SegmentedTab::new(|| "Trading".into()),
                SegmentedTab::new(|| "Favorite".into()),
            ],
        )
        TabBarView(controller: tabs) {
            TabPage(controller: tabs) { ScrollView(style: css!(flex_grow: 1.0)) { StockList() } }
            TabPage(controller: tabs) { ScrollView(style: css!(flex_grow: 1.0)) { TradingList() } }
            TabPage(controller: tabs) { ScrollView(style: css!(flex_grow: 1.0)) { FavoriteList() } }
        }
    }
}
// tabs.index() is the selected page, tabs.select(1) switches with an animation.
```

For Flutter's default underlined look, use `TabBar` instead of `SegmentedTabBar`. Labels are
centered in equal-width tabs, and the indicator line is drawn over the bottom divider while it
follows the swipe.

```rust
let tabs = use_page_tab_controller(2, 0);
render! {
    TabBar(
        controller: tabs,
        tabs: vec![SegmentedTab::new(|| "Orders".into()), SegmentedTab::new(|| "History".into())],
    )
    TabBarView(controller: tabs) {
        TabPage(controller: tabs) { OrderList() }
        TabPage(controller: tabs) { HistoryList() }
    }
}
```

## Pull to refresh

`RefreshScrollView` is Flutter's `RefreshIndicator` (or `UniversalListView` with `onRefresh`): a
vertical `ScrollView` that shows the native pull-to-refresh control (iOS `UIRefreshControl`,
Android `SwipeRefreshLayout`). `on_refresh` is an async closure; the spinner stays up until its
future finishes, and pulls while it runs are ignored. While refreshing it holds a
`masamune.indicator.show|refresh` trace, so `masamune-whisker-ai-debugger` notices spinners that
never stop. Without `on_refresh`, and on hosts other than iOS and Android, it behaves as a plain
`ScrollView`.

The children sit in a column (`content_style`) that is at least as tall as the viewport, so put
the list and its empty state in the same `RefreshScrollView`: a centered empty message fills
the screen and can still be pulled.

```rust
let items = RwSignal::new(Vec::<Item>::new());
render! {
    RefreshScrollView(
        style: css!(flex_grow: 1.0),
        content_style: css!(padding: px(16), gap: px(8)),
        on_refresh: move || async move { items.set(load_items().await) },
    ) {
        Show(when: move || items.with(Vec::is_empty)) {
            View(style: css!(flex_grow: 1.0, justify_content: JustifyContent::Center, align_items: AlignItems::Center)) {
                Text(value: "No items")
            }
        }
        ForEach(
            each: move || items.get(),
            key: |item: &Item| item.id.clone(),
            children: |item: ReadSignal<Item>| render! { ItemRow(item: item) },
        )
    }
}
```

With swipeable tabs, put one `RefreshScrollView` in each `TabPage` instead of the `ScrollView`:

```rust
TabBarView(controller: tabs) {
    TabPage(controller: tabs) {
        RefreshScrollView(style: css!(flex_grow: 1.0), on_refresh: move || async move { stock.reload().await }) { StockList() }
    }
    TabPage(controller: tabs) {
        RefreshScrollView(style: css!(flex_grow: 1.0), on_refresh: move || async move { trading.reload().await }) { TradingList() }
    }
}
```

## Dialogs and the busy indicator

```rust
let overlay = provide_overlay();
render! {
    View { /* app */ }
    OverlayHost(overlay: overlay)
}
// anywhere below
overlay.confirm("Delete", "Are you sure?", "Delete", "Cancel", move || { /* … */ }, None);
overlay.guarded(async { /* shows a spinner while running */ }).await;
```

## Maestro / accessibility identifiers

Interactive widgets expose accessibility semantics, so screen readers and UI automation
(Maestro, XCUITest, UIAutomator) can find them. Pass `identifier` to select a widget by id
(`id:` in Maestro, `accessibilityIdentifier` on iOS); its label (`text:` in Maestro) comes from
the visible text.

| Widget | Props | Semantics |
|---|---|---|
| `FilledButton`, `OutlinedButton`, `TextButton` | `identifier: Option<String>` | label = `label`, role button, disabled while `disabled` or `loading` |
| `AppBar` | `identifier: Option<String>` | the title is a header labelled with `title`; the back button is `<identifier>_back`, labelled "Back" |
| `AppBarAction` | `identifier: Option<String>`, `label: Option<String>` | role button with its disabled state; pass `label`, the icon has no text |
| `BottomNavigationBarItem`, `TabItem`, `SegmentedTab` | `.identifier(..)` (field `identifier: Option<String>`) | role tab labelled with the item label, with its selected state |
| `DialogHost` (`OverlayHost`) | – | buttons `dialog_submit` / `dialog_cancel`, labelled with their text |

```rust
render! {
    Scaffold(app_bar: move || render! { AppBar(title: "Home", identifier: "home_app_bar") {} }) {
        FilledButton(label: "Increment", identifier: "home_increment", on_tap: increment)
    }
}
```

```yaml
- tapOn:
    id: home_increment
- assertVisible:
    text: "Count: 1"
```

For your own views, set `accessibility:` with a label and a role next to the identifier. On iOS a
view becomes an accessibility element only when it has a label, hint or role, so a view with only
an identifier is not visible to Maestro:

```rust
let count_semantics = computed(move || {
    Accessibility::new()
        .identifier("home_count")
        .label(format!("Count: {}", count.get()))
        .role(AccessibilityRole::Text)
});
render! {
    View(accessibility: count_semantics) {
        Text(value: computed(move || format!("Count: {}", count.get())))
    }
}
```
