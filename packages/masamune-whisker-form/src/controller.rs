//! Form state (katana_form's `FormController`).
//!
//! A [`FormController`] holds the form value and the fields that belong to it. Each field
//! registers a validate / save / reset step while it is mounted and removes it when it is
//! unmounted. [`FormController::validate`] runs every validator (each field shows its own
//! error), and only when all of them pass runs every `on_saved` in registration order and
//! returns the updated value.
//!
//! ```ignore
//! #[derive(Clone, Default)]
//! struct Profile { name: String, email: String }
//!
//! let form = FormController::new(Profile::default());
//! render! {
//!     FormTextField(
//!         form: form.field(move |name: String| Profile { name, ..form.value() }),
//!         empty_error_text: "Enter your name",
//!     )
//!     FormTextField(
//!         form: form.field(move |email: String| Profile { email, ..form.value() }),
//!         keyboard_type: KeyboardType::Email,
//!     )
//!     FormButton(label: "Save", on_tap: move |_| {
//!         if let Some(profile) = form.validate() {
//!             save(profile);
//!         }
//!     })
//! }
//! ```

use std::rc::Rc;

use whisker::prelude::*;

/// One registered field.
#[derive(Clone)]
struct FieldEntry {
    id: u64,
    validate: Rc<dyn Fn() -> bool>,
    save: Rc<dyn Fn()>,
    reset: Rc<dyn Fn()>,
}

/// The fields of a form. A `Copy` handle shared by [`FormController`] and [`FormField`].
#[derive(Clone, Copy)]
struct FormRegistry {
    entries: RwSignal<Vec<FieldEntry>>,
    next_id: RwSignal<u64>,
}

impl FormRegistry {
    fn new() -> Self {
        Self {
            entries: signal(Vec::new()),
            next_id: signal(0),
        }
    }

    fn register(&self, entry: impl FnOnce(u64) -> FieldEntry) -> u64 {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        let entry = entry(id);
        self.entries.update(|entries| entries.push(entry));
        id
    }

    fn unregister(&self, id: u64) {
        // The form may already be gone when its page unmounts before the field.
        let _ = self
            .entries
            .try_update(|entries| entries.retain(|entry| entry.id != id));
    }

    /// The fields at this moment, so the steps can register or unregister fields themselves.
    fn snapshot(&self) -> Vec<FieldEntry> {
        self.entries.with_untracked(Clone::clone)
    }

    fn len(&self) -> usize {
        self.entries.with_untracked(Vec::len)
    }
}

/// State of a form (katana_form's `FormController`): the form value and its fields.
///
/// `Copy`, so it can move into as many closures as needed. Bind a field with
/// [`FormController::field`] (or pass the controller itself when the field has no `on_saved`).
pub struct FormController<T: 'static> {
    value: RwSignal<T>,
    initial: RwSignal<T>,
    registry: FormRegistry,
}

impl<T: 'static> Clone for FormController<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for FormController<T> {}

impl<T: Clone + 'static> FormController<T> {
    /// A form that starts with `value`.
    pub fn new(value: T) -> Self {
        Self {
            value: signal(value.clone()),
            initial: signal(value),
            registry: FormRegistry::new(),
        }
    }

    /// The current form value (not tracked; read [`FormController::signal`] to follow changes).
    pub fn value(&self) -> T {
        self.value.get_untracked()
    }

    /// Replaces the form value, for example with loaded data. [`FormController::reset`] returns
    /// to this value.
    pub fn set_value(&self, value: T) {
        self.initial.set(value.clone());
        self.value.set(value);
    }

    /// The form value as a signal (updated by [`FormController::validate`] and
    /// [`FormController::set_value`]).
    pub fn signal(&self) -> ReadSignal<T> {
        self.value.read_only()
    }

    /// Binds a field to this form. `on_saved` receives the field value when the form is saved
    /// and returns the new form value (katana_form's `onSaved`). Fields are saved in the order
    /// they were mounted, each one starting from the value the previous one returned, so read
    /// the current value with [`FormController::value`] inside `on_saved`.
    pub fn field<V: 'static>(&self, on_saved: impl Fn(V) -> T + 'static) -> FormField<V> {
        let value = self.value;
        FormField {
            registry: self.registry,
            on_saved: Some(Callback::new(move |field_value: V| {
                value.set(on_saved(field_value));
            })),
        }
    }

    /// Runs every field's validator, showing each field's error. When all of them pass, saves
    /// every field (see [`FormController::field`]) and returns the updated value; otherwise
    /// returns `None` and keeps the value.
    pub fn validate(&self) -> Option<T> {
        let fields = self.registry.snapshot();
        let mut valid = true;
        for field in &fields {
            // Every field runs so that every error is shown at once.
            valid = (field.validate)() && valid;
        }
        if !valid {
            return None;
        }
        for field in &fields {
            (field.save)();
        }
        Some(self.value.get_untracked())
    }

    /// Clears every field's error, returns every field to its initial value and the form
    /// value to the one given to [`FormController::new`] or the last
    /// [`FormController::set_value`].
    pub fn reset(&self) {
        for field in self.registry.snapshot() {
            (field.reset)();
        }
        self.value.set(self.initial.get_untracked());
    }

    /// Number of mounted fields.
    pub fn field_count(&self) -> usize {
        self.registry.len()
    }
}

