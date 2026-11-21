# masamune-whisker-activity

Motion activity recognition (still, walking, running, cycling, in a vehicle) for Whisker apps,
the counterpart of Flutter's `activity_recognition_flutter`.

- **iOS**: `CMMotionActivityManager` (Motion & Fitness permission).
- **Android**: the Activity Recognition Transition API of Google Play services
  (`com.google.android.gms:play-services-location`).

## Setup

### iOS

```rust,ignore
use masamune_whisker_activity::cng::MasamuneActivity;

app.project_plugin::<MasamuneActivity>(|c| {
    c.motion_usage("Used to switch the music when you start or stop moving.");
});
```

`NSMotionUsageDescription` is required: iOS terminates an app that reads motion activity without
it. Localize it with `MasamuneLocalize`.

### Android

The crate's manifest declares `android.permission.ACTIVITY_RECOGNITION` (Android 10+),
`com.google.android.gms.permission.ACTIVITY_RECOGNITION` (Android 9 and earlier) and the
non-exported receiver `net.mathru.whisker.activity.ActivityTransitionReceiver`. The device needs
Google Play services; without it the status is `Unsupported`.

## Usage

```rust,ignore
use masamune_whisker_activity::{self as activity, ActivityKind, PermissionStatus};

if activity::permission_status() == PermissionStatus::NotDetermined {
    activity::request_permission().await?;
}
// Keep the subscription; dropping it stops recognition.
let watch = activity::watch_activity(|event| match event {
    Ok(event) if event.kind.is_moving() => { /* started moving */ }
    Ok(_) => {}
    Err(message) => { /* stopped */ }
})?;
```

## Behavior and limits

- iOS reports confidence as low / medium / high (0.33 / 0.66 / 1.0) and may report several flags
  at once; the most specific activity is used and updates with no flag are skipped.
- Android reports transitions (entering an activity) with confidence 1.0. Transitions are
  delivered while the app process runs; after the process ends the registration is removed by the
  system on the next start of the stream.
- One native stream serves every `watch_activity` subscription.
