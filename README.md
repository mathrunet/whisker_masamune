# whisker_masamune

Native modules and app foundations for [Whisker](https://whisker.rs) apps, extracted from the
apps built with the Masamune framework. Every crate is published from this workspace as
`masamune-whisker-*`.

Whisker discovers native modules through `cargo metadata`, so the Android (and later iOS)
sources inside each crate are picked up whether the crate comes from a path, a git repository
or crates.io.

## Crates

| Crate | Description |
|---|---|
| `masamune-whisker-bridge` | Shared JSON bridge used by every native module (Rust only) |
| `masamune-whisker-fonts` | Register bundled fonts by family name |
| `masamune-whisker-device-info` | Locale, screen metrics, time zone and app version |
| `masamune-whisker-app-control` | Open URLs externally and close the app |
| `masamune-whisker-date-picker` | Native date and time pickers |
| `masamune-whisker-view-capture` | Capture a view subtree as PNG |
| `masamune-whisker-share` | OS share sheet |
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

| Crate | Description |
|---|---|
| `masamune-whisker-workers` | HTTP client for Masamune Cloudflare Workers and Firebase callable functions |
| `masamune-whisker-model` | Query types, REST database client, on-device libsql cache and `CachedStore` |
| `masamune-whisker-turso` | Turso through the Masamune worker, with the libsql cache |
| `masamune-whisker-tidb` | TiDB through the Masamune worker (counts, vector search, bulk writes), with the libsql cache |
| `masamune-whisker-firestore` | Firestore reads/listeners with cache policies and UI signals |
| `masamune-whisker-appearance` | Light/dark/system themes, languages, typography and app-wide rebuilds |
| `masamune-whisker-localize-build` | Build-script generator of typed `l()` tables from localize YAML |
| `masamune-whisker-screen` | Screen size/orientation signals, orientation lock and `configChanges` CNG plugin |
| `masamune-whisker-boot` | Boot screen with startup work, retry/quit and forced updates |
| `masamune-whisker-ui` | Material-style widgets (Scaffold, AppBar, navigation, sheets, dialogs, buttons) |

Android is implemented for all native modules today. The Rust APIs are platform-neutral so
iOS and Web hosts can be added without breaking apps.

## Usage

Depend on a release tag:

```toml
[dependencies]
masamune-whisker-billing = { git = "https://github.com/mathrunet/whisker_masamune", tag = "v0.1.0" }

# Required by the tab/scaffold widgets: keeps tabs mounted when switching tabs inside a Stack.
[patch.crates-io]
whisker-router = { git = "https://github.com/mathrunet/whisker_masamune", tag = "v0.1.0" }
```

### Developing against a local checkout

Point the git dependencies at a local clone with a git-ignored `.cargo/config.toml` in the app
(the same idea as `pubspec_overrides.yaml` in Flutter). Paths are relative to the directory
that contains `.cargo/`:

```toml
[patch."https://github.com/mathrunet/whisker_masamune"]
masamune-whisker-billing = { path = "../whisker_masamune/packages/masamune-whisker-billing" }

# A git `[patch]` cannot be patched again, so override the crates-io patch directly.
[patch.crates-io]
whisker-router = { path = "../whisker_masamune/patches/whisker-router" }
```

While the override is active, `Cargo.lock` records the path sources. Remove the override and
run `cargo update -p <crate>` before committing the app.

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo test --workspace
scripts/check-android.sh   # cross-check for aarch64-linux-android with the NDK toolchain
```

## License

MIT
