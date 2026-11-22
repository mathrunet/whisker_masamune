# masamune-whisker-deeplink

Receive custom URL scheme links, Universal Links (iOS) and App Links (Android) in a Whisker app,
at launch and while it runs (the counterpart of Masamune's `masamune_deeplink`).

- **iOS**: the module hooks the app's scene delegate (`scene(_:willConnectTo:options:)`,
  `scene(_:openURLContexts:)`, `scene(_:continue:)`) when it is registered at launch, so the
  generated project needs no changes.
- **Android**: `ACTION_VIEW` intents of `MainActivity` (the launch intent and `onNewIntent`).

## Setup

Register the schemes and domains with the CNG plugin in `whisker.rs`:

```rust,ignore
use masamune_whisker_deeplink::cng::MasamuneDeeplink;

app.project_plugin::<MasamuneDeeplink>(|c| {
    c.scheme("segue")                      // segue://…
        .app_link("segue.day", Some("/app/")); // https://segue.day/app/…
});
```

| Platform | What the plugin writes | What you provide |
|---|---|---|
| iOS | `CFBundleURLTypes` (schemes), `com.apple.developer.associated-domains` = `applinks:<host>` | `https://<host>/.well-known/apple-app-site-association`; the Associated Domains capability on the App ID (automatic signing enables it) |
| Android | `VIEW` + `BROWSABLE` intent filters on `.MainActivity` (`android:autoVerify="true"` for app links, `https`, host, `pathPrefix`) and `android:launchMode="singleTask"` | `https://<host>/.well-known/assetlinks.json` with the signing certificate fingerprints |

Restrict a scheme to some hosts, or an app link to one platform:

```rust,ignore
app.project_plugin::<MasamuneDeeplink>(|c| {
    c.scheme_hosts("segue", ["track", "playlist"]) // segue://track/…, segue://playlist/…
        .android_app_link("segue.day", Some("/app/")) // Android App Link only
        .ios_app_link("links.segue.day");            // iOS Universal Link only
});
```

- `scheme_hosts` writes one `<data android:scheme android:host>` per host in the scheme's intent
  filter. iOS cannot restrict a URL scheme to hosts, so the scheme opens any host there; filter
  the received links by `Link::host` when that matters.
- `app_link` targets both platforms. `android_app_link` adds only the Android intent filter and
  `ios_app_link` only the `applinks:` associated domain (`AppLink::ios` / `AppLink::android`).

Re-run `whisker run` after changing `whisker.rs`.

## Usage

With the adapter, links are filtered and published to a signal:

```rust,ignore
use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_deeplink::{DeeplinkConfig, DeeplinkMasamuneAdapter, latest_link, take_pending};

MasamuneAdapters::new()
    .adapter(DeeplinkMasamuneAdapter::new(DeeplinkConfig {
        schemes: vec!["segue".into(), "https".into()],
        hosts: vec!["segue.day".into()],
    }))
    .install();

// In a component: navigate when a link arrives.
effect(move || {
    if latest_link().get().is_some() && let Some(link) = take_pending() {
        // link.scheme, link.host, link.segments(), link.query_value("ref")
    }
});
```

Without the adapter, use the functions directly:

```rust,ignore
if let Some(url) = masamune_whisker_deeplink::get_initial_link().await { /* … */ }
let subscription = masamune_whisker_deeplink::on_link(|url| { /* … */ });
```

`parse_link` splits a URL into scheme, host, decoded path segments, query parameters and fragment.

## Behavior and limits

- Each link is delivered once. The launch link comes from `get_initial_link` (the adapter reads
  it at startup), later links from `on_link`.
- iOS links that arrive before the first `on_link` subscription are queued and delivered when it
  starts.
- Apps without a scene manifest (no `UIApplicationSceneManifest`) are not supported on iOS; Whisker
  apps always have one.
- The OS decides which links open the app. A custom scheme can be sent by any app, so check links
  with `DeeplinkConfig` and treat their content as untrusted input.
