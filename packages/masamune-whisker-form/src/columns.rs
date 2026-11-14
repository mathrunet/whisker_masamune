//! Scrolling value columns in a bottom sheet with Cancel / Confirm buttons: the picker of the
//! date, month and duration fields (katana_form's column pickers).

use masamune_whisker_appearance::{ColorExt, TextKind, palette, text, text_with};
use masamune_whisker_ui::{FilledButton, InkWell, ModalBottomSheet, TextButton};
use whisker::css::{AlignItems, JustifyContent, TextAlign};
use whisker::prelude::*;

use crate::style::FormStyle;

/// One scrolling column of numbers. Tapping a number selects it.
#[component]
pub(crate) fn picker_column(
    values: Vec<u32>,
    selected: RwSignal<u32>,
    suffix: String,
    /// Shows numbers below 10 with a leading zero (minutes, seconds).
    #[prop(default = false)]
    two_digits: bool,
    style: FormStyle,
) -> Element {
    let values = StoredValue::new(values.clone());
    let suffix = StoredValue::new(suffix.clone());
    render! {
        ScrollView(style: css!(flex_grow: 1.0, flex_basis: px(0), height: px(240))) {
            View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Column)) {
                ForEach(
                    each: move || values.get(),
                    key: |v: &u32| *v,
                    children: move |v: ReadSignal<u32>| {
                        let v = v.get_untracked();
                        let row = computed(move || {
                            css!(
                                display: Display::Flex,
                                justify_content: JustifyContent::Center,
                                padding: (px(10), px(4)),
                                border_radius: px(style.option_radius),
                                background_color: if selected.get() == v {
                                    style.active_color.with_alpha(0.15)
                                } else {
                                    palette().surface
                                },
                            )
                        });
                        let text_style = computed(move || {
                            text_with(
                                style.option_kind,
                                if selected.get() == v {
                                    style.active_color
                                } else {
                                    palette().text
                                },
                            )
                        });
                        let label = if two_digits {
                            format!("{v:02}{}", suffix.get())
                        } else {
                            format!("{v}{}", suffix.get())
                        };
                        render! {
                            InkWell(style: row, on_tap: Callback::new(move |()| selected.set(v))) {
                                Text(value: label.clone(), style: text_style)
                            }
                        }
                    },
                )
            }
        }
    }
}

/// Bottom sheet with the picked value as its title, the columns in `children` side by side
/// and Cancel / Confirm buttons. Confirm calls `on_submit`; both buttons close the sheet.
#[component]
pub(crate) fn column_sheet(
    open: RwSignal<bool>,
    picked: Signal<String>,
    confirm_text: Signal<String>,
    cancel_text: Signal<String>,
    on_submit: Callback<()>,
    children: Children,
) -> Element {
    let children = StoredValue::new(children.clone());
    render! {
        ModalBottomSheet(open: open) {
            View(
                style: css!(display: Display::Flex, flex_direction: FlexDirection::Column, gap: px(16)),
            ) {
                Text(value: picked, style: text(TextKind::TitleLarge).text_align(TextAlign::Center))
                View(
                    style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, gap: px(8)),
                ) {
                    {(children.get())()}
                }
                View(
                    style: css!(
                        display: Display::Flex,
                        flex_direction: FlexDirection::Row,
                        justify_content: JustifyContent::FlexEnd,
                        align_items: AlignItems::Center,
                        gap: px(8),
                    ),
                ) {
                    TextButton(label: cancel_text, on_tap: Callback::new(move |()| open.set(false)))
                    FilledButton(
                        label: confirm_text,
                        on_tap: Callback::new(move |()| {
                            on_submit.run(());
                            open.set(false);
                        }),
                    )
                }
            }
        }
    }
}
