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
//! | tab shell with routed body | [`TabScaffold`] |
//! | `showModalBottomSheet` | [`ModalBottomSheet`] |
//! | `showDialog` / `AlertDialog` | [`Overlay`] + [`OverlayHost`] |
//! | `CircularProgressIndicator` | [`CircularProgressIndicator`] |
//! | `FilledButton` / `OutlinedButton` / `TextButton` | [`FilledButton`], [`OutlinedButton`], [`TextButton`] |
//! | `Divider` | [`Divider`], [`LabeledDivider`] |
//! | `showTimePicker` / `TimePickerDialog` | [`TimePickerDialog`] (+ [`TimeOfDay`], [`HourFormat`]) |

pub mod material;
pub mod style;

pub use material::app_bar::{APP_BAR_HEIGHT, AppBar, AppBarAction};
pub use material::bottom_sheet::ModalBottomSheet;
pub use material::buttons::{FilledButton, OutlinedButton, TextButton};
pub use material::dialog::{
    BusyHost, Dialog, DialogHost, Overlay, OverlayHost, provide_overlay, use_overlay,
};
pub use material::divider::{Divider, LabeledDivider, VerticalGap};
pub use material::navigation::{
    BottomNavigationBar, BottomNavigationBarItem, TAB_FADE_MS, TabItem, TabScaffold, tab_of,
};
pub use material::progress::CircularProgressIndicator;
pub use material::scaffold::{SafeArea, Scaffold, ScaffoldInsets, Slot, use_scaffold_insets};
pub use material::time_picker::{
    HourFormat, TimeOfDay, TimePickerDialog, TimePickerEntryMode, TimePickerLabels,
};