/// The form side of a field: which form it belongs to and what it does on save.
///
/// Made by [`FormController::field`], or converted from a [`FormController`] for a field that
/// only takes part in validation. Custom fields register themselves with
/// [`FormField::register`].
pub struct FormField<V: 'static> {
    registry: FormRegistry,
    on_saved: Option<Callback<V>>,
}

impl<V: 'static> Clone for FormField<V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V: 'static> Copy for FormField<V> {}

impl<T: Clone + 'static, V: 'static> From<FormController<T>> for FormField<V> {
    fn from(form: FormController<T>) -> Self {
        Self {
            registry: form.registry,
            on_saved: None,
        }
    }
}

impl<V: 'static> FormField<V> {
    /// Registers a field with its form until the current component unmounts.
    ///
    /// - `validate` checks the field, shows or clears its error and returns whether it passed.
    /// - `value` returns the field value handed to `on_saved` when the form is saved.
    /// - `reset` returns the field to its initial value and clears its error.
    pub fn register(
        self,
        validate: impl Fn() -> bool + 'static,
        value: impl Fn() -> V + 'static,
        reset: impl Fn() + 'static,
    ) {
        let on_saved = self.on_saved;
        let registry = self.registry;
        let id = registry.register(move |id| FieldEntry {
            id,
            validate: Rc::new(validate),
            save: Rc::new(move || {
                if let Some(on_saved) = on_saved {
                    on_saved.run(value());
                }
            }),
            reset: Rc::new(reset),
        });
        on_cleanup(move || registry.unregister(id));
    }
}

/// Validation shared by the built-in fields: the empty check first, then `validator`.
pub(crate) fn check<V>(
    value: &V,
    is_empty: impl Fn(&V) -> bool,
    empty_error_text: Option<Signal<String>>,
    validator: Option<Callback<V, Option<String>>>,
) -> Option<String>
where
    V: Clone + 'static,
{
    if let Some(text) = empty_error_text
        && is_empty(value)
    {
        return Some(text.try_get_untracked().unwrap_or_default());
    }
    validator.and_then(|validator| validator.run(value.clone()))
}

