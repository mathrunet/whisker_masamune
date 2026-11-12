//! Dialogs and the busy indicator shown over the whole app (`showDialog` / `Modal.confirm`).

use std::future::Future;
use std::rc::Rc;

use masamune_whisker_appearance::{TextKind, bold, palette, shape, text, text_with};
use whisker::css::{AlignItems, Color, JustifyContent, PositionKind};
use whisker::prelude::*;

use crate::material::ink_well::InkWell;
use crate::material::progress::CircularProgressIndicator;
use crate::material::semantics;
use crate::style::{center, column, fill, row};

/// Accessibility identifier of the submit button of the dialog shown by [`DialogHost`].
pub const DIALOG_SUBMIT_IDENTIFIER: &str = "dialog_submit";
/// Accessibility identifier of the cancel button of the dialog shown by [`DialogHost`].
pub const DIALOG_CANCEL_IDENTIFIER: &str = "dialog_cancel";

/// An alert or confirmation dialog.
#[derive(Clone)]
pub struct Dialog {
    pub title: String,
    pub text: String,
    pub submit_text: String,
    /// Shows a cancel button when set.
    pub cancel_text: Option<String>,
    pub on_submit: Option<Rc<dyn Fn()>>,
    pub on_cancel: Option<Rc<dyn Fn()>>,
}

impl PartialEq for Dialog {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.text == other.text
            && self.submit_text == other.submit_text
            && self.cancel_text == other.cancel_text
    }
}

/// Dialog and busy state shared by the app. Provide one with [`provide_overlay`] (or keep it in
/// your app state) and render it with [`OverlayHost`].
#[derive(Clone, Copy)]
pub struct Overlay {
    pub dialog: RwSignal<Option<Dialog>>,
    /// Number of running guarded operations; the busy indicator shows while it is above zero.
    pub busy: RwSignal<u32>,
}

impl Default for Overlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay {
    pub fn new() -> Self {
        Self {
            dialog: signal(None),
            busy: signal(0),
        }
    }

    /// Shows `dialog`, replacing the current one.
    pub fn show(&self, dialog: Dialog) {
        self.dialog.set(Some(dialog));
    }

    /// Closes the current dialog without calling its callbacks.
    pub fn dismiss(&self) {
        self.dialog.set(None);
    }

    /// Alert with a single button.
    pub fn alert(
        &self,
        title: impl Into<String>,
        text: impl Into<String>,
        submit_text: impl Into<String>,
    ) {
        self.show(Dialog {
            title: title.into(),
            text: text.into(),
            submit_text: submit_text.into(),
            cancel_text: None,
            on_submit: None,
            on_cancel: None,
        });
    }

    /// Alert with a single button that calls `on_submit` when closed.
    pub fn alert_then(
        &self,
        title: impl Into<String>,
        text: impl Into<String>,
        submit_text: impl Into<String>,
        on_submit: impl Fn() + 'static,
    ) {
        self.show(Dialog {
            title: title.into(),
            text: text.into(),
            submit_text: submit_text.into(),
            cancel_text: None,
            on_submit: Some(Rc::new(on_submit)),
            on_cancel: None,
        });
    }

    /// Confirmation with submit and cancel buttons.
    pub fn confirm(
        &self,
        title: impl Into<String>,
        text: impl Into<String>,
        submit_text: impl Into<String>,
        cancel_text: impl Into<String>,
        on_submit: impl Fn() + 'static,
        on_cancel: Option<Rc<dyn Fn()>>,
    ) {
        self.show(Dialog {
            title: title.into(),
            text: text.into(),
            submit_text: submit_text.into(),
            cancel_text: Some(cancel_text.into()),
            on_submit: Some(Rc::new(on_submit)),
            on_cancel,
        });
    }

    /// Shows the busy indicator while `future` runs.
    pub async fn guarded<T>(&self, future: impl Future<Output = T>) -> T {
        let busy = self.busy;
        busy.update(|n| *n += 1);
        let result = future.await;
        busy.update(|n| *n = n.saturating_sub(1));
        result
    }
}

/// Creates an [`Overlay`] and provides it as context.
pub fn provide_overlay() -> Overlay {
    let overlay = Overlay::new();
    provide_context(overlay);
    overlay
}

/// The [`Overlay`] provided by an ancestor. Call it in a component body.
pub fn use_overlay() -> Option<Overlay> {
    use_context::<Overlay>()
}

