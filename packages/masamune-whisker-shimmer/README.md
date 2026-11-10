# masamune-whisker-shimmer

Shimmer loading placeholders, modeled on katana_ui's `ShimmerBox` / `ShimmerMultiLine` and the
`shimmer` Flutter package. A highlight band sweeps across the box every `period_ms`
(1500 ms by default).

```rust,ignore
use whisker::prelude::*;
use masamune_whisker_shimmer::{ShimmerBox, ShimmerMultiLine};

render! {
    // Fixed size.
    ShimmerBox(width: Some(120.0), height: Some(16.0))
    // Fills the parent's main axis (no `height`), with rounded corners and an inset.
    ShimmerBox(padding: 4.0, border_radius: Some(8.0))
    // Three text-like lines.
    ShimmerMultiLine(line_count: 3, height: Some(12.0))
}
```

Colors default to opaque blends of the theme's `text` over `background` (8% and 22%), so
the box looks the same on any surface. Pass `base_color` / `highlight_color` to override, and
`enabled: false` to draw a still box in the base color.

The band is a moving linear gradient drawn as the box background, so it is clipped by the
border radius on every host and needs neither `overflow` nor transforms. Unlike Flutter's
`Shimmer(child)` (a shader mask), it cannot shine through arbitrary children such as text:
build placeholders from boxes instead.

For custom placeholders, drive your own views with `use_shimmer` and `shimmer_gradient`.