/// Registers a built-in field: `error` shows the result of `check`, `reset` restores the value.
pub(crate) fn bind_field<V: Clone + 'static>(
    form: Option<FormField<V>>,
    value: impl Fn() -> V + 'static,
    check: impl Fn(&V) -> Option<String> + 'static,
    error: RwSignal<Option<String>>,
    reset: impl Fn() + 'static,
) {
    let Some(form) = form else {
        return;
    };
    let value = Rc::new(value);
    let read = Rc::clone(&value);
    form.register(
        move || {
            let message = check(&value());
            let valid = message.is_none();
            error.set(message);
            valid
        },
        move || read(),
        move || {
            error.set(None);
            reset();
        },
    );
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use whisker::Owner;

    use super::*;

    #[derive(Clone, Debug, Default, PartialEq)]
    struct Profile {
        name: String,
        email: String,
    }

    fn with_runtime<R>(f: impl FnOnce() -> R) -> R {
        let runtime =
            whisker::runtime::RuntimeContext::new(whisker::runtime::RuntimeWakeHandle::new(|| {}));
        let out = runtime.enter(|| {
            let owner = Owner::new(None);
            let out = owner.with(f);
            owner.dispose();
            out
        });
        runtime.shutdown();
        out
    }

    /// A text field like the built-in ones, mounted in its own owner.
    struct TestField {
        text: RwSignal<String>,
        error: RwSignal<Option<String>>,
        owner: Owner,
    }

    fn text_field(form: FormField<String>, initial: &str, required: bool) -> TestField {
        let owner = Owner::new(None);
        let initial = initial.to_owned();
        let (text, error) = owner.with(|| {
            let text = signal(initial.clone());
            let error = signal(None::<String>);
            bind_field(
                Some(form),
                move || text.get_untracked(),
                move |value: &String| {
                    check(
                        value,
                        |v: &String| v.trim().is_empty(),
                        required.then(|| Signal::from("required")),
                        None,
                    )
                },
                error,
                move || text.set(initial.clone()),
            );
            (text, error)
        });
        TestField { text, error, owner }
    }

    #[test]
    fn validate_saves_every_field_in_order() {
        with_runtime(|| {
            let form = FormController::new(Profile::default());
            let name = text_field(
                form.field(move |name| Profile {
                    name,
                    ..form.value()
                }),
                "",
                true,
            );
            let email = text_field(
                form.field(move |email| Profile {
                    email,
                    ..form.value()
                }),
                "",
                false,
            );
            name.text.set("Taro".into());
            email.text.set("taro@example.com".into());
            let saved = form.validate();
            assert_eq!(
                saved,
                Some(Profile {
                    name: "Taro".into(),
                    email: "taro@example.com".into(),
                })
            );
            assert_eq!(form.value(), saved.unwrap());
            assert_eq!(name.error.get_untracked(), None);
        });
    }

    #[test]
    fn failed_validation_shows_errors_and_keeps_the_value() {
        with_runtime(|| {
            let form = FormController::new(Profile::default());
            let saves = Rc::new(RefCell::new(0));
            let counter = Rc::clone(&saves);
            let name = text_field(
                form.field(move |name| {
                    *counter.borrow_mut() += 1;
                    Profile {
                        name,
                        ..form.value()
                    }
                }),
                "",
                true,
            );
            let email = text_field(form.into(), "", true);
            email.text.set("taro@example.com".into());
            assert_eq!(form.validate(), None);
            assert_eq!(name.error.get_untracked(), Some("required".into()));
            assert_eq!(email.error.get_untracked(), None);
            assert_eq!(*saves.borrow(), 0);
            assert_eq!(form.value(), Profile::default());
            name.text.set("   ".into());
            assert_eq!(form.validate(), None);
            name.text.set("Taro".into());
            assert!(form.validate().is_some());
            assert_eq!(name.error.get_untracked(), None);
            assert_eq!(*saves.borrow(), 1);
        });
    }

    #[test]
    fn reset_restores_fields_errors_and_value() {
        with_runtime(|| {
            let form = FormController::new(Profile::default());
            form.set_value(Profile {
                name: "Loaded".into(),
                email: String::new(),
            });
            let name = text_field(
                form.field(move |name| Profile {
                    name,
                    ..form.value()
                }),
                "Loaded",
                true,
            );
            name.text.set(String::new());
            assert_eq!(form.validate(), None);
            assert!(name.error.get_untracked().is_some());
            name.text.set("Changed".into());
            assert!(form.validate().is_some());
            assert_eq!(form.value().name, "Changed");
            name.error.set(Some("stale".into()));
            form.reset();
            assert_eq!(name.text.get_untracked(), "Loaded");
            assert_eq!(name.error.get_untracked(), None);
            assert_eq!(form.value().name, "Loaded");
        });
    }

    #[test]
    fn unmounted_fields_leave_the_form() {
        with_runtime(|| {
            let form = FormController::new(Profile::default());
            let name = text_field(form.into(), "", true);
            let _email = text_field(form.into(), "x", true);
            assert_eq!(form.field_count(), 2);
            assert_eq!(form.validate(), None);
            name.owner.dispose();
            assert_eq!(form.field_count(), 1);
            assert_eq!(form.validate(), Some(Profile::default()));
        });
    }
}
