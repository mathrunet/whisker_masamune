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

Switching tabs inside a `Stack` needs the `whisker-router` fix shipped in this repository,
otherwise the whole tab layout is recreated on every switch:

```toml
[patch.crates-io]
whisker-router = { git = "https://github.com/mathrunet/whisker_masamune", tag = "v0.2.0" }
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
