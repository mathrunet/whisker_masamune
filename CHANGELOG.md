# Changelog

## 0.2.1

- `masamune-whisker-ui`: `Scaffold` paints the bottom safe area with the color of the bar above it
  (`bottom_navigation_bar_color`, `bottom_sheet_color`). `TabScaffold` takes `bar_color`.

## 0.1.0

- Initial release. Moved the native modules built for NANSURU into `masamune-whisker-*` crates:
  bridge, fonts, device-info, app-control, date-picker, view-capture, share, image-picker,
  local-notification, in-app-review, billing, admob, google-sign-in, apple-sign-in,
  firebase-app-check, firebase-remote-config, firebase-firestore-db, firebase-ai.
- Added the `whisker-router` fix that keeps tabs mounted when switching tabs inside a `Stack`.
