# Changelog

## 0.3.0 (unreleased)

- Added `masamune-whisker-adapter`: `MasamuneAdapter` / `MasamuneAdapters` register plugin
  settings and startup work in one place (Masamune's `masamuneAdapters`). `install` prepares the
  adapters in order and spawns their start work under the installing owner; `primary()` reads
  the installed settings.
- `masamune-whisker-local-notification`: added `LocalNotificationMasamuneAdapter`, which
  configures the channel while the app prepares and re-arms the saved schedules after startup.
- `masamune-whisker-appearance`: `ThemeConfig` is now the app-wide theme, like Masamune's
  `AppThemeData`.
  - `Palette` adds `tertiary`, the `*_container` colors, `on_error`, `on_warning`, `success`,
    `info` (with `on_*`) and `shadow`. `Palette::with_brand` swaps the brand colors of a baseline
    and `ColorExt::with_alpha` makes translucent variants.
  - `Typography` (`ThemeConfig::typography`) configures the 15 text roles (size, weight, line
    height, letter spacing) plus `font_size_factor` / `font_size_delta`. `text()` / `bold()` now
    set line height and letter spacing from the Material 3 type scale.
  - `ThemeConfig::font_family` sets the default font family resolver (like
    `defaultFontFamilyResolver`) and `TextStyle::with_family` gives one role its own family.
    Font files are still registered with `masamune-whisker-fonts`.
  - `Shape` / `shape()`, `Spacing` / `spacing()` and `elevation(level)` add corner radius, gap and
    shadow tokens. `set_font_family_resolver` / `font_family` are exported from the crate root.
  - Breaking: `Palette` and `ThemeConfig` have new fields. Build them with struct update syntax:
    `Palette { primary: .., ..Palette::MATERIAL_DARK }` (or `Palette::MATERIAL_DARK.with_brand(..)`)
    and `ThemeConfig { light, dark, storage_key, ..Default::default() }`. Text gets the Material 3
    line heights, so multi-line text may become taller.
- `masamune-whisker-ui`: buttons, dialogs and bottom sheets take their corner radii from
  `shape()`.
- `masamune-whisker-ui`: added `TimePickerDialog`, the Material 3 time picker (Flutter's
  `showTimePicker`) drawn in Whisker, so it looks the same on iOS and Android. It has the dial
  (with the 24-hour double ring) and the keyboard input mode. `HourFormat` picks `HH` / `H` / `h`,
  `TimePickerLabels` takes the localized texts and `TimeOfDay` is the picked value. The crate now
  depends on `whisker-input`.
- `masamune-whisker-appearance`: `Palette` adds `surface_container_highest` (Material's
  `surfaceContainerHighest`).
- Added `masamune-whisker-localize`, which holds everything about languages:
  - `localize!` declares typed strings written as a Rust trait with one implementation per
    language (generates `LANGUAGES`, `register()` and `l()`).
  - `system_languages()` / `on_system_languages_changed()` / `follow_system()` read and follow the
    system languages on Android, iOS and the Web (`navigator.languages`, `languagechange`).
  - `language_name()` gives the native name of a language for language pickers.
  - Breaking: the language selection moved from `masamune-whisker-appearance`
    (`appearance::{LanguagePref, init_locale, change_language, …}` →
    `masamune_whisker_localize::…`), and `appearance::follow_system` no longer follows languages
    (call `masamune_whisker_localize::follow_system` as well). Saved preferences keep working.
  - Breaking: `masamune-whisker-localize-build` became the `build` feature
    (`masamune_whisker_localize::LocalizeBuilder`); generated code now reads
    `::masamune_whisker_localize`.
  - Breaking: `masamune-whisker-device-info` no longer reports `locales`; use
    `masamune_whisker_localize::system_languages()`.
- iOS: every native module now has an iOS implementation (a Swift package next to the Android
  sources; iOS 15 or later): admob, app-control, apple-sign-in, billing, date-picker,
  device-info, firebase-ai, firebase-app-check, firebase-firestore-db, firebase-remote-config,
  fonts, google-sign-in, image-picker, in-app-review, local-notification, share and
  view-capture. Firebase modules use firebase-ios-sdk 12.19.2 like `whisker-firebase`.
  - CNG plugins write the iOS `Info.plist` keys and entitlements: `MasamuneAdmob`
    (`GADApplicationIdentifier`, `SKAdNetworkItems`, tracking usage), `MasamuneImagePicker`
    (camera / photo library usage), `MasamuneFirebaseAppCheck` (App Attest),
    `MasamuneAppleSignIn` (Sign in with Apple), `MasamuneGoogleSignIn` (URL scheme from
    `REVERSED_CLIENT_ID`), `MasamuneAppControl` (`LSApplicationQueriesSchemes`) and
    `MasamuneLocalize` (`CFBundleLocalizations` and localized `InfoPlist.strings`).
  - `masamune-whisker-screen`: `ios_orientations` / `ipad_orientations` set the supported
    orientations; `configChanges` stay Android-only.
  - `masamune-whisker-admob`: added `request_tracking_authorization` (App Tracking
    Transparency; Android reports `Authorized`).
  - `masamune-whisker-local-notification`: added `permission_status()` /
    `PermissionStatus::NotDetermined` so apps can ask on iOS before showing a "blocked" state.
  - `masamune-whisker-boot`: iOS shows no "quit" button (apps must not close themselves there);
    failures offer only "retry" and the update dialog stays up after opening the store.
  - `masamune-whisker-ui`: `Scaffold`, `AppBar`, bottom sheets and `SafeArea` keep clear of the
    side safe areas (landscape iPhones).
- Breaking: `masamune-whisker-billing` has one API for Google Play Billing and StoreKit 2:
  `Product` / `Offer` / `PricingPhase`, `Purchase` (`token` is the purchase token on Android and
  the StoreKit 2 JWS on iOS), `purchase(product_id, offer, replacing)`, `query_purchases`,
  `finish` (acknowledge / `Transaction.finish`) and `on_purchases`. `ProductDetails`,
  `acknowledge` and the response-code constants were removed.
- Whisker comes from the [mathrunet/whisker](https://github.com/mathrunet/whisker) fork `main`
  through `[patch.crates-io]`; `scripts/whisker-fork.sh` checks the lock and installs the
  matching CLI. Added `scripts/check-ios.sh` and an iOS CI job.

## Unreleased

- Added `masamune-whisker-logger`: flavor-aware `log` output (dev: Debug+, prod: Warn+)
  with `LoggerAdapter`s that receive records, `send` events and `report_error` errors at
  once, plus `ConsoleLoggerAdapter` and `RuntimeLoggerAdapter`.
- Added `masamune-whisker-logger-firebase`: `FirebaseLoggerAdapter` sends events to
  Analytics and warnings, handled errors and panics to Crashlytics.
- Added `masamune-whisker-cached-image` with URL-keyed temporary files, physical-pixel
  decode bounds, a configurable 64 MiB decoded-image LRU, and cache removal APIs.
- Added `masamune-whisker-functions`, `masamune-whisker-functions-cloudflare` and
  `masamune-whisker-functions-firebase`: typed actions, injectable adapters, JSON REST,
  edge/region routing, callable error details, and runtime stubs.
- `masamune-whisker-workers`: added the SDK-independent `CallableClient` and custom
  request headers. Deadlines now include token acquisition. The legacy `Callable`
  keeps its API but now returns App Check acquisition errors instead of continuing
  without a required token; malformed callable responses also fail explicitly.
- Added `masamune-whisker-storage` (`StorageAdapter`, `LocalFile`, `RemoteFile`, `RuntimeStorageAdapter`),
  `masamune-whisker-storage-firebase` (Cloud Storage for Firebase) and
  `masamune-whisker-storage-cloudflare` (Cloudflare R2 through the `storage_cloudflare` worker).
- Added `masamune-whisker-ai-debugger`, the Whisker port of `masamune_ai_debugger`: a dev-only
  floating panel that sends instructions and screenshots to SamuraiAI, reports panics, handled
  errors and slow model loads / progress indicators as AI sessions, and offers debug login,
  forced purchases and debug camera pictures. It uses the same `/api/app-debug/*` API and Maestro
  identifiers as the Flutter package and does nothing in prod.
- `masamune-whisker-logger`: `start_trace` returns a guard that reports `trace_start` /
  `trace_stop` to adapters (default no-op methods); `RuntimeLoggerAdapter` records them as
  `LogEntry::TraceStart` / `TraceStop`.
- `masamune-whisker-model`: `CachedStore::load` / `query` run inside
  `masamune.model.load|<document|collection>|<load|query>|<database>.<table>` traces.
- `masamune-whisker-ui`: `CircularProgressIndicator` runs a `masamune.indicator.show|<trace_name>`
  trace while shown (new `trace_name` prop; `BusyHost` uses `busy`).
- `masamune-whisker-billing`: `debug_force_purchase` / `debug_force_unpurchase` simulate purchases
  for development; they reach `on_purchases` and `query_purchases`.
- `masamune-whisker-image-picker`: `set_debug_picture` makes the camera return a prepared image.
- `masamune-whisker-bridge`: `Subscription::new` wraps a value whose drop ends a subscription.

## 0.2.1

- `masamune-whisker-ui`: `Scaffold` paints the bottom safe area with the color of the bar above it
  (`bottom_navigation_bar_color`, `bottom_sheet_color`). `TabScaffold` takes `bar_color`.

## 0.1.0

- Initial release. Moved the native modules built for NANSURU into `masamune-whisker-*` crates:
  bridge, fonts, device-info, app-control, date-picker, view-capture, share, image-picker,
  local-notification, in-app-review, billing, admob, google-sign-in, apple-sign-in,
  firebase-app-check, firebase-remote-config, firebase-firestore-db, firebase-ai.
- Added the `whisker-router` fix that keeps tabs mounted when switching tabs inside a `Stack`.
