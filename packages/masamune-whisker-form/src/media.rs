//! Images, videos and other files (katana_form's `FormMedia` and `FormMultiMedia`).
//!
//! The fields have no picker of their own: `on_tap` receives a ref and the app picks a file
//! however it likes (for example `masamune-whisker-image-picker`) and hands the result to
//! `ref.update(uri, media_type)`.

use masamune_whisker_appearance::{ColorExt, palette, text_with};
use masamune_whisker_ui::InkWell;
use masamune_whisker_ui::style::{css_color, fill};
use whisker::css::{AlignItems, Border, Color, FlexWrap, JustifyContent, Overflow, PositionKind};
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};
use whisker_image::{Image, ImageMode};

use crate::controller::{FormField, bind_field, check};
use crate::style::{FormStyle, resolve};

/// Kind of a picked file (katana_form's `FormMediaType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FormMediaType {
    /// An image, previewed with `whisker-image`.
    #[default]
    Image,
    /// A video, shown with a video icon.
    Video,
    /// Any other file, shown with a file icon.
    Others,
}

/// A picked file: its URI (`file://`, `http(s)://`) and kind (katana_form's `FormMediaValue`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FormMediaValue {
    /// Where the file is.
    pub uri: String,
    /// What the file is.
    pub media_type: FormMediaType,
}

impl FormMediaValue {
    /// A file at `uri` of kind `media_type`.
    pub fn new(uri: impl Into<String>, media_type: FormMediaType) -> Self {
        Self {
            uri: uri.into(),
            media_type,
        }
    }

    /// An image at `uri`.
    pub fn image(uri: impl Into<String>) -> Self {
        Self::new(uri, FormMediaType::Image)
    }

    /// A video at `uri`.
    pub fn video(uri: impl Into<String>) -> Self {
        Self::new(uri, FormMediaType::Video)
    }
}

/// Handle given to [`FormMedia`]'s `on_tap` (katana_form's `FormMediaRef`).
#[derive(Clone, Copy)]
pub struct FormMediaRef {
    value: RwSignal<Option<FormMediaValue>>,
    on_changed: Option<Callback<Option<FormMediaValue>>>,
}

impl FormMediaRef {
    /// The current file.
    pub fn value(&self) -> Option<FormMediaValue> {
        self.value.try_get_untracked().flatten()
    }

    /// Replaces the file with the one at `uri`.
    pub fn update(&self, uri: impl Into<String>, media_type: FormMediaType) {
        self.set(Some(FormMediaValue::new(uri, media_type)));
    }

    /// Removes the file.
    pub fn clear(&self) {
        self.set(None);
    }

    fn set(&self, value: Option<FormMediaValue>) {
        if self.value.try_set(value.clone())
            && let Some(on_changed) = self.on_changed
        {
            on_changed.run(value);
        }
    }
}

/// Appends `value` unless the list already has `max` files.
pub(crate) fn push_media(
    list: &mut Vec<FormMediaValue>,
    value: FormMediaValue,
    max: Option<usize>,
) -> bool {
    if max.is_some_and(|max| list.len() >= max) {
        return false;
    }
    list.push(value);
    true
}

/// Handle given to [`FormMultiMedia`]'s `on_tap` (katana_form's `FormMultiMediaRef`).
#[derive(Clone, Copy)]
pub struct FormMultiMediaRef {
    values: RwSignal<Vec<FormMediaValue>>,
    max_length: Option<usize>,
    on_changed: Option<Callback<Vec<FormMediaValue>>>,
    on_remove: Option<Callback<FormMediaValue>>,
}

impl FormMultiMediaRef {
    /// The current files.
    pub fn values(&self) -> Vec<FormMediaValue> {
        self.values.try_get_untracked().unwrap_or_default()
    }

    /// Whether `max_length` files are already there.
    pub fn is_full(&self) -> bool {
        self.max_length
            .is_some_and(|max| self.values.with_untracked(Vec::len) >= max)
    }

    /// Adds the file at `uri`. Returns `false` when the list is full.
    pub fn update(&self, uri: impl Into<String>, media_type: FormMediaType) -> bool {
        let mut list = self.values();
        if !push_media(
            &mut list,
            FormMediaValue::new(uri, media_type),
            self.max_length,
        ) {
            return false;
        }
        self.set(list);
        true
    }

