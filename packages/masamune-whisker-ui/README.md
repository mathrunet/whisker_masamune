# masamune-whisker-ui

Material-style widgets for [Whisker](https://whisker.rs) apps. Colors and text styles come from
`masamune-whisker-appearance`, so every widget follows the app palette and theme mode.

| Flutter | masamune-whisker-ui |
|---|---|
| `Scaffold` | `Scaffold`, `SafeArea`, `use_scaffold_insets` |
| `AppBar` | `AppBar`, `AppBarAction` |
| `BottomNavigationBar` | `BottomNavigationBar` |
| Tab shell with a routed body | `TabScaffold` |
| `showModalBottomSheet` | `ModalBottomSheet` |
| `showDialog` / `AlertDialog` | `Overlay` + `OverlayHost` |
| `CircularProgressIndicator` | `CircularProgressIndicator` |
| `FilledButton` / `OutlinedButton` / `TextButton` | same names |
| `Divider` | `Divider`, `LabeledDivider` |

More widgets (ListTile, Card, Switch, Checkbox, TextField, Chip, SnackBar, TabBar, Drawer,
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

```rust
render! {
    Scaffold(app_bar: move || render! { AppBar(title: "Settings", back: true) {} }) {
        // body
    }
}
```

## Tabs

`TabScaffold` is the layout component of a route that wraps a `Switch` of tab groups.
The body fades only when the tab changes (not when returning from a screen pushed on the outer
stack), and the bar hides while the keyboard is open.
Tab screens are not padded by the side safe areas, so each screen's own `Scaffold` decides
whether its background runs edge to edge in landscape.

```rust
fn tabs() -> Vec<TabItem> {
    vec![
        TabItem::new("/(home)", lucide::House, || "Home".into()),
        TabItem::new("/(mypage)", lucide::User, || "My page".into()),
    ]
}

#[component]
fn tabs_layout() -> Element {
    render! { TabScaffold(tabs: tabs()) }
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

Switching tabs inside a `Stack` needs the `whisker-router` fix in the
[mathrunet/whisker](https://github.com/mathrunet/whisker) fork, otherwise the whole tab layout
is recreated on every switch:

```toml
[patch.crates-io]
whisker-router = { git = "https://github.com/mathrunet/whisker", branch = "main" }
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
