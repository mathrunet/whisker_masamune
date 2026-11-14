//! Custom fields (katana_form's `FormFutureField`, `FormBuilder` and `FormContainer`).

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use masamune_whisker_appearance::text_with;
use masamune_whisker_ui::Slot;
use whisker::prelude::*;
use whisker_icons::lucide;

use crate::controller::{FormField, bind_field, check};
use crate::frame::{FormFieldFrame, PickedValue};
use crate::style::{FormStyle, resolve};

type BoxedFuture<T> = Pin<Box<dyn Future<Output = Option<T>>>>;

/// The `on_tap` of [`FormFutureField`]: an async function from the current value to the new
/// one (`None` keeps the value). Made from any `Fn(Option<T>) -> impl Future<Output =
/// Option<T>>` closure.
pub struct FormFutureTap<T: 'static>(Rc<dyn Fn(Option<T>) -> BoxedFuture<T>>);

impl<T: 'static> Clone for FormFutureTap<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: 'static, F, Fut> From<F> for FormFutureTap<T>
where
    F: Fn(Option<T>) -> Fut + 'static,
    Fut: Future<Output = Option<T>> + 'static,
{
    fn from(f: F) -> Self {
        Self(Rc::new(move |current| Box::pin(f(current))))
    }
}

impl<T: 'static> FormFutureTap<T> {
    /// Runs the function with the current value.
    pub fn run(&self, current: Option<T>) -> BoxedFuture<T> {
        (self.0)(current)
    }
}

/// Field whose value comes from an async function, for example another page or a dialog
/// (katana_form's `FormFutureField`).
///
/// Tapping it awaits `on_tap(current)`; `Some` replaces the value and `None` keeps it. The
/// value is shown with `builder` (any element) or as the text from `parse_to_string`
/// (katana_form's `parseToString`).
#[component]
pub fn form_future_field<T: Clone + PartialEq + 'static>(
    /// Returns the new value from the current one.
    on_tap: FormFutureTap<T>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<T>>>,
    /// Two-way bound value. Without it the field keeps its own value, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<T>>>,
    /// Initial value, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<T>,
    /// Text of a value.
    parse_to_string: Option<Callback<T, String>>,
    /// Element showing a value; replaces `parse_to_string`.
    builder: Option<Callback<T, Element>>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Text shown while empty (katana_form's `hintText`).
    hint: Option<Signal<String>>,
    /// Error shown when empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid value, or `None`.
    validator: Option<Callback<Option<T>, Option<String>>>,
    /// Called when `on_tap` returns a value.
    on_changed: Option<Callback<Option<T>>>,
    /// `false` disables the field.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Trailing icon; `None` hides it.
    #[prop(default = Some(lucide::ChevronRight))]
    icon: Option<&'static str>,
    /// Shown before the value.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the value.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Look of the field; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let (prefix, suffix) = (prefix.clone(), suffix.clone());
    let reset_value = match value {
        Some(value) if initial_value.is_none() => value.get_untracked(),
        _ => initial_value.clone(),
    };
    let value = value.unwrap_or_else(|| signal(initial_value.clone()));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || value.get_untracked(),
        move |v: &Option<T>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || value.set(reset_value.clone()),
    );
    let busy = signal(false);
    let on_tap = StoredValue::new(on_tap.clone());
    let open = Callback::new(move |()| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        let future = on_tap.get().run(value.get_untracked());
        spawn_local(async move {
            let picked = future.await;
            let _ = busy.try_set(false);
            if let Some(picked) = picked
                && value.try_set(Some(picked.clone()))
                && let Some(on_changed) = on_changed
            {
                on_changed.run(Some(picked));
            }
        });
    });
    // Every new value rebuilds the element of `builder`.
    let generation = signal(0u64);
    effect(move || {
        value.with(|_| ());
        generation.update_untracked(|g| *g += 1);
    });
    let built = computed(move || {
        let current = value.get();
        let generation = generation.get_untracked();
        match (builder.is_some(), current) {
            (true, Some(current)) => vec![(generation, current)],
            _ => Vec::new(),
        }
    });
    let shown = computed(move || match (builder, parse_to_string, value.get()) {
        (None, Some(parse), Some(current)) => parse.run(current),
        _ => String::new(),
    });
    let show_text = computed(move || builder.is_none() || value.with(Option::is_none));
    let disabled = computed(move || !enabled.get() || busy.get());
    render! {
        FormFieldFrame(
            label_option: label,
            error: error.read_only(),
            disabled: computed(move || !enabled.get()),
            prefix: prefix.clone(),
            suffix: suffix.clone(),
            style: style,
            on_tap: open,
        ) {
            ForEach(
                each: move || built.get(),
                key: |(generation, _): &(u64, T)| *generation,
                children: move |item: ReadSignal<(u64, T)>| {
                    let (_, current) = item.get_untracked();
                    match builder {
                        Some(builder) => render! {
                            View(
                                style: css!(display: Display::Flex, flex_grow: 1.0, min_width: px(0)),
                            ) {
                                {builder.run(current.clone())}
                            }
                        },
                        None => render! {
                            Fragment {
                            }
                        },
                    }
                },
            )
            Show(when: move || show_text.get()) {
                PickedValue(
                    value: shown,
                    hint: hint.unwrap_or_default(),
                    icon_option: icon,
                    disabled: disabled,
                    style: style,
                )
            }
        }
    }
}