    /// Removes the file at `index` and returns it (calling `on_remove`).
    pub fn remove(&self, index: usize) -> Option<FormMediaValue> {
        let mut list = self.values();
        if index >= list.len() {
            return None;
        }
        let removed = list.remove(index);
        self.set(list);
        if let Some(on_remove) = self.on_remove {
            on_remove.run(removed.clone());
        }
        Some(removed)
    }

    fn set(&self, list: Vec<FormMediaValue>) {
        if self.values.try_set(list.clone())
            && let Some(on_changed) = self.on_changed
        {
            on_changed.run(list);
        }
    }
}

/// Preview of a file filling its parent.
#[component]
fn media_preview(value: FormMediaValue, style: FormStyle) -> Element {
    let is_image = value.media_type == FormMediaType::Image;
    let icon = match value.media_type {
        FormMediaType::Video => lucide::Video,
        _ => lucide::File,
    };
    let uri = StoredValue::new(value.uri.clone());
    let icon_color = StoredValue::new(css_color(style.icon_color));
    render! {
        Show(
            when: move || is_image,
            fallback: move || render! {
                View(
                    style: fill()
                        .display(Display::Flex)
                        .align_items(AlignItems::Center)
                        .justify_content(JustifyContent::Center),
                ) {
                    Icon(svg: icon, color: icon_color.get(), size: "32")
                }
            },
        ) {
            Image(src: uri.get(), mode: ImageMode::AspectFill, style: fill())
        }
    }
}

/// Box of a media field: border, radius and background of the style, content clipped.
fn media_box(style: FormStyle, has_error: bool, width: Option<f32>, height: f32) -> Css {
    let css = css!(
        position: PositionKind::Relative,
        height: px(height),
        overflow: Overflow::Hidden,
        border_radius: px(style.radius),
        background_color: style.background_color,
    )
    .border(
        Border::new()
            .width(px(style.border_width))
            .solid()
            .color(if has_error {
                style.error_color
            } else {
                style.border_color
            }),
    );
    match width {
        Some(width) => css.width(px(width)),
        None => css.width(percent(100)),
    }
}

/// One image, video or file (katana_form's `FormMedia`).
///
/// Tapping the box calls `on_tap` with a [`FormMediaRef`]; pick a file there and pass it to
/// `update`. Images are previewed; while empty (or always, with `show_overlay_icon`) the box
/// shows `icon`.
///
/// ```ignore
/// FormMedia(
///     form: form.field(move |image: Option<FormMediaValue>| Profile { image, ..form.value() }),
///     on_tap: move |media: FormMediaRef| {
///         spawn_local(async move {
///             if let Ok(Some(picked)) = masamune_whisker_image_picker::pick_image(false, 1024).await {
///                 media.update(format!("file://{}", picked.path), FormMediaType::Image);
///             }
///         });
///     },
/// )
/// ```
#[component]
pub fn form_media(
    /// Called with a ref when the box is tapped.
    on_tap: Callback<FormMediaRef>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<FormMediaValue>>>,
    /// Two-way bound file. Without it the field keeps its own file, starting at `initial_value`.
    value: Option<RwSignal<Option<FormMediaValue>>>,
    /// Initial file, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<FormMediaValue>,
    /// Error shown when empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid file, or `None`.
    validator: Option<Callback<Option<FormMediaValue>, Option<String>>>,
    /// Called when the file changes.
    on_changed: Option<Callback<Option<FormMediaValue>>>,
    /// `false` makes the box read-only (katana_form's `readOnly`).
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Width of the box; `None` fills the parent.
    #[prop(default = None)]
    width: Option<f32>,
    /// Height of the box.
    #[prop(default = 200.0)]
    height: f32,
    /// Icon shown while empty and over the preview.
    #[prop(default = lucide::ImagePlus)]
    icon: &'static str,
    /// Size of `icon`.
    #[prop(default = 56.0)]
    icon_size: f32,
    /// Shows `icon` over a picked file too.
    #[prop(default = true)]
    show_overlay_icon: bool,
    /// Color over a picked file behind the icon.
    #[prop(default = Color::rgba(0, 0, 0, 0.38))]
    overlay_color: Color,
    /// Color of the icon over a picked file.
    #[prop(default = Color::rgba(255, 255, 255, 0.7))]
    overlay_icon_color: Color,
    /// Look of the box; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = match value {
        Some(value) if initial_value.is_none() => value.get_untracked(),
        _ => initial_value.clone(),
    };
    let value = value.unwrap_or_else(|| signal(initial_value.clone()));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || value.get_untracked(),
        move |v: &Option<FormMediaValue>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || value.set(reset_value.clone()),
    );
    let media = FormMediaRef { value, on_changed };
    let box_style = computed(move || {
        media_box(style, error.get().is_some(), width, height).opacity(if enabled.get() {
            1.0
        } else {
            style.disabled_opacity
        })
    });
    let items = computed(move || value.get().into_iter().collect::<Vec<FormMediaValue>>());
    let empty = computed(move || value.with(Option::is_none));
    let icon_size = StoredValue::new(format!("{icon_size}"));
    let overlay = computed(move || {
        let css = fill()
            .display(Display::Flex)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center);
        if empty.get() {
            css
        } else {
            css.background_color(overlay_color)
        }
    });
    let icon_color = computed(move || {
        css_color(if empty.get() {
            style.icon_color
        } else {
            overlay_icon_color
        })
    });
    let error_value = computed(move || error.get().unwrap_or_default());
    let (error_top, error_h) = style.error_padding;
    let error_style = StoredValue::new(text_with(style.error_kind, style.error_color).padding((
        px(error_top),
        px(error_h),
        px(0),
        px(error_h),
    )));
    render! {
        View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Column)) {
            InkWell(
                style: box_style,
                accessibility: Accessibility::new()
                    .label("Media")
                    .role(AccessibilityRole::Button),
                disabled: computed(move || !enabled.get()),
                on_tap: Callback::new(move |()| on_tap.run(media)),
            ) {
                ForEach(
                    each: move || items.get(),
                    key: |v: &FormMediaValue| v.uri.clone(),
                    children: move |v: ReadSignal<FormMediaValue>| render! {
                        MediaPreview(value: v.get_untracked(), style: style)
                    },
                )
                Show(when: move || empty.get() || show_overlay_icon) {
                    View(style: overlay) {
                        Icon(svg: icon, color: icon_color, size: icon_size.get())
                    }
                }
            }
            Show(when: move || error.get().is_some()) {
                Text(value: error_value, style: error_style.get())
            }
        }
    }
}

