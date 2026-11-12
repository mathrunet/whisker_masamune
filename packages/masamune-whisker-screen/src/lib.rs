//! Screen size and orientation for Whisker apps.
//!
//! Android recreates the activity on rotation by default, and recreating the activity recreates
//! the whole Whisker runtime (the app restarts). Declare the size-related changes in the
//! activity's `configChanges` with the [`cng::MasamuneScreen`] CNG plugin so rotation and split
//! screen only re-layout, then read [`screen_size`] / [`orientation`] in reactive closures.

pub mod cng;
pub use cng::IosOrientation;

use std::ops::BitOr;

pub use masamune_whisker_app_control::OrientationLock;
use masamune_whisker_app_control::{NativeResult, set_orientation};
use masamune_whisker_device_info::{ScreenMetrics, Subscription};
use whisker::prelude::*;

/// Window size in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenSize {
    pub width: f32,
    pub height: f32,
}

impl ScreenSize {
    /// Fallback while the size is unknown (a typical phone).
    pub const FALLBACK: ScreenSize = ScreenSize {
        width: 390.0,
        height: 844.0,
    };

    fn from_dp(width: f64, height: f64) -> Self {
        Self {
            width: Some(width as f32)
                .filter(|w| *w > 0.0)
                .unwrap_or(Self::FALLBACK.width),
            height: Some(height as f32)
                .filter(|h| *h > 0.0)
                .unwrap_or(Self::FALLBACK.height),
        }
    }

    pub fn orientation(&self) -> Orientation {
        if self.width > self.height {
            Orientation::Landscape
        } else {
            Orientation::Portrait
        }
    }
}

/// Orientation derived from the window size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Orientation {
    Portrait,
    Landscape,
}

struct Slot {
    size: ReadSignal<ScreenSize>,
    _subscription: Subscription,
}

whisker::runtime_local! {
    static SIZE: Slot = {
        let info = masamune_whisker_device_info::info().ok();
        let size = signal(ScreenSize::from_dp(
            info.as_ref().map_or(0.0, |i| i.width_dp),
            info.as_ref().map_or(0.0, |i| i.height_dp),
        ));
        let subscription = masamune_whisker_device_info::on_metrics_changed(move |m: ScreenMetrics| {
            let next = ScreenSize::from_dp(m.width_dp, m.height_dp);
            if size.get_untracked() != next {
                size.set(next);
            }
        });
        Slot { size: size.read_only(), _subscription: subscription }
    };
}

/// Window size. Updates on rotation, split screen and window resizing.
pub fn screen_size() -> ReadSignal<ScreenSize> {
    SIZE.with(|slot| slot.size)
}

/// Orientation of the window.
pub fn orientation() -> ReadSignal<Orientation> {
    let size = screen_size();
    computed(move || size.get().orientation())
}

/// Current window width. Use [`screen_size`] to follow changes.
pub fn screen_width() -> f32 {
    screen_size().get_untracked().width
}

/// Current window height. Use [`screen_size`] to follow changes.
pub fn screen_height() -> f32 {
    screen_size().get_untracked().height
}

/// Locks (or unlocks with [`OrientationLock::Unspecified`]) the screen orientation.
pub fn set_orientation_lock(lock: OrientationLock) -> NativeResult<bool> {
    set_orientation(lock)
}

/// Android `configChanges` the activity handles itself instead of being recreated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ConfigChanges(u32);

impl ConfigChanges {
    /// Rotation, split screen and window resizing.
    pub const RESIZE: ConfigChanges = ConfigChanges(1);
    /// Hardware keyboard attach/detach.
    pub const KEYBOARD: ConfigChanges = ConfigChanges(1 << 1);
    /// Dark mode (handle it with `masamune-whisker-appearance`).
    pub const UI_MODE: ConfigChanges = ConfigChanges(1 << 2);
    /// System language and layout direction (handle it with `masamune-whisker-appearance`).
    pub const LOCALE: ConfigChanges = ConfigChanges(1 << 3);
    /// Display density.
    pub const DENSITY: ConfigChanges = ConfigChanges(1 << 4);
    /// System font size.
    pub const FONT_SCALE: ConfigChanges = ConfigChanges(1 << 5);

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn from_bits(bits: u32) -> Self {
        ConfigChanges(bits)
    }

    pub fn contains(self, other: ConfigChanges) -> bool {
        self.0 & other.0 == other.0
    }

    /// Value of `android:configChanges`.
    pub fn android_value(self) -> String {
        let mut values = Vec::new();
        if self.contains(Self::RESIZE) {
            values.extend([
                "orientation",
                "screenSize",
                "smallestScreenSize",
                "screenLayout",
            ]);
        }
        if self.contains(Self::KEYBOARD) {
            values.extend(["keyboard", "keyboardHidden"]);
        }
        if self.contains(Self::UI_MODE) {
            values.push("uiMode");
        }
        if self.contains(Self::LOCALE) {
            values.extend(["locale", "layoutDirection"]);
        }
        if self.contains(Self::DENSITY) {
            values.push("density");
        }
        if self.contains(Self::FONT_SCALE) {
            values.push("fontScale");
        }
        values.join("|")
    }
}

impl BitOr for ConfigChanges {
    type Output = ConfigChanges;

    fn bitor(self, rhs: ConfigChanges) -> ConfigChanges {
        ConfigChanges(self.0 | rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_config_changes() {
        let changes = ConfigChanges::RESIZE
            | ConfigChanges::KEYBOARD
            | ConfigChanges::UI_MODE
            | ConfigChanges::LOCALE;
        assert_eq!(
            changes.android_value(),
            "orientation|screenSize|smallestScreenSize|screenLayout|keyboard|keyboardHidden|uiMode|locale|layoutDirection"
        );
    }

    #[test]
    fn derives_orientation() {
        assert_eq!(
            ScreenSize {
                width: 800.0,
                height: 400.0
            }
            .orientation(),
            Orientation::Landscape
        );
        assert_eq!(ScreenSize::from_dp(0.0, 0.0), ScreenSize::FALLBACK);
    }
}
