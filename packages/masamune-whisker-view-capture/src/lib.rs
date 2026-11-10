//! Element that saves the screen, including its children, as a PNG image.
//!
//! `ElementRef::command("capture", {fileName, heightDp?})` saves it to the cache,
//! then `captured` (`{path}` or `{error}`) is delivered. Passing `heightDp` crops from the top.
//!
//! The PNG has the element's size in physical pixels (dp × density on Android, points × screen
//! scale on iOS) and is saved under the app cache (`Caches/masamune_whisker_view_capture/` on iOS).
//! On iOS an element inside the window is snapshotted as composited (`drawHierarchy`); an element
//! placed outside the window, such as an off-screen share card, is rendered from its layer tree,
//! which omits effects that only exist in the compositor (backdrop blur, video).

use whisker::event::CustomEvent;

#[doc(hidden)]
#[whisker::module_element(name = "masamune-whisker-view-capture:Capture", measurement = None, commands = [("capture", Map)])]
pub fn view_capture(style: Style, on_captured: CustomEvent, children: Children) {}

/// Element schema referenced by the generated Host bootstrap code (Web, desktop).
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [view_capture_schema::element_provider()],
    )
}
