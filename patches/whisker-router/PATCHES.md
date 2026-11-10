# whisker-router local patch

A copy of `whisker-router 0.14.3` from crates.io (upstream `packages/whisker-router`, commit `09aa265d`).
Apps swap it in through `[patch.crates-io]` in the app's `Cargo.toml`:

```toml
[patch.crates-io]
whisker-router = { git = "https://github.com/mathrunet/whisker_masamune", tag = "..." }
```

Once upstream ships an equivalent fix, remove the `[patch.crates-io]` entry (and any related `[workspace] exclude`
entry) from the app's `Cargo.toml` to go back to the crates.io release.

## Changes

### Switching tabs inside a Stack rebuilds every tab (`src/render/node.rs`)

- Symptom: with a `Stack { Route(layout) { Switch { … } } }` layout, switching tabs makes the outer Stack treat it as a
  "replace", so it rebuilds the whole layout including the bottom navigation (every tab) and replays the top-level
  screen transition (`slide_fade` by default on Android). Tab state (keep-alive) is lost as well.
- Cause: `reconcile_stack` decided on a swap with `StackWrapper::fingerprint != entry.state`, and `entry.state` also
  included the selection of the inner Switch and the history of inner Stacks.
- Fix: added `same_entry`, which compares only the entry's own identity (path, params and location for a Route; path
  for a Stack/Switch). It replaces the comparison in three places: `navigation_changed`, `survivor_matches` and
  `needs_swap`.
- Regression test: `tab_switch_inside_stack_keeps_tabs_mounted` in `src/render/tests.rs`.

### Other

- Added an empty `[workspace]` at the end of `Cargo.toml` so this directory can run `cargo test` on its own.
- Removed `[package.metadata.whisker.module.platforms.web]` from `Cargo.toml`. The crates.io package does not include
  `web/`, and as a path dependency the whisker CLI reports the missing manifest as an error (the web target is not
  built).

## Tests

```sh
cd patches/whisker-router && cargo test --lib
```
