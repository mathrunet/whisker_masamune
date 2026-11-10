# masamune-whisker-cached-image

URL-keyed images with temporary-file persistence, physical-pixel decode bounds,
and a shared byte-bounded LRU of decoded images. Presentation uses `whisker-image`.

```rust,ignore
use whisker::prelude::*;
use masamune_whisker_cached_image::{CachedImage, configure_image_cache};

// Await before mounting images, for example in the application's boot task.
configure_image_cache(32 * 1024 * 1024).await?;

render! {
    CachedImage(
        src: "https://example.com/photo.jpg?v=2",
        decode_width: Some(400),
        decode_height: Some(300),
        style: css!(width: px(200), height: px(150)),
    )
}
```

The dimensions above are **physical pixels**, independent of layout units. Supply
layout size × display scale for a sharp image. Both bounds fit the source inside
the box while preserving aspect ratio; one bound infers the other. Sources are
never enlarged. Omit both for original resolution. Zero is invalid. An 800×600
image decoded at 400×300 retains about 480,000 bytes in RGBA, plus platform row
padding. `ImageMode::AspectFill` may need larger decode bounds when the source and
view have different aspect ratios. Animated sources use the first frame.

The lookup order is decoded memory, original temporary file, then HTTP(S).
Originals are keyed by the **exact input URL including query parameters** and
stored under its SHA-256 filename. Decode bounds identify variants beneath that
URL; all variants count against one application-wide budget, initially 64 MiB.
Headers are sent when fetching but do not change identity: use different URLs
for account-dependent content, or await `remove_cached_image(url)` when changing
its meaning.

Every memory hit makes an entry most recently used. Adding an entry or lowering
the budget evicts the least recently used entries until the budget fits. An
oversized image is displayed without retention; a zero budget disables memory
retention. Files survive memory eviction and process restarts until the OS clears
them. There is no expiration timer. The removal API deletes the original file and
all decoded variants; it does not remove images from currently visible views.

On iOS/Android, files live in the application's temporary directory under
`whisker-images`. Desktop uses the OS temporary directory with a hash of the
executable path to separate applications. Loading and cache control run off the
UI thread. A serial cache worker orders storage operations and prevents duplicate
concurrent downloads. Damaged files are removed and fetched once again. Downloads
are limited to 64 MiB; failed or invalid images are not committed.

The budget measures retained decoded images, **not process RSS**. Visible images,
GPU resources, compressed downloads and transient decode allocations are outside
it. Apple ImageIO and Android native decoders downsample where supported; Desktop
may temporarily decode the full source before resizing.

Web delegates to `<img>` and the browser HTTP cache. It cannot enforce temporary
files, decode bounds or this memory budget; cache configuration/removal returns
an error, and custom request headers retain `whisker-image`'s Web limitation.

## Dependency setup

Add this crate from the same `whisker_masamune` revision as the other Masamune
crates. The application root must patch `whisker-image` as well as its other
Whisker dependencies to the fork:

```toml
[patch.crates-io]
whisker-image = { git = "https://github.com/mathrunet/whisker", branch = "main" }
# Keep the other Whisker patches from the workspace's documented setup.
```
