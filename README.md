# whisker_masamune

Native modules and app foundations for [Whisker](https://whisker.rs) apps, extracted from the
apps built with the Masamune framework. Every crate is published from this workspace as
`masamune-whisker-*`.

Whisker discovers native modules through `cargo metadata`, so the Android (Kotlin) and iOS
(Swift package) sources inside each crate are picked up whether the crate comes from a path, a
git repository or crates.io.

## Crates

| Crate | Description |
|---|---|
| `masamune-whisker-bridge` | Shared JSON bridge used by every native module (Rust only) |
| `masamune-whisker-fonts` | Register bundled fonts by family name |
| `masamune-whisker-device-info` | Screen metrics, time zone, app version and appearance settings |
| `masamune-whisker-app-control` | Open URLs externally and close the app |
| `masamune-whisker-date-picker` | Native date and time pickers |
| `masamune-whisker-view-capture` | Capture a view subtree as PNG |
| `masamune-whisker-share` | OS share sheet |
| `masamune-whisker-youtube` | [YouTube player (IFrame Player API in a native web view) and a 16:9 `YouTube` box](packages/masamune-whisker-youtube/README.md) |
| `masamune-whisker-image-picker` | Gallery / camera picker with resizing |
| `masamune-whisker-local-notification` | Daily scheduled local notifications |
| `masamune-whisker-in-app-review` | In-app review with store fallback |
| `masamune-whisker-billing` | In-app subscriptions |
| `masamune-whisker-admob` | AdMob banners and full-screen ads |
| `masamune-whisker-google-sign-in` | Google Sign-In (Firebase Auth) |
| `masamune-whisker-apple-sign-in` | Sign in with Apple (Firebase Auth) |
| `masamune-whisker-firebase-app-check` | Firebase App Check |
| `masamune-whisker-firebase-remote-config` | Firebase Remote Config |
| `masamune-whisker-firebase-firestore-db` | Cloud Firestore on named databases |
| `masamune-whisker-firebase-ai` | Firebase AI Logic (Gemini) |

### Foundations

Application logs use [`masamune-whisker-logger`](packages/masamune-whisker-logger/README.md): initialize with the app flavor (`dev`, `test` or `prod`) before startup work; Debug/Info are disabled in prod. Register adapters with `LoggerConfig` to send records, events and errors to every output at once, such as Firebase Analytics/Crashlytics via [`masamune-whisker-logger-firebase`](packages/masamune-whisker-logger-firebase/README.md). Never log credentials or personal data.

| Crate | Description |
|---|---|
| `masamune-whisker-auth` | [`AuthAdapter` interface and `Auth` token handle](packages/masamune-whisker-auth/README.md) |
| `masamune-whisker-auth-firebase` | [Firebase Authentication `AuthAdapter` (ID tokens, UID)](packages/masamune-whisker-auth-firebase/README.md) |
| `masamune-whisker-auth0` | [Auth0 Universal Login with Keychain/Keystore credentials, `Auth0` session handle (`AuthAdapter`), runtime adapter, Functions body injection and URL scheme CNG plugin](packages/masamune-whisker-auth0/README.md) |
| `masamune-whisker-auth0-firebase` | [Firebase Auth sign-in with an Auth0 session (`auth0_custom_token` exchange): login, restore, logout](packages/masamune-whisker-auth0-firebase/README.md) |
| `masamune-whisker-functions` | [Typed server actions, the shared HTTP foundation (`ApiError`, retry), JSON REST and runtime adapters](packages/masamune-whisker-functions/README.md) |
| `masamune-whisker-functions-cloudflare` | [Cloudflare Functions adapter with edge/region routing](packages/masamune-whisker-functions-cloudflare/README.md) |
| `masamune-whisker-functions-firebase` | [Firebase callable Functions adapter with Auth and App Check](packages/masamune-whisker-functions-firebase/README.md) |
| `masamune-whisker-model` | Query types, REST database client, on-device libsql cache and `CachedStore` |
| `masamune-whisker-turso` | Turso through the Masamune worker (any `FunctionsAdapter`), with the libsql cache |
| `masamune-whisker-tidb` | TiDB through the Masamune worker (any `FunctionsAdapter`; counts, vector search, bulk writes), with the libsql cache |
| `masamune-whisker-firestore` | Firestore reads/listeners with cache policies and UI signals |
| `masamune-whisker-model-functions` | [Firestore documents/collections through the Masamune model Functions callables](packages/masamune-whisker-model-functions/README.md) (any `FunctionsAdapter`; Masamune's `FunctionsModelAdapter`), with the libsql cache |
| `masamune-whisker-storage` | `StorageAdapter` trait (upload, download, delete, public and download URLs), file types and an in-memory adapter |
| `masamune-whisker-storage-firebase` | Cloud Storage for Firebase as a `StorageAdapter` (needs `whisker-firebase-storage` in the app's `[patch.crates-io]`) |
| `masamune-whisker-storage-cloudflare` | Cloudflare R2 through the Masamune storage worker (any `FunctionsAdapter`) as a `StorageAdapter` |
| `masamune-whisker-storage-functions` | [Files through the Masamune storage Functions callable](packages/masamune-whisker-storage-functions/README.md) (`storage_firebase`; any `FunctionsAdapter`; Masamune's `FunctionsStorageAdapter`) as a `StorageAdapter` |
| `masamune-whisker-appearance` | App-wide Material theme (`ThemeConfig`: palettes, typography, shapes, spacing, elevation), light/dark/system modes and app-wide rebuilds |
| `masamune-whisker-localize` | Everything about languages: system languages (Android, iOS, Web), system or pinned language saved across launches, typed strings in Rust (`localize!`) and the localize YAML generator (feature `build`) |
| `masamune-whisker-screen` | Screen size/orientation signals, orientation lock and `configChanges` CNG plugin |
| `masamune-whisker-adapter` | [Plugin settings and startup work registered in one place](packages/masamune-whisker-adapter/README.md) (Masamune's `MasamuneAdapter`) |
| `masamune-whisker-boot` | Boot screen with startup work, retry/quit and forced updates |
| `masamune-whisker-cached-image` | URL-keyed temporary images, sized decoding and a byte-bounded memory LRU ([usage](packages/masamune-whisker-cached-image/README.md)) |
| `masamune-whisker-ui` | Material-style widgets (Scaffold, AppBar, navigation, sheets, dialogs, buttons, Material 3 time picker) with accessibility labels, roles and `identifier` props for UI automation such as Maestro ([usage](packages/masamune-whisker-ui/README.md)) |
| `masamune-whisker-shimmer` | Shimmer loading placeholders (`ShimmerBox`, `ShimmerMultiLine`) ([usage](packages/masamune-whisker-shimmer/README.md)) |
| `masamune-whisker-ai-debugger` | [AI debugger overlay for dev and test builds](packages/masamune-whisker-ai-debugger/README.md): instructions, screenshots, crashes and slow operations to SamuraiAI, plus debug login, purchases and camera |

Every native module is implemented for Android and iOS (iOS 15 or later) behind the same Rust
API. Where a platform has no equivalent, the function's documentation says what it does there
(for example `exit_app` does nothing on iOS, where apps must not close themselves).

### iOS setup

Some modules need `Info.plist` keys or entitlements on iOS. Each of those crates ships a CNG
plugin to configure them from `whisker.rs`:

```rust
use masamune_whisker_admob::cng::MasamuneAdmob;
use masamune_whisker_apple_sign_in::cng::MasamuneAppleSignIn;
use masamune_whisker_firebase_app_check::cng::MasamuneFirebaseAppCheck;
use masamune_whisker_google_sign_in::cng::MasamuneGoogleSignIn;
use masamune_whisker_image_picker::cng::MasamuneImagePicker;
use masamune_whisker_localize::cng::MasamuneLocalize;

app.plugin::<MasamuneAdmob>(|c| {
    c.ios_app_id("ca-app-pub-xxxxxxxx~yyyyyyyy")
        .tracking_usage_description("Used to show ads that fit you.");
});
app.plugin::<MasamuneImagePicker>(|c| {
    c.camera_usage("Take a profile photo.").photo_library_usage("Pick a profile photo.");
});
app.project_plugin::<MasamuneFirebaseAppCheck>(|c| { c.app_attest_environment("production"); });
app.project_plugin::<MasamuneAppleSignIn>(|_| {});
// Registers REVERSED_CLIENT_ID from GoogleService-Info.plist as a URL scheme.
app.project_plugin::<MasamuneGoogleSignIn>(|c| { c.ios_config("firebase/GoogleService-Info.plist"); });
// Declares the app's languages so iOS system UI (pickers, share sheets, prompts) follows them.
app.project_plugin::<MasamuneLocalize>(|c| {
    c.strings("en", "CFBundleDisplayName", "My App").strings("ja", "CFBundleDisplayName", "マイアプリ");
});
```

`masamune-whisker-screen` restricts the iOS orientations (`ios_orientations` /
`ipad_orientations`). Firebase modules use firebase-ios-sdk 12.19.2, the version
`whisker-firebase` pins.

## Usage

Depend on a release tag:

```toml
[dependencies]
masamune-whisker-billing = { git = "https://github.com/mathrunet/whisker_masamune", tag = "v0.7.2" }

# Use the same fork main source as the other whisker crates, including the router fixes.
[patch.crates-io]
whisker-router = { git = "https://github.com/mathrunet/whisker", branch = "main" }
```

### Developing against a local checkout

Point the git dependencies at a local clone with a git-ignored `.cargo/config.toml` in the app
(the same idea as `pubspec_overrides.yaml` in Flutter). Paths are relative to the directory
that contains `.cargo/`:

```toml
[patch."https://github.com/mathrunet/whisker_masamune"]
masamune-whisker-billing = { path = "../whisker_masamune/packages/masamune-whisker-billing" }

# The router stays on the whisker fork main declared in Cargo.toml.
```

While the override is active, `Cargo.lock` records the path sources. Remove the override and
run `cargo update -p <crate>` before committing the app.

### Adapters

List the plugins' settings once, like `masamuneAdapters` in Flutter Masamune, and install them
while preparing the app. Each adapter prepares itself (for example the notification channel)
and starts its background work (for example re-arming saved notifications); plugins read their
settings with `XxxMasamuneAdapter::primary()`.

Crates with an adapter: admob, ai-debugger, apple-sign-in, auth0, billing, firebase-ai,
firebase-app-check, firebase-remote-config, google-sign-in, image-picker, in-app-review and
local-notification. share and view-capture have no runtime settings, and build-time settings stay
in their CNG plugins.

```rust
#[whisker::main]
fn app() -> Element {
    logger::init();
    MasamuneAdapters::new()
        .adapter(LocalNotificationMasamuneAdapter::new(NotificationConfig { /* … */ }))
        .adapter(BillingMasamuneAdapter::new(["monthly", "yearly"]))
        .install();
    render! { Root }
}
```

### Localization

Write the strings as a trait with one implementation per language; a missing string does not
compile. The first language is the fallback.

```rust
// src/i18n.rs
pub trait Strings {
    fn ok(&self) -> &'static str;
    fn hello(&self, name: &str) -> String;
}
pub struct En;
impl Strings for En { /* … */ }
pub struct Ja;
impl Strings for Ja { /* … */ }

masamune_whisker_localize::localize! { Strings; "en_US" => En, "ja_JP" => Ja }

// Startup
i18n::register();
masamune_whisker_localize::init_locale(&masamune_whisker_localize::system_languages(), None);
masamune_whisker_localize::follow_system();

// Screens: i18n::l().hello("Taro")
// Settings: masamune_whisker_localize::change_language(LanguagePref::Fixed("ja".into()))
```

Language changes rebuild the UI under `masamune_whisker_appearance::AppRoot`. On Android, add
`ConfigChanges::LOCALE` (masamune-whisker-screen) so a system language change does not restart
the app. Apps ported from Flutter can keep their localize YAML with
`masamune-whisker-localize = { default-features = false, features = ["build"] }` in
`[build-dependencies]` and `LocalizeBuilder` in `build.rs`.

## Development

The workspace (and the apps using it) builds against the
[mathrunet/whisker](https://github.com/mathrunet/whisker) fork `main`: every whisker crate is
redirected with `[patch.crates-io]` using the same source string.

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
scripts/check-android.sh   # cross-check for aarch64-linux-android with the NDK toolchain
scripts/check-ios.sh       # cargo check for the iOS targets + build every Swift package (macOS)
scripts/whisker-fork.sh check   # every whisker crate in Cargo.lock comes from one fork revision
scripts/whisker-fork.sh cli     # install the whisker CLI of that revision
```

Apps copy `scripts/whisker-fork.sh` and run the same `check` / `cli` steps in CI.

## License

MIT

## Security checks

`PATH=<python-3.11-or-newer-bin>:$PATH scripts/whisker-fork.sh check` verifies both Rust
and the standalone Swift manifests use the same fork revision. When intentionally updating
Whisker after its fixes merge, run `cargo update -p whisker`, update the Swift manifest revision
pins to the new lock revision, prepare the matching CLI and regenerate native app projects.
Generated CNG applications must use the local Host SDK at that same lock revision.
Standalone Swift manifests also use the fork, so they no longer silently build upstream 0.1.19.

`python3 scripts/test-release-security.py` checks debug and release behavior and scans the
executables for dummy credential sentinels. `JAVA_HOME=<Android-Studio-JBR> python3
scripts/test-native-security.py` executes the native file, URI and origin policies using
installed Swift and cached Kotlin tooling. Set `SECURITY_ANDROID_SERIAL` to additionally
execute them on a connected Android device/emulator. No dependencies are installed.

Security changes require explicit dev flavor for AI Debugger, deny native WebView privileges
without explicit trusted origins, and reauthorize every Functions storage read/URL lookup
(no cross-operation metadata/byte cache). Capture filenames are single ASCII basenames of
1–128 bytes, starting with a letter or digit; parent traversal and escaping symlinks are rejected.
Android shared streams must use `content://` URI grants. Release forced-purchase APIs are
inert and never create entitlements.

Account binding for billing (`obfuscatedAccountId` / `appAccountToken`) remains separate work:
applications and receipt-verification servers must agree on stable pseudonymous IDs, UUID
mapping for Apple, and anonymous-purchase/account-switch behavior before enabling it.
The build-only unmaintained `unsafe-libyaml` dependency is unchanged. Whisker framework
WebView, deep-link and cleartext fixes remain tracked by the separate framework audit.
