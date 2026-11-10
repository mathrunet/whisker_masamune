# whisker-router ローカルパッチ

crates.io の `whisker-router 0.14.3`（上流 `packages/whisker-router`、commit `09aa265d`）をコピーし、
`whisker/Cargo.toml` の `[patch.crates-io]` で差し替えている。上流が同等の修正を出したら、このディレクトリと
`[patch.crates-io]`・`[workspace] exclude` の記述を削除して crates.io 版に戻す。

## 変更点

### Stack 内のタブ切替でタブ全体が作り直される（`src/render/node.rs`）

- 症状：`Stack { Route(layout) { Switch { … } } }` 構成でタブを切り替えると、外側の Stack が「replace」と判定し、
  ボトムナビを含むレイアウト全体（全タブ）を作り直して、最上位画面の遷移（Android 既定の `slide_fade`）を再生する。
  タブの状態保持（keep-alive）も失われる。
- 原因：`reconcile_stack` が `StackWrapper::fingerprint != entry.state` で入れ替えを判定しており、`entry.state` に
  中の Switch の選択や内側 Stack の履歴まで含まれていた。
- 修正：`same_entry` を追加し、エントリ自身の識別子（Route は path・params・location、Stack/Switch は path）だけで
  比べる。`navigation_changed`・`survivor_matches`・`needs_swap` の3か所を置き換えた。
- 回帰テスト：`src/render/tests.rs` の `tab_switch_inside_stack_keeps_tabs_mounted`。

### その他

- `Cargo.toml` の末尾に空の `[workspace]` を追加（このディレクトリ単体で `cargo test` するため）。
- `Cargo.toml` から `[package.metadata.whisker.module.platforms.web]` を削除。crates.io のパッケージには `web/` が
  含まれておらず、パス依存にすると whisker CLI がマニフェストの不在をエラーにするため（このアプリは Web をビルドしない）。

## テスト

```sh
cd whisker/patches/whisker-router
CARGO_TARGET_DIR=../../target/router-test cargo test --lib
```
