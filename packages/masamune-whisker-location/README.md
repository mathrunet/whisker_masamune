# masamune-whisker-location

Location permission, the current position and position streams for Whisker apps (the counterpart
of Flutter's `masamune_location` / `geolocator`).

- **iOS**: `CLLocationManager` (iOS 15 or later; precise / approximate accuracy authorization).
- **Android**: the framework `LocationManager` (the fused provider on Android 12 and later, then
  GPS or network). No Google Play services dependency. Background updates run in a `location`
  foreground service.

## Setup

### iOS

Write the usage descriptions (iOS terminates an app that asks without them) and, for background
updates, the `location` background mode:

```rust,ignore
use masamune_whisker_location::cng::MasamuneLocation;

app.project_plugin::<MasamuneLocation>(|c| {
    c.when_in_use_usage("Used to match the music to where you are.")
        .always_usage("Used to keep the soundtrack in step while the screen is off.")
        .background(true); // UIBackgroundModes: location
});
```

Localize the descriptions with `masamune-whisker-localize`'s `MasamuneLocalize` (`strings(lang,
"NSLocationWhenInUseUsageDescription", …)`).

### Android

The crate's `AndroidManifest.xml` declares `ACCESS_FINE_LOCATION`, `ACCESS_COARSE_LOCATION`,
`FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_LOCATION` and

```xml
<service android:name="net.mathru.whisker.location.LocationForegroundService"
    android:foregroundServiceType="location" android:exported="false" />
```

Google Play asks for a declaration of the `location` foreground service type. Background updates
through the foreground service only need the "while in use" permission.
`ACCESS_BACKGROUND_LOCATION` (positions without a foreground service, "Allow all the time") is
added only with `c.background_permission(true)` and needs a Play Console declaration.

## Usage

```rust,ignore
use std::time::Duration;
use masamune_whisker_location::{
    self as location, Accuracy, AndroidLocationNotification, LocationSettings, PermissionStatus,
};

if location::permission_status() == PermissionStatus::NotDetermined {
    location::request_permission().await?;
}
if location::permission_status().is_granted() {
    let here = location::current_position(Accuracy::High, Duration::from_secs(10)).await?;
}

// Keep the subscription; dropping it stops the updates.
let watch = location::watch_position(
    &LocationSettings {
        accuracy: Accuracy::High,
        distance_filter: 20.0,
        interval_ms: 10_000,
        background: true,
        android_notification: AndroidLocationNotification {
            // Show the media notification instead of a second one while it exists.
            reuse_notification_id: Some(masamune_whisker_audio_session::ANDROID_NOTIFICATION_ID),
            channel_id: "location".into(),
            channel_name: "Location".into(),
            title: "Segue".into(),
            text: "Following your day".into(),
            ..Default::default()
        },
    },
    |update| match update {
        Ok(position) => { /* position.latitude, position.longitude */ }
        Err(error) => { /* updates stopped: error.code */ }
    },
)?;
```

## Behavior and limits

- Android cannot tell "not asked yet" from "denied" by itself; the crate remembers whether it
  asked, so `permission_status` reports `NotDetermined` until the first request.
- On Android 11 and later `request_background_permission` opens the system settings page instead
  of a dialog, and the permission must be declared (`background_permission(true)`).
- One native stream serves every `watch_position` subscription: the latest settings win and the
  stream stops when the last subscription is dropped.
- Background updates on Android need the stream to start while the app is in the foreground
  (Android 12 and later refuse to start the foreground service from the background); otherwise
  an error with `BackgroundUnavailable` is reported.
- On iOS, `background: true` without `location` in `UIBackgroundModes` reports
  `BackgroundUnavailable` instead of crashing.
- `distance_between` computes great-circle distances in Rust (no native call).
