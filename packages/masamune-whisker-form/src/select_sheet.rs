//! Bottom sheet with a list of options (the picker of katana_form's modal and dropdown fields).

use masamune_whisker_appearance::{TextKind, bold, palette, text_with};
use masamune_whisker_ui::style::css_color;
use masamune_whisker_ui::{FilledButton, InkWell, ModalBottomSheet};
use whisker::css::{AlignItems, TextAlign};
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};

use crate::style::{FormStyle, resolve};

/// A [`ModalBottomSheet`] listing `options`. Tapping one calls `on_select` with its index and
/// closes the sheet. The selected option is tinted with [`FormStyle::selected_color`]; long
/// lists scroll inside `max_height`.
///
/// With `multi_selected` the sheet selects several options: every selected option is tinted
/// and checked, tapping one calls `on_select` (toggle it there) and keeps the sheet open, and
/// `done_text` adds a button that closes it.
#[component]
pub fn select_sheet(
    /// Opens and closes the sheet.
    open: RwSignal<bool>,
    /// Labels of the options, in order.
    options: Signal<Vec<String>>,
    /// Index of the selected option.
    #[prop(default = Signal::from(None))]
    selected: Signal<Option<usize>>,
    /// Indexes of the selected options; turns on multi-select (see above).
    multi_selected: Option<Signal<Vec<usize>>>,
    /// Label of the button that closes a multi-select sheet; `None` shows no button.
    done_text: Option<Signal<String>>,
    /// Called with the index of the tapped option.
    on_select: Callback<usize>,
    /// Title above the options.
    title: Option<Signal<String>>,
    /// Maximum height of the list; longer lists scroll.
    #[prop(default = 360.0)]
    max_height: f32,
    /// Look of the options; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let multi = multi_selected.is_some();
    let is_selected = move |i: usize| match multi_selected {
        Some(many) => many.get().contains(&i),
        None => selected.get() == Some(i),
    };
    let has_done = done_text.is_some();
    let done_label = computed(move || done_text.map(|t| t.get()).unwrap_or_default());
    let indexes = computed(move || (0..options.get().len()).collect::<Vec<usize>>());
    let has_title = title.is_some();
    let title_value = computed(move || title.map(|t| t.get()).unwrap_or_default());
    render! {
        ModalBottomSheet(open: open) {
            Show(when: move || has_title) {
                Text(
                    value: title_value,
                    style: bold(TextKind::TitleMedium, palette().text)
                        .text_align(TextAlign::Center)
                        .padding_bottom(px(12)),
                )
            }
            ScrollView(style: css!(max_height: px(max_height))) {
                View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Column)) {
                    ForEach(
                        each: move || indexes.get(),
                        key: |i: &usize| *i,
                        children: move |i: ReadSignal<usize>| {
                            let i = i.get_untracked();
                            let row = computed(move || {
                                let (v, h) = style.option_padding;
                                css!(
                                    display: Display::Flex,
                                    flex_direction: FlexDirection::Row,
                                    align_items: AlignItems::Center,
                                    min_height: px(style.option_min_height),
                                    padding: (px(v), px(h)),
                                    border_radius: px(style.option_radius),
                                    background_color: if is_selected(i) {
                                        style.selected_color
                                    } else {
                                        palette().surface
                                    },
                                )
                            });
                            let label = computed(move || options.get().get(i).cloned().unwrap_or_default());
                            let accessibility = computed(move || {
                                Accessibility::new()
                                    .label(label.get())
                                    .role(AccessibilityRole::Button)
                                    .state(AccessibilityState::new().selected(is_selected(i)))
                            });
                            render! {
                                InkWell(
                                    style: row,
                                    accessibility: accessibility,
                                    on_tap: Callback::new(move |()| {
                                        on_select.run(i);
                                        if !multi {
                                            open.set(false);
                                        }
                                    }),
                                ) {
                                    Text(
                                        value: label,
                                        style: text_with(style.option_kind, palette().text).flex_grow(1.0),
                                    )
                                    Show(when: move || multi && is_selected(i)) {
                                        Icon(
                                            svg: lucide::Check,
                                            color: css_color(style.active_color),
                                            size: "20",
                                        )
                                    }
                                }
                            }
                        },
                    )
                }
            }
            Show(when: move || has_done) {
                View(
                    style: css!(
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        padding_top: px(12),
                    ),
                ) {
                    FilledButton(
                        label: done_label,
                        expand: true,
                        on_tap: Callback::new(move |()| open.set(false)),
                    )
                }
            }
        }
    }
}
