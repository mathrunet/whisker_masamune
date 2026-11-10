//! URL-keyed images backed by temporary files and a byte-bounded decoded-image LRU.
//!
//! `CachedImage` uses native image presentation from `whisker-image`. Set decode
//! bounds in physical pixels to retain only the resolution the view needs.
//! Original files are shared by URL; decoded sizes share a default 64 MiB budget.
//! Web falls back to browser image loading without enforceable cache controls.

use whisker::prelude::*;
use whisker::runtime::view::Element;
use whisker_image::{Image, ImageCacheOptions};
pub use whisker_image::{ImageCallback, ImageEvent, ImageMode};
pub use whisker_image::{configure_image_cache, remove_cached_image};

/// Display a cached image with optional physical-pixel decode bounds.
///
/// Both bounds preserve aspect ratio inside the requested box without upscaling.
/// A single bound infers the other. Omitted bounds retain the source resolution.
/// Headers are sent on a cache miss but do not affect the URL key. Use separate
/// URLs for resources that differ by account or authorization.
#[component]
pub fn cached_image(
    /// Exact URL, including its query string.
    src: Signal<String>,
    /// Maximum decoded width in physical pixels; zero is invalid.
    #[prop(default = None.into())]
    decode_width: Signal<Option<u32>>,
    /// Maximum decoded height in physical pixels; zero is invalid.
    #[prop(default = None.into())]
    decode_height: Signal<Option<u32>>,
    /// Display fit, independent of the decode bounds.
    #[prop(default = ImageMode::default().into())]
    mode: Signal<ImageMode>,
    /// Request headers as a JSON object of strings.
    #[prop(default = String::new().into())]
    headers: Signal<String>,
    /// Layout and presentation styles.
    style: Option<whisker::Style>,
    /// Receives the decoded pixel dimensions.
    on_load: Option<ImageCallback>,
    /// Receives download, storage or decode failures.
    on_error: Option<ImageCallback>,
) -> Element {
    let options = computed(move || {
        Some(ImageCacheOptions {
            decode_width: decode_width.get(),
            decode_height: decode_height.get(),
        })
    });
    let style_prop = style.clone().unwrap_or_default();
    let load = on_load.clone();
    let error = on_error.clone();
    Image::builder()
        .src(src)
        .mode(mode)
        .headers(headers)
        .cache(options)
        .style(style_prop)
        .on_load(move |event| {
            if let Some(callback) = &load {
                callback.call(event);
            }
        })
        .on_error(move |event| {
            if let Some(callback) = &error {
                callback.call(event);
            }
        })
        .build()
}
