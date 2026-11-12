//! Material-style widgets for Whisker apps.
//!
//! Widgets read their default colors and text styles from `masamune-whisker-appearance`, so
//! they follow the app's palette and theme mode.
//!
//! | Flutter | here |
//! |---|---|
//! | `Scaffold` | [`Scaffold`] (+ [`use_scaffold_insets`], [`SafeArea`]) |
//! | `AppBar` | [`AppBar`], [`AppBarAction`] |
//! | `BottomNavigationBar` | [`BottomNavigationBar`] |
//! | `TabController` | [`TabController`] ([`use_tab_controller`], [`TabItem`]) |
//! | `TabBarView` | [`TabView`] |
//! | `TabBar` + `TabBarView` (in-screen, swipeable) | [`TabBar`] (underlined), [`SegmentedTabBar`] (pill), [`TabBarView`], [`TabPage`] ([`use_page_tab_controller`], [`PageTabController`]) |
//! | `showModalBottomSheet` | [`ModalBottomSheet`] (+ [`ModalLayer`], [`ModalLayerHost`]) |
//! | `showDialog` / `AlertDialog` | [`Overlay`] + [`OverlayHost`] |
//! | `CircularProgressIndicator` | [`CircularProgressIndicator`] |
//! | `FilledButton` / `OutlinedButton` / `TextButton` | [`FilledButton`], [`OutlinedButton`], [`TextButton`] |
//! | `InkWell` | [`InkWell`] |
//! | `Divider` | [`Divider`], [`LabeledDivider`] |
//! | `FadeUpwardsPageTransitionsBuilder` | [`FadeUpwardsTransition`] ([`fade_upwards_transition`]) |
//! | `TransitionQuery.fullscreen` / `fullscreenDialog` | [`fullscreen_transition`] on the route + [`Fullscreen`] in the page ([`use_fullscreen`]) |
//! | `showTimePicker` / `TimePickerDialog` | [`TimePickerDialog`] (+ [`TimeOfDay`], [`HourFormat`]) |

pub mod material;
pub mod style;

pub use material::app_bar::{APP_BAR_HEIGHT, AppBar, AppBarAction};
pub use material::bottom_sheet::{
    ModalBottomSheet, ModalLayer, ModalLayerHost, provide_modal_layer,
};
pub use material::buttons::{FilledButton, OutlinedButton, TextButton};
pub use material::dialog::{
    BusyHost, DIALOG_CANCEL_IDENTIFIER, DIALOG_SUBMIT_IDENTIFIER, Dialog, DialogHost, Overlay,
    OverlayHost, provide_overlay, use_overlay,
};
pub use material::divider::{Divider, LabeledDivider, VerticalGap};
pub use material::fullscreen::{
    Fullscreen, fullscreen_transition, provide_fullscreen, use_fullscreen,
};
pub use material::ink_well::{InkWell, default_highlight_color, default_splash_color};
pub use material::navigation::{
    BottomNavigationBar, BottomNavigationBarItem, TAB_FADE_MS, TabController, TabItem, TabOptions,
    TabView, tab_destination, tab_of, use_tab_controller, use_tab_controller_with,
};
pub use material::page_transition::{
    FADE_UPWARDS_MS, FADE_UPWARDS_OFFSET_PCT, FadeUpwardsTransition, fade_upwards_transition,
};
pub use material::progress::CircularProgressIndicator;
pub use material::scaffold::{SafeArea, Scaffold, ScaffoldInsets, Slot, use_scaffold_insets};
pub use material::tab_bar::{
    PageTabController, SegmentedTab, SegmentedTabBar, TabBar, TabBarView, TabPage,
    use_page_tab_controller,
};
pub use material::time_picker::{
    HourFormat, TimeOfDay, TimePickerDialog, TimePickerEntryMode, TimePickerLabels,
};
