# Changelog

## 0.6.0

- `masamune-whisker-billing`: upgrade Google Play Billing from 7.1.1 to the Java-only Billing 8.3.0 artifact and adapt product queries to `QueryProductDetailsResult`, preserving the subscription bridge API.

- `masamune-whisker-ui`: added `InkWell` (Flutter's `InkWell`): a tappable box that shows a
  highlight and an ink splash growing from the touch point. `FilledButton`, `OutlinedButton`,
  `TextButton`, the `AppBar` back/close button, `AppBarAction`, `BottomNavigationBar`, `TabBar`,
  `SegmentedTabBar` and the dialog buttons now use it, so they show tap feedback.
- **Breaking** whisker requirements are raised to 0.17 (mathrunet/whisker fork main), which
  reports the pointer position inside the tapped element (`TouchEvent::offset` / `size`) and
  lets a route transition be translucent with a fading barrier
  (`RouteTransition::modal().barrier(color)`, whiskerrs/whisker#834). Swift Host SDK pins follow
  the same fork revision.
- `masamune-whisker-ui`: full-screen pages can be declared explicitly (Flutter's
  `TransitionQuery.fullscreen`). Open the route with `fullscreen_transition()` and wrap the page
  in `Fullscreen` (or call `provide_fullscreen()`); every `AppBar` inside shows a close button (X)
  at the top start by default, also while the page is loading and even with `back: true`.
  `use_fullscreen()` tells custom headers whether they are in a full-screen page, and the new
  `AppBar` prop `hide_leading` hides the leading button.
- **Breaking** `masamune-whisker-logger`: added `Flavor::Test` for automated test builds (E2E
  runs such as Maestro). `Flavor::from_env(Some("test"))` returns it and it logs at Debug like
  dev; `InvalidFlavor` now reads "expected dev, test or prod flavor". Exhaustive `match`es on
  `Flavor` need a `Test` arm.
- **Breaking** `masamune-whisker-ai-debugger`: the debugger is also enabled in debug builds with
  `Flavor::Test`. Emit the `masamune_ai_debugger` cfg from `build.rs` for every flavor except prod
  (dev and test).
- **Breaking** `masamune-whisker-ui`: `BottomNavigationBarItem` and `TabItem` have a new
  `identifier: Option<String>` field (set it with `.identifier(..)`); struct literals need it.
- **Breaking** `masamune-whisker-ui`: `TabScaffold` is removed. Build the tab layout like
  Flutter: create a `TabController` with `use_tab_controller(tabs)` (or `use_tab_controller_with`
  and `TabOptions`), pass `BottomNavigationBar(items: tabs.items(), selected: tabs.selected(),
  on_tap: tabs.on_tap())` to the `bottom_navigation_bar` of a `Scaffold` with
  `ModalLayerHost(layer: tabs.modal_layer())` as its `overlay`, and put `TabView(controller:
  tabs) { Outlet {} }` in its body. `TabItem::new` takes the URL of the tab's root screen as its
  second argument. Selecting a tab now always shows its root screen, also when switching from
  another tab (`TabOptions { reset_on_tab_change: false, .. }` keeps the screens left open in
  the tab).
- `masamune-whisker-ui`: widgets expose accessibility semantics for screen readers and UI
  automation. `FilledButton`, `OutlinedButton` and `TextButton` take `identifier: Option<String>`
  and are labelled with their text, with the button role and the disabled state. `AppBar` takes
  `identifier` (the title is a header; the back button is `<identifier>_back`, labelled "Back"),
  `AppBarAction` takes `identifier` and `label: Option<String>`, bottom navigation destinations
  are tabs with their selected state, and the dialog buttons are `dialog_submit` /
  `dialog_cancel` (`DIALOG_SUBMIT_IDENTIFIER` / `DIALOG_CANCEL_IDENTIFIER`).
- `masamune-whisker-ui`: a `ModalBottomSheet` on a tab screen now slides up over
  the bottom navigation bar instead of above it, and closes when the tab changes. Added
  `ModalLayer`, `provide_modal_layer` and `ModalLayerHost` to draw sheets over a larger area.
- `masamune-whisker-ui`: `Scaffold` takes `bottom_sheet_radius` to round the top corners of
  `bottom_sheet` (with its background and the safe area below it), and the sheet now shrinks
  when the body has no room left (for example with the keyboard open), so a sheet shown as a page
  can scroll its content instead of overflowing the top of the screen.
- `masamune-whisker-ui`: added `fade_upwards_transition()` (`FadeUpwardsTransition`), a
  full-screen page transition for `whisker-router` routes like Flutter's
  `FadeUpwardsPageTransitionsBuilder`: the screen rises slightly from below while fading in, and
  the screen beneath it stays still.
- Added `examples/kiwame_sample`: a minimal app with a test flavor, runtime adapters and
  accessibility identifiers for Maestro E2E flows.

## 0.5.0

- **Breaking** `masamune-whisker-ai-debugger`: `ai_debugger_config!` reads the
  `MASAMUNE_AI_DEBUGGER_*` values only when the app crate is compiled with the
  `masamune_ai_debugger` cfg. Emit it from `build.rs` for dev builds only; without it the
  debugger is disabled. Production debug builds no longer embed the API key or the accounts when
  the variables are set in the build environment.
- `masamune-whisker-ai-debugger`: when disabled, `AiDebugger` wraps its children in a flex
  container instead of a fragment, so it can stay the app root in production builds (a fragment
  root left the screen blank).
- `Scaffold`: added `safe_area_horizontal` (default `true`). With `false` the body runs edge to
  edge in landscape and the side safe areas are handed down through `use_scaffold_insets`.
- `TabScaffold`: tab screens are no longer padded by the side safe areas; each screen's own
  `Scaffold` keeps its content clear of them, so tab backgrounds reach the screen edges.

## 0.4.0

- Added `masamune-whisker-webview`: `MasamuneWebView` (`WKWebView` / `android.webkit.WebView`)
  for app shells around a web app, with scripts injected at document start (`user_scripts`),
  navigation restricted to `allowed_origins` (other links go to `on_external_navigation`),
  `window.open` / `target="_blank"` handed to `on_open_window`, camera / microphone grants for
  `getUserMedia`, `<input type="file">` on Android, recovery when the web content process ends,
  page messaging through `MasamuneWebViewRef` and `WebViewMasamuneAdapter` for app defaults
  (including `max_download_bytes`).
- Added `masamune-whisker-share-receive`: receives text, URLs and images shared by other apps
  (Android `ACTION_SEND` / `ACTION_SEND_MULTIPLE`; iOS Share Extension writing into an App Group),
  delivered once through `take_pending` / `latest_share`, with the `MasamuneShareReceive` CNG
  plugin and `ShareReceiveMasamuneAdapter`.

- Added `masamune-whisker-auth0`: Auth0 Universal Login (Auth0.swift 2.x / Auth0.Android 3.x)
  with credentials stored and renewed by the native credentials managers. `Auth0Adapter` with
  `NativeAuth0Adapter` and `RuntimeAuth0Adapter`, the `Auth0` session handle (`login`, `restore`,
  `access_token`, `logout`, `user_id`; implements `AuthAdapter`), `Auth0MasamuneAdapter` /
  `auth0_auth()`, `Auth0FunctionsAdapter` (adds `auth0_domain` / `auth0_access_token` to request
  bodies) and the `MasamuneAuth0` CNG plugin (iOS URL scheme, Android `auth0Scheme` /
  `auth0Domain` manifest placeholders).
- Added `masamune-whisker-auth0-firebase`: signs in to Firebase Auth with the Auth0 session through
  the `auth0_custom_token` callable (`Auth0CustomTokenAction`); `login`, `restore`, `logout`,
  `sign_in_firebase` and `Auth0FirebaseAuthAdapter`. The Functions backend is passed in by the app.

- Added `masamune-whisker-youtube`: `YouTubePlayer` hosts the YouTube IFrame Player API in a
  native web view (`WKWebView` / `android.webkit.WebView`) with `video_id`, `autoplay`,
  `loop_video` and `origin` props, ready / state / error events and a `YouTubePlayerRef`
  (play, pause, cue, load, seek, mute, unmute); `YouTube` is the 16:9 box of Masamune's
  `VideoWidget` (black until the ID is set and the page transition ended).
- Added `masamune-whisker-model-functions`: `FunctionsStore` reads and writes Firestore documents
  and collections through the `document_model_firestore` / `collection_model_firestore` /
  `aggregate_model_firestore` callables (Masamune's `FunctionsModelAdapter`) as a `RemoteStore`,
  with on-device query evaluation and `field_value` helpers for Masamune timestamps, references
  and counters.
- Added `masamune-whisker-storage-functions`: `FunctionsStorageAdapter` uploads, downloads and
  deletes files through the `storage_firebase` callable (Masamune's `FunctionsStorageAdapter`;
  `{method, path: "{bucket}/{path}", binary (base64), meta}`) as a `StorageAdapter`, with the
  public and download URLs and downloaded bytes cached in memory like `FunctionsStorageDatabase`.
- `masamune-whisker-model`: `CachedStore` keeps an in-memory layer for the current session
  (Masamune's `ref.app.model`). Cacheable rows are served from memory before the on-device cache,
  `CachedStore::peek` returns the last known value of any row synchronously (including rows
  rejected by the cache filter, and `Some(None)` for rows known not to exist), and
  `CacheOptions::with_memory(false)` turns the layer off. The memory is cleared by
  `set_session`, `clear_cache` and `clear_database`.

## 0.3.1

- Added `masamune-whisker-shimmer`: `ShimmerBox` and `ShimmerMultiLine` loading placeholders
  (katana_ui's shimmer widgets), plus `use_shimmer` / `shimmer_gradient` for custom ones.

## 0.3.0

- Added `masamune-whisker-adapter`: `MasamuneAdapter` / `MasamuneAdapters` register plugin
  settings and startup work in one place (Masamune's `masamuneAdapters`). `install` prepares the
  adapters in order and spawns their start work under the installing owner; `primary()` reads
  the installed settings.
- `masamune-whisker-local-notification`: added `LocalNotificationMasamuneAdapter`, which
  configures the channel while the app prepares and re-arms the saved schedules after startup.
- `masamune-whisker-billing`: added `BillingMasamuneAdapter` with the app's product ids;
  `load_products` (run after startup by the adapter) fills `products()` in the configured order.
  On Android, calls made while the store connection is being set up now wait for it instead of
  failing with `billing setup failed`.
- Added adapters to more crates (all existing functions keep working without them):
  - `AdmobMasamuneAdapter`: default banner and interstitial units (`default_banner_ad_unit_id`,
    `show_default_interstitial`) and an optional App Tracking Transparency request after startup.
  - `AiDebuggerMasamuneAdapter`: installs the panic hook once and hands the controller to the root.
  - `AppleSignInMasamuneAdapter` / `GoogleSignInMasamuneAdapter`: the scopes `sign_in` and `link`
    request (unchanged defaults).
  - `FirebaseAiMasamuneAdapter`: the model used by requests whose `model` is empty.
  - `FirebaseAppCheckMasamuneAdapter`: activates App Check while the app prepares.
  - `FirebaseRemoteConfigMasamuneAdapter`: the intervals `fetch` uses, optionally fetched after
    startup.
  - `ImagePickerMasamuneAdapter`: the size `pick` scales images down to.
  - `InAppReviewMasamuneAdapter`: the store pages `review` falls back to.
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
- **Breaking:** removed `masamune-whisker-workers`. Its parts moved to base crates
  modeled after Flutter Masamune's `katana_auth` / `katana_functions`, so backend
  crates depend only on interfaces:
  - Added `masamune-whisker-auth` (`AuthAdapter`, `Auth`, `RuntimeAuthAdapter`,
    `AuthError`) and `masamune-whisker-auth-firebase` (`FirebaseAuthAdapter`,
    `firebase_auth()`, `current_uid()`).
  - `masamune-whisker-functions` now owns `ApiError`, `ApiResult`, `RetryPolicy`,
    `retry`, `with_timeout`, `client()`, `read_json` and `CallableFailure`, and adds
    `Functions::send`, `FunctionRequest::with_segments` and builder helpers, and
    `ApiError: From<FunctionsError>`.
  - `CallableClient` moved to `masamune-whisker-functions-firebase`; its `firebase`
    feature provides `with_firebase_providers()` and `firebase_app_check_auth()`.
    The legacy `Callable` was removed; use `FirebaseFunctionsAdapter`.
  - `masamune-whisker-functions-cloudflare` dropped its `firebase-auth` feature;
    inject `firebase_auth()` (or any `AuthAdapter`) with `with_auth`.
  - `RestDatabase::new`, `Turso::new` / `with_config`, `Tidb::new` / `with_config` and
    `CloudflareStorageAdapter::new` take a `Functions` handle instead of `Workers`;
    `RestDatabase::workers()` became `functions()` / `request()` / `send()`.
    `masamune-whisker-storage-cloudflare::map_error` accepts `FunctionsError` too.
  - `masamune-whisker-model` re-exports `ApiError`, `ApiResult`, `RetryPolicy`,
    `retry` and `with_timeout`. `masamune-whisker-boot`'s `workers` feature is now
    `functions`.
  - Migration: `Workers::new(url).with_auth(AuthProvider::firebase())` →
    `Functions::new(CloudflareFunctionsAdapter::new(url).with_auth(firebase_auth()))`;
    `AuthProvider::new` / `fixed` / `none` → `Auth::from_fn` / `fixed` / `none`;
    `Callable::new(project, region)` →
    `FirebaseFunctionsAdapter::new(project, region).with_firebase_providers()`.
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
