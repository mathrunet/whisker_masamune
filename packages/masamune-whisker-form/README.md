# masamune-whisker-form

Form widgets for [Whisker](https://whisker.rs) apps: the Whisker version of Masamune's
`katana_form`. A `FormController` validates its fields together and collects their values,
and every field takes its look from one `FormStyle`. Colors and text styles default to
`masamune-whisker-appearance`.

| katana_form | masamune-whisker-form |
|---|---|
| `FormController` | `FormController` (`field(on_saved)`, `validate`, `reset`, `value`, `set_value`) |
| `form:` + `onSaved:` | `form: form.field(move \|v\| …)`; `form: form` takes part in validation only |
| `FormStyle` / `FormStyleScope` | `FormStyle` / `provide_form_style`, `use_form_style` |
| `FormInputBorderStyle` | `FormInputBorderStyle` (`None`, `Outline`, `Underline`) |
| `InputDecoration` (label, prefix, suffix, error) | `FormFieldFrame` |
| `FormTextField` | `FormTextField` |
| `FormEnumModalField` / `FormEnumDropdownField` | `FormEnumModalField` / `FormEnumDropdownField` (same widget) |
| `FormMapModalField` / `FormMapDropdownField` | `FormMapModalField` / `FormMapDropdownField` (same widget) |
| `FormNumModalField` | `FormNumModalField` |
| `FormDateField` | `FormDateField` (year / month / day columns in a bottom sheet) |
| `FormDateTimeField` | `FormDateTimeField` (OS pickers from `masamune-whisker-date-picker`) |
| `FormDateTimeRangeField` | `FormDateTimeRangeField` (the OS date picker twice: start, then end) |
| `FormMonthModalField` | `FormMonthModalField` (year / month columns) |
| `FormDurationField` | `FormDurationField` (day / hour / minute / second columns) |
| `FormChipsField` | `FormChipsField` (free-text chips with `max_chips` and `suggestion`) |
| `FormMapTagDropdownField` | `FormMapTagDropdownField` (multi-select sheet, picked keys as chips) |
| `FormMedia` / `FormMultiMedia` | `FormMedia` / `FormMultiMedia` (`FormMediaValue` = URI + `FormMediaType`) |
| `FormRatingBar` | `FormRatingBar` |
| `FormPinField` | `FormPinField` |
| `FormFutureField` | `FormFutureField` (async `on_tap(current)`) |
| `FormBuilder` / `FormContainer` | `FormBuilder` / `FormContainer` |
| `FormListBuilder` | `FormListBuilder` (`FormListRef`: `add`, `insert`, `update`, `delete_at`, `delete`) |
| `FormCheckbox` / `FormSwitch` | `FormCheckbox` / `FormSwitch` |
| `FormButton` | `FormButton` |
| `FormLabel` | `FormLabel` |
| picker of the modal fields | `SelectSheet` |

Common props follow katana_form: `label` (`labelText`), `hint` (`hintText`), `initial_value`,
`empty_error_text`, `validator`, `on_changed`, `enabled`, `prefix`, `suffix` and `style`.
Each field also takes a `value` (`text` for text and PIN fields) signal for two-way binding, so it can
be used without a form too.

## Forms

Bind each field with `form.field(on_saved)`. `on_saved` receives the field value and returns the
new form value; fields are saved in the order they were mounted, each one starting from the value
the previous one returned. `validate()` runs every validator first (every field shows its own
error under the frame) and saves only when all of them pass.

```rust
#[derive(Clone, Default)]
struct Profile {
    name: String,
    gender: Option<Gender>,
    birthday: Option<NaiveDate>,
}

let form = FormController::new(Profile::default());
render! {
    FormTextField(
        form: form.field(move |name: String| Profile { name, ..form.value() }),
        label: "Name",
        hint: "Enter your name",
        empty_error_text: "Enter your name",
    )
    FormEnumModalField(
        values: Gender::ALL.to_vec(),
        label_builder: |gender: Gender| gender.label().to_owned(),
        form: form.field(move |gender: Option<Gender>| Profile { gender, ..form.value() }),
        label: "Gender",
        hint: "Select",
    )
    FormDateField(
        form: form.field(move |birthday: Option<NaiveDate>| Profile { birthday, ..form.value() }),
        label: "Birthday",
        empty_error_text: "Select your birthday",
        confirm_text: "OK",
        cancel_text: "Cancel",
    )
    FormButton(label: "Save", on_tap: move |_| {
        if let Some(profile) = form.validate() {
            save(profile);
        }
    })
}
```

`form.set_value(loaded)` replaces the value (for example after loading); `reset()` clears the
errors and returns the fields and the value to it. A field unmounted with `Show` leaves the form,
so hidden fields are not validated.

## Style

Provide one `FormStyle` inside `AppRoot` (so theme changes rebuild it); fields without a
`style` prop use it, and `FormStyle::from_theme()` is the fallback.

```rust
#[component]
fn form_theme(children: Children) -> Element {
    provide_form_style(FormStyle {
        border_color: Color::hex(0x9E9E9E),
        content_padding: (16.0, 16.0),
        padding: (0.0, 16.0),
        focused_border_color: None,
        ..FormStyle::from_theme()
    });
    let children = children.clone();
    render! { Fragment { {children()} } }
}

// A single field with another look:
render! {
    FormTextField(text: comment, style: FormStyle { radius: 8.0, ..use_form_style() })
}
```

The label sits on the top border (`FormLabelPosition::Floating`) or above the frame (`Top`).
`text_kind` / `text_weight` set the value text, and the error text uses `error_kind` /
`error_color`.

## Text fields

`FormTextField` never needs a height: it uses the line height `L` of `FormStyle::text_kind`.

- One line (default): the input is `L` high.
- `max_lines: 5`: starts at `min_lines × L`, grows with the text up to `5 × L`, then scrolls
  inside. `max_lines: 0` grows without a limit; `min_lines == max_lines` keeps a fixed height.
- `expands: true`: fills the parent column (for example a message field that takes the rest of
  a page).

```rust
render! {
    FormTextField(
        text: message,
        hint: "Comment",
        max_lines: 5,
        return_key: ReturnKey::Send,
        on_submit: move |_| send(),
    )
    FormTextField(
        text: price,
        keyboard_type: KeyboardType::Number,
        prefix: move || render! { Text(value: "¥") },
    )
}
```

## Select fields

`FormEnumModalField` (`values` + `label_builder`), `FormMapModalField` (`(key, label)` pairs)
and `FormNumModalField` (`begin` / `end` / `interval` / `unit_suffix` or `format`) open a
`SelectSheet` (a `ModalBottomSheet` that tints the selected row and scrolls past
`max_height`). Whisker has no anchored dropdown menu, so the dropdown fields are the same
widgets. `FormNumModalField` takes `begin` and `end` as signals, so a limit loaded later
updates the options.

```rust
render! {
    FormNumModalField(
        value: quantity,
        begin: 1,
        end: limit,
        format: move |n: i64| format!("{n} units"),
        enabled: computed(move || limit.get() > 0),
    )
}
```

## Dates

`FormDateField` opens year / month / day columns in a bottom sheet between `start_date`
(default 1900-01-01) and `end_date` (default today), starting at `default_date` (2000-01-01)
while empty. `FormDateTimeField` opens the OS date and time pickers
(`FormDateTimePickerMode::DateTime`, `Date` or `Time`). Strings such as `confirm_text` and
`cancel_text` default to English; pass localized strings.

`FormMonthModalField` picks a year and a month (the value is a date on `day`: the 1st, a fixed
day or the last day). `FormDurationField` picks a `std::time::Duration` between `begin` and
`end`; `show_days` adds a day column and `show_seconds` (default on) the second column.
`FormDateTimeRangeField` has no column sheet: it opens the OS date picker for the start and then
for the end (whose earliest day is the start) and stores a `FormDateRange`.

## Chips and tags

`FormChipsField` turns typed text into chips on return (trimmed, no duplicates, at most
`max_chips`); `suggestion` offers matching texts as chips under the input.
`FormMapTagDropdownField` picks several keys of `(key, label)` pairs from a multi-select
`SelectSheet` (`multi_selected` + `done_text`) and shows their labels as chips.

```rust
render! {
    FormChipsField(
        form: form.field(move |tags: Vec<String>| Post { tags, ..form.value() }),
        hint: "Add a tag",
        max_chips: Some(5),
        suggestion: vec!["Rust".to_string(), "Flutter".to_string()],
    )
}
```

## Media

`FormMedia` and `FormMultiMedia` have no picker of their own. `on_tap` receives a ref; pick a file
with any plugin and pass it to `update(uri, media_type)`. With `masamune-whisker-image-picker`:

```rust
use masamune_whisker_image_picker::pick_image;

render! {
    FormMedia(
        form: form.field(move |image: Option<FormMediaValue>| Profile { image, ..form.value() }),
        on_tap: move |media: FormMediaRef| {
            spawn_local(async move {
                if let Ok(Some(picked)) = pick_image(false, 1024).await {
                    media.update(format!("file://{}", picked.path), FormMediaType::Image);
                }
            });
        },
        height: 192.0,
    )
    FormMultiMedia(
        value: photos,
        max_length: Some(4),
        on_tap: move |media: FormMultiMediaRef| {
            spawn_local(async move {
                if let Ok(Some(picked)) = pick_image(false, 1024).await {
                    media.update(format!("file://{}", picked.path), FormMediaType::Image);
                }
            });
        },
    )
}
```

`FormMultiMedia` shows the files as tiles with a remove badge (`FormMultiMediaRef::remove`,
`on_remove`) and hides the add tile at `max_length`.

## Rating and PIN

`FormRatingBar` sets the rating to the tapped star (`allow_half` gives half a star for its left
half) and keeps it between `min` and `max`. `FormPinField` draws `length` boxes over one
transparent numeric input, keeps only digits and calls `on_completed` when every box is filled;
`length_error_text` rejects a partial PIN.

## Custom fields

- `FormFutureField`: `on_tap: move |current: Option<T>| async move { … }` returns the new value
  (`None` keeps it), for example from another page; show it with `parse_to_string` or `builder`.
- `FormBuilder`: any UI. `builder` gets a `FormBuilderRef` (`signal()`, `update`, `error()`);
  `validator` runs with the form.
- `FormContainer`: any children inside the form frame, validated by `validator`.
- `FormListBuilder`: an editable list. `builder` gets `(FormListRef, item, index)`; `top` /
  `bottom` add elements around the rows. Adding and deleting rebuild the rows; `update` keeps
  them, so inputs in a row keep focus.

```rust
render! {
    FormListBuilder(
        form: form.field(move |links: Vec<String>| Profile { links, ..form.value() }),
        initial_value: vec![String::new()],
        builder: move |(list, link, index): (FormListRef<String>, String, usize)| render! {
            FormTextField(
                initial_value: link,
                on_changed: move |text: String| { list.update(index, text); },
                suffix: move || render! {
                    TextButton(label: "Remove", on_tap: move |_| { list.delete_at(index); })
                },
            )
        },
        bottom: move |list: FormListRef<String>| render! {
            TextButton(label: "Add", on_tap: move |_| list.add(String::new()))
        },
    )
}
```