/// Renders the dialog and the busy indicator of `overlay` above everything else.
#[component]
pub fn overlay_host(overlay: Overlay) -> Element {
    render! {
        Fragment {
            BusyHost(overlay: overlay)
            DialogHost(overlay: overlay)
        }
    }
}

/// Renders the dialog of `overlay`.
///
/// The buttons are exposed to UI automation as [`DIALOG_SUBMIT_IDENTIFIER`] and
/// [`DIALOG_CANCEL_IDENTIFIER`] (only one dialog is shown at a time), labelled with their text.
#[component]
pub fn dialog_host(overlay: Overlay) -> Element {
    let dialog = overlay.dialog;
    render! {
        Show(when: move || dialog.with(Option::is_some)) {
            DialogView(overlay: overlay)
        }
    }
}

#[component]
fn dialog_view(overlay: Overlay) -> Element {
    let dialog = overlay.dialog;
    let title =
        computed(move || dialog.with(|d| d.as_ref().map(|d| d.title.clone()).unwrap_or_default()));
    let text_value =
        computed(move || dialog.with(|d| d.as_ref().map(|d| d.text.clone()).unwrap_or_default()));
    let submit = computed(move || {
        dialog.with(|d| {
            d.as_ref()
                .map(|d| d.submit_text.clone())
                .unwrap_or_default()
        })
    });
    let cancel = computed(move || dialog.with(|d| d.as_ref().and_then(|d| d.cancel_text.clone())));
    let on_submit = Callback::new(move |()| {
        let current = dialog.get_untracked();
        dialog.set(None);
        if let Some(callback) = current.and_then(|d| d.on_submit) {
            callback();
        }
    });
    let on_cancel = Callback::new(move |()| {
        let current = dialog.get_untracked();
        dialog.set(None);
        if let Some(callback) = current.and_then(|d| d.on_cancel) {
            callback();
        }
    });
    let submit_accessibility =
        computed(move || semantics::button(Some(DIALOG_SUBMIT_IDENTIFIER), submit.get(), false));
    let cancel_accessibility = computed(move || {
        semantics::button(
            Some(DIALOG_CANCEL_IDENTIFIER),
            cancel.get().unwrap_or_default(),
            false,
        )
    });
    let palette = palette();
    render! {
        View(style: fill().z_index(1000).merge(center()).background_color(palette.barrier)) {
            View(style: column()
                .width(percent(85))
                .max_width(px(400))
                .padding(px(24))
                .gap(px(16))
                .border_radius(px(shape().extra_large))
                .background_color(palette.surface)) {
                Show(when: move || !title.get().is_empty()) {
                    Text(value: title, style: text(TextKind::HeadlineSmall))
                }
                Text(value: text_value, style: text_with(TextKind::BodyMedium, palette.sub_text))
                View(style: row().justify_content(JustifyContent::FlexEnd).gap(px(8))) {
                    Show(when: move || cancel.get().is_some()) {
                        InkWell(
                            style: css!(padding: (px(10), px(12)), border_radius: px(shape().full)),
                            accessibility: cancel_accessibility,
                            on_tap: on_cancel,
                        ) {
                            Text(
                                value: computed(move || cancel.get().unwrap_or_default()),
                                style: bold(TextKind::LabelLarge, palette.primary),
                            )
                        }
                    }
                    InkWell(
                        style: css!(padding: (px(10), px(12)), border_radius: px(shape().full)),
                        accessibility: submit_accessibility,
                        on_tap: on_submit,
                    ) {
                        Text(value: submit, style: bold(TextKind::LabelLarge, palette.primary))
                    }
                }
            }
        }
    }
}

/// Full-screen busy indicator that blocks taps while `overlay.busy > 0`.
#[component]
pub fn busy_host(overlay: Overlay) -> Element {
    let busy = overlay.busy;
    render! {
        Show(when: move || busy.get() > 0) {
            View(style: fill()
                .z_index(900)
                .merge(center())
                .align_items(AlignItems::Center)
                .background_color(Color::rgba(0, 0, 0, 0.3))
                .position(PositionKind::Absolute),
                on_tap_catch: |_| {}) {
                CircularProgressIndicator(size: 40.0, trace_name: "busy".to_owned())
            }
        }
    }
}
