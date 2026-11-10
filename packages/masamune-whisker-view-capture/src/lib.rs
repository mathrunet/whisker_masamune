//! 子要素ごと画面を PNG 画像に保存する要素。
//!
//! `ElementRef::command("capture", {fileName, heightDp?})` でキャッシュに保存し、
//! `captured`（`{path}` か `{error}`）が届く。`heightDp` を渡すと上から切り取る。

use whisker::event::CustomEvent;

#[doc(hidden)]
#[whisker::module_element(name = "masamune-whisker-view-capture:Capture", measurement = None, commands = [("capture", Map)])]
pub fn view_capture(style: Style, on_captured: CustomEvent, children: Children) {}

/// 生成される Host 起動処理（Web・desktop）から参照される要素スキーマ。
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [view_capture_schema::element_provider()],
    )
}