/// Several images, videos or files (katana_form's `FormMultiMedia` with the inline delegate).
///
/// The files are shown as square tiles followed by an add tile, which calls `on_tap` with a
/// [`FormMultiMediaRef`]: pick a file there and pass it to `update`. The add tile disappears at
/// `max_length` files. Each tile's remove badge removes its file (calling `on_remove`).
#[component]
pub fn form_multi_media(
    /// Called with a ref when the add tile is tapped.
    on_tap: Callback<FormMultiMediaRef>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Vec<FormMediaValue>>>,
    /// Two-way bound files. Without it the field keeps its own files, starting at
    /// `initial_value`.
    value: Option<RwSignal<Vec<FormMediaValue>>>,
    /// Initial files, also restored by `FormController::reset`.
    #[prop(default = Vec::new())]
    initial_value: Vec<FormMediaValue>,
    /// Maximum number of files; `None` for no limit.
    #[prop(default = None)]
    max_length: Option<usize>,
    /// Called with a removed file.
    on_remove: Option<Callback<FormMediaValue>>,
    /// Label above the tiles (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Error shown when empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for invalid files, or `None`.
    validator: Option<Callback<Vec<FormMediaValue>, Option<String>>>,
    /// Called with the files after every change.
    on_changed: Option<Callback<Vec<FormMediaValue>>>,
    /// `false` makes the field read-only (no add tile, no remove badges).
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Side of each tile.
    #[prop(default = 96.0)]
    tile_size: f32,
    /// Icon of the add tile.
    #[prop(default = lucide::ImagePlus)]
    add_icon: &'static str,
    /// Look of the tiles; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = match value {
        Some(value) if initial_value.is_empty() => value.get_untracked(),
        _ => initial_value.clone(),
    };
    let values = value.unwrap_or_else(|| signal(initial_value.clone()));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || values.get_untracked(),
        move |v: &Vec<FormMediaValue>| check(v, Vec::is_empty, empty_error_text, validator),
        error,
        move || values.set(reset_value.clone()),
    );
    let media = FormMultiMediaRef {
        values,
        max_length,
        on_changed,
        on_remove,
    };
    let items = computed(move || values.get().into_iter().enumerate().collect::<Vec<_>>());
    let can_add =
        computed(move || enabled.get() && max_length.is_none_or(|max| values.with(Vec::len) < max));
    let tile = move |has_error: bool| media_box(style, has_error, Some(tile_size), tile_size);
    let add_style = computed(move || {
        tile(error.get().is_some())
            .display(Display::Flex)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center)
    });
    let has_label = label.is_some();
    let label_value = computed(move || label.map(|l| l.get()).unwrap_or_default());
    let error_value = computed(move || error.get().unwrap_or_default());
    let add_color = StoredValue::new(css_color(style.icon_color));
    render! {
        View(
            style: css!(display: Display::Flex, flex_direction: FlexDirection::Column, gap: px(4)),
        ) {
            Show(when: move || has_label) {
                Text(value: label_value, style: text_with(style.label_kind, style.label_color))
            }
            View(
                style: css!(
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    gap: px(8),
                ),
            ) {
                ForEach(
                    each: move || items.get(),
                    key: |(i, v): &(usize, FormMediaValue)| format!("{i}:{}", v.uri),
                    children: move |item: ReadSignal<(usize, FormMediaValue)>| {
                        let (index, value) = item.get_untracked();
                        render! {
                            View(style: tile(false)) {
                                MediaPreview(value: value.clone(), style: style)
                                Show(when: move || enabled.get()) {
                                    View(
                                        style: css!(
                                            position: PositionKind::Absolute,
                                            top: px(2),
                                            right: px(2),
                                            padding: px(2),
                                        ),
                                        accessibility: Accessibility::new()
                                            .label("Remove")
                                            .role(AccessibilityRole::Button),
                                        on_tap_catch: move |_| {
                                            media.remove(index);
                                        },
                                    ) {
                                        Icon(
                                            svg: lucide::CircleX,
                                            color: css_color(palette().error.with_alpha(0.9)),
                                            size: "22",
                                        )
                                    }
                                }
                            }
                        }
                    },
                )
                Show(when: move || can_add.get()) {
                    InkWell(
                        style: add_style,
                        accessibility: Accessibility::new()
                            .label("Add")
                            .role(AccessibilityRole::Button),
                        on_tap: Callback::new(move |()| on_tap.run(media)),
                    ) {
                        Icon(svg: add_icon, color: add_color.get(), size: "32")
                    }
                }
            }
            Show(when: move || error.get().is_some()) {
                Text(value: error_value, style: text_with(style.error_kind, style.error_color))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_lists_respect_the_limit() {
        let mut list = Vec::new();
        assert!(push_media(
            &mut list,
            FormMediaValue::image("file:///a.png"),
            Some(2)
        ));
        assert!(push_media(
            &mut list,
            FormMediaValue::video("file:///b.mp4"),
            Some(2)
        ));
        assert!(!push_media(
            &mut list,
            FormMediaValue::image("file:///c.png"),
            Some(2)
        ));
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].media_type, FormMediaType::Video);
        assert!(push_media(
            &mut list,
            FormMediaValue::image("file:///c.png"),
            None
        ));
    }

    #[test]
    fn refs_update_and_remove() {
        let runtime =
            whisker::runtime::RuntimeContext::new(whisker::runtime::RuntimeWakeHandle::new(|| {}));
        runtime.enter(|| {
            let owner = whisker::Owner::new(None);
            owner.with(|| {
                let single = FormMediaRef {
                    value: signal(None),
                    on_changed: None,
                };
                single.update("file:///a.png", FormMediaType::Image);
                assert_eq!(single.value(), Some(FormMediaValue::image("file:///a.png")));
                single.clear();
                assert_eq!(single.value(), None);

                let removed = signal(Vec::<FormMediaValue>::new());
                let multi = FormMultiMediaRef {
                    values: signal(Vec::new()),
                    max_length: Some(2),
                    on_changed: None,
                    on_remove: Some(Callback::new(move |v: FormMediaValue| {
                        removed.update(|r| r.push(v))
                    })),
                };
                assert!(multi.update("a", FormMediaType::Image));
                assert!(multi.update("b", FormMediaType::Others));
                assert!(multi.is_full());
                assert!(!multi.update("c", FormMediaType::Image));
                assert_eq!(multi.remove(0).map(|v| v.uri), Some("a".into()));
                assert_eq!(multi.remove(5), None);
                assert_eq!(
                    multi.values(),
                    vec![FormMediaValue::new("b", FormMediaType::Others)]
                );
                assert_eq!(removed.get_untracked().len(), 1);
            });
            owner.dispose();
        });
        runtime.shutdown();
    }
}
