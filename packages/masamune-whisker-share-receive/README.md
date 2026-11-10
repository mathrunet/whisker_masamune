# masamune-whisker-share-receive

Receive text, URLs and images that other apps share with a Whisker app.

- **Android**: `ACTION_SEND` (`text/plain`, `image/*`) and `ACTION_SEND_MULTIPLE` (`image/*`) on the
  main activity. Shared streams (`content://`, `file://`) are copied into
  `<cacheDir>/masamune_whisker_share_receive/`.
- **iOS**: a Share Extension writes the shared items into an App Group container and closes at once;
  the app picks them up at launch and whenever it becomes active, moving the images into
  `Caches/masamune_whisker_share_receive/`. No URL scheme is used.

Each share is delivered once. Image paths are absolute; the app may move or delete the files
(Android removes copies older than a day).

## Setup

Register the CNG plugin in `whisker.rs`:

```rust,ignore
use masamune_whisker_share_receive::cng::MasamuneShareReceive;

app.project_plugin::<MasamuneShareReceive>(|c| {
    c.app_group("group.net.mathru.samurai"); // defaults to `group.<app bundle id>`
    // c.extension_suffix("ShareExtension");  // extension bundle id: `<app bundle id>.<suffix>`
    // c.display_name("SamuraiAI");           // name in the share sheet (defaults to the app's)
    // c.max_images(10);                      // most images in one share on iOS
});
```

The plugin also runs with these defaults when whisker.rs does not mention it. It adds the intent
filters on Android and sets the main activity's `launchMode` to `singleTask` (kept when it is
already `singleTask`, `singleInstance` or `singleInstancePerTask`), so a share reaches the running
app through `onNewIntent`. On iOS it adds the Share Extension
target, embeds it in the app, adds the App Group entitlement to both, and records the group in
both `Info.plist` files (`MasamuneShareReceiveAppGroup`, `cng::APP_GROUP_INFO_KEY`). Register the App Group for both
App IDs in the developer account (automatic signing does this). Re-run `whisker run` after
changing `whisker.rs`.

## Usage

With the Masamune adapters (filtering included):

```rust,ignore
use masamune_whisker_adapter::MasamuneAdapters;
use masamune_whisker_share_receive::{
    ShareReceiveConfig, ShareReceiveMasamuneAdapter, latest_share, take_pending,
};

MasamuneAdapters::new()
    .adapter(ShareReceiveMasamuneAdapter::new(ShareReceiveConfig {
        ignore_prefixes: vec!["samurai://".into()],
        ..Default::default() // jpg/jpeg/png/gif/webp, at most 10000 characters
    }))
    .install();

// In a component: react to new shares.
let latest = latest_share(); // ReadSignal<Option<SharedPayload>>
// …or take the pending one (clears `latest_share`).
if let Some(share) = take_pending() {
    // share.text, share.image_paths
}
```

`prepare` subscribes to running-app shares once per runtime and `start` reads the launch share.
Payloads pass through `filter_payload`: empty text, text starting with an ignored prefix, text
longer than `max_text_len` (dropped with a warning) and images with other extensions are removed;
a payload with nothing left is ignored. A newer share replaces an unread one.

Without the adapter:

```rust,ignore
use masamune_whisker_share_receive::{get_initial_share, on_share};

// Unfiltered payloads; keep the subscription alive.
let _subscription = on_share(|share| { /* shared while running */ });
whisker::spawn_local(async {
    if let Some(share) = get_initial_share().await { /* shared at launch */ }
});
```

`get_initial_share` is asynchronous: on Android it waits for the host activity (the first render
runs before the activity is attached) and copies the images off the UI thread.