/// Handle given to [`FormBuilder`]'s `builder` (katana_form's `FormBuilderRef`).
pub struct FormBuilderRef<T: 'static> {
    value: RwSignal<T>,
    error: RwSignal<Option<String>>,
    on_changed: Option<Callback<T>>,
}

impl<T: 'static> Clone for FormBuilderRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for FormBuilderRef<T> {}

impl<T: Clone + 'static> FormBuilderRef<T> {
    /// The current value (not tracked).
    pub fn value(&self) -> T {
        self.value.get_untracked()
    }

    /// The value as a signal, to show it reactively.
    pub fn signal(&self) -> ReadSignal<T> {
        self.value.read_only()
    }

    /// Replaces the value and clears the error.
    pub fn update(&self, value: T) {
        self.value.set(value.clone());
        self.error.set(None);
        if let Some(on_changed) = self.on_changed {
            on_changed.run(value);
        }
    }

    /// The error of the last validation.
    pub fn error(&self) -> ReadSignal<Option<String>> {
        self.error.read_only()
    }
}

/// A field with any UI (katana_form's `FormBuilder`). `builder` is called once with a
/// [`FormBuilderRef`]: read the value from `ref.signal()` and change it with `ref.update`.
/// `validator` runs when the form validates and its message is shown under the element.
#[component]
pub fn form_builder<T: Clone + 'static>(
    /// Initial value, also restored by `FormController::reset`.
    initial_value: T,
    /// Builds the field's UI.
    builder: Callback<FormBuilderRef<T>, Element>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<T>>,
    /// Two-way bound value. Without it the field keeps its own value.
    value: Option<RwSignal<T>>,
    /// Returns an error message for an invalid value, or `None`.
    validator: Option<Callback<T, Option<String>>>,
    /// Called with every value passed to `update`.
    on_changed: Option<Callback<T>>,
    /// Label above the element (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// Look of the label and the error; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = initial_value.clone();
    let value = value.unwrap_or_else(|| signal(initial_value.clone()));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || value.get_untracked(),
        move |v: &T| validator.and_then(|validator| validator.run(v.clone())),
        error,
        move || value.set(reset_value.clone()),
    );
    let field = FormBuilderRef {
        value,
        error,
        on_changed,
    };
    let has_label = label.is_some();
    let label_value = computed(move || label.map(|l| l.get()).unwrap_or_default());
    let error_value = computed(move || error.get().unwrap_or_default());
    let (error_top, error_h) = style.error_padding;
    render! {
        View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Column)) {
            Show(when: move || has_label) {
                Text(
                    value: label_value,
                    style: text_with(style.label_kind, style.label_color).padding_bottom(px(4)),
                )
            }
            {builder.run(field)}
            Show(when: move || error.get().is_some()) {
                Text(
                    value: error_value,
                    style: text_with(style.error_kind, style.error_color).padding((
                        px(error_top),
                        px(error_h),
                        px(0),
                        px(error_h),
                    )),
                )
            }
        }
    }
}

/// Any children in a form frame that takes part in validation (katana_form's
/// `FormContainer`). `validator` runs when the form validates; its message colors the frame
/// and is shown under it. Nothing is saved.
#[component]
pub fn form_container(
    /// The form whose validation includes this container.
    form: Option<FormField<()>>,
    /// Returns an error message, or `None`.
    validator: Option<Callback<(), Option<String>>>,
    /// Label on the frame (katana_form's `labelText`).
    label: Option<Signal<String>>,
    /// `false` draws the frame disabled.
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Grows to fill the parent column.
    #[prop(default = false)]
    expand: bool,
    /// Shown before the children.
    #[prop(default = Slot::default())]
    prefix: Slot,
    /// Shown after the children.
    #[prop(default = Slot::default())]
    suffix: Slot,
    /// Called when the frame is tapped.
    on_tap: Option<Callback<()>>,
    /// Look of the frame; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
    children: Children,
) -> Element {
    let children = StoredValue::new(children.clone());
    let error = signal(None::<String>);
    bind_field(
        form,
        || (),
        move |_: &()| validator.and_then(|validator| validator.run(())),
        error,
        || {},
    );
    render! {
        FormFieldFrame(
            label_option: label,
            error: error.read_only(),
            disabled: computed(move || !enabled.get()),
            expand: expand,
            prefix: prefix.clone(),
            suffix: suffix.clone(),
            style_option: style,
            on_tap_option: on_tap,
        ) {
            {(children.get())()}
        }
    }
}
