//! Editable list of items (katana_form's `FormListBuilder`).

use masamune_whisker_appearance::text_with;
use whisker::prelude::*;

use crate::controller::{FormField, bind_field, check};
use crate::style::{FormStyle, resolve};

/// Handle given to [`FormListBuilder`]'s builders (katana_form's `FormListBuilderRef`).
///
/// Adding and deleting rebuild the rows; `update` changes an item without rebuilding, so
/// inputs inside the rows keep their focus.
pub struct FormListRef<T: 'static> {
    items: RwSignal<Vec<T>>,
    generation: RwSignal<u64>,
    on_changed: Option<Callback<Vec<T>>>,
}

impl<T: 'static> Clone for FormListRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for FormListRef<T> {}

impl<T: Clone + 'static> FormListRef<T> {
    /// The current items.
    pub fn items(&self) -> Vec<T> {
        self.items.try_get_untracked().unwrap_or_default()
    }

    /// The items as a signal.
    pub fn signal(&self) -> ReadSignal<Vec<T>> {
        self.items.read_only()
    }

    /// Number of items.
    pub fn len(&self) -> usize {
        self.items.with_untracked(Vec::len)
    }

    /// Whether there is no item.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Appends `item`.
    pub fn add(&self, item: T) {
        self.insert(self.len(), item);
    }

    /// Inserts `item` at `index` (clamped to the end).
    pub fn insert(&self, index: usize, item: T) {
        self.change(true, |items| {
            items.insert(index.min(items.len()), item);
            true
        });
    }

    /// Replaces the item at `index`. Returns `false` when there is none.
    pub fn update(&self, index: usize, item: T) -> bool {
        self.change(false, |items| match items.get_mut(index) {
            Some(slot) => {
                *slot = item;
                true
            }
            None => false,
        })
    }

    /// Removes and returns the item at `index`.
    pub fn delete_at(&self, index: usize) -> Option<T> {
        let mut removed = None;
        self.change(true, |items| {
            if index < items.len() {
                removed = Some(items.remove(index));
            }
            removed.is_some()
        });
        removed
    }

    /// Removes the first item equal to `item`. Returns whether one was removed.
    pub fn delete(&self, item: &T) -> bool
    where
        T: PartialEq,
    {
        match self.items().iter().position(|i| i == item) {
            Some(index) => self.delete_at(index).is_some(),
            None => false,
        }
    }

    fn change(&self, rebuild: bool, f: impl FnOnce(&mut Vec<T>) -> bool) -> bool {
        let mut items = self.items();
        if !f(&mut items) {
            return false;
        }
        self.items.set(items.clone());
        if rebuild {
            self.generation.update(|g| *g += 1);
        }
        if let Some(on_changed) = self.on_changed {
            on_changed.run(items);
        }
        true
    }
}

/// Rows of `(generation, index, item)` for the list; the key `(generation, index)` changes on
/// every add and delete, so those rebuild the rows while `update` keeps them.
pub(crate) fn list_rows<T: Clone>(generation: u64, items: &[T]) -> Vec<(u64, usize, T)> {
    items
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, item)| (generation, index, item))
        .collect()
}

/// An editable list with one row per item (katana_form's `FormListBuilder`).
///
/// `builder` gets a [`FormListRef`], the item and its index and returns the row; `top` and
/// `bottom` add elements above and below (an add button, for example). The value is the
/// list; `empty_error_text` rejects an empty list.
#[component]
pub fn form_list_builder<T: Clone + 'static>(
    /// Builds the row of an item.
    builder: Callback<(FormListRef<T>, T, usize), Element>,
    /// Element above the rows.
    top: Option<Callback<FormListRef<T>, Element>>,
    /// Element below the rows.
    bottom: Option<Callback<FormListRef<T>, Element>>,
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Vec<T>>>,
    /// Two-way bound items. Without it the list keeps its own items, starting at
    /// `initial_value`.
    value: Option<RwSignal<Vec<T>>>,
    /// Initial items, also restored by `FormController::reset`.
    #[prop(default = Vec::new())]
    initial_value: Vec<T>,
    /// Error shown when the list is empty while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for invalid items, or `None`.
    validator: Option<Callback<Vec<T>, Option<String>>>,
    /// Called with the items after every change through the ref.
    on_changed: Option<Callback<Vec<T>>>,
    /// Space between the rows.
    #[prop(default = 8.0)]
    gap: f32,
    /// Look of the error; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = match value {
        Some(value) if initial_value.is_empty() => value.get_untracked(),
        _ => initial_value.clone(),
    };
    let items = value.unwrap_or_else(|| signal(initial_value.clone()));
    let generation = signal(0u64);
    let error = signal(None::<String>);
    bind_field(
        form,
        move || items.get_untracked(),
        move |v: &Vec<T>| check(v, Vec::is_empty, empty_error_text, validator),
        error,
        move || {
            items.set(reset_value.clone());
            generation.update(|g| *g += 1);
        },
    );
    let list = FormListRef {
        items,
        generation,
        on_changed,
    };
    let top_view = move || top.map(|top| top.run(list));
    let bottom_view = move || bottom.map(|bottom| bottom.run(list));
    let error_value = computed(move || error.get().unwrap_or_default());
    render! {
        View(
            style: css!(display: Display::Flex, flex_direction: FlexDirection::Column, gap: px(gap)),
        ) {
            {top_view()}
            ForEach(
                each: move || items.with(|items| list_rows(generation.get(), items)),
                key: |(generation, index, _): &(u64, usize, T)| (*generation, *index),
                children: move |row: ReadSignal<(u64, usize, T)>| {
                    let (_, index, item) = row.get_untracked();
                    builder.run((list, item, index))
                },
            )
            Fragment {
                {bottom_view()}
            }
            Show(when: move || error.get().is_some()) {
                Text(value: error_value, style: text_with(style.error_kind, style.error_color))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use whisker::Owner;

    use super::*;
    use crate::controller::FormController;

    fn with_runtime(f: impl FnOnce()) {
        let runtime =
            whisker::runtime::RuntimeContext::new(whisker::runtime::RuntimeWakeHandle::new(|| {}));
        runtime.enter(|| {
            let owner = Owner::new(None);
            owner.with(f);
            owner.dispose();
        });
        runtime.shutdown();
    }

    #[test]
    fn rows_are_keyed_by_generation_and_index() {
        assert_eq!(list_rows(3, &["a", "b"]), vec![(3, 0, "a"), (3, 1, "b")]);
    }

    #[test]
    fn refs_add_update_and_delete() {
        with_runtime(|| {
            let list = FormListRef {
                items: signal(vec![1, 2]),
                generation: signal(0),
                on_changed: None,
            };
            list.add(3);
            list.insert(0, 0);
            assert_eq!(list.items(), vec![0, 1, 2, 3]);
            assert_eq!(list.generation.get_untracked(), 2);
            assert!(list.update(1, 10));
            assert!(!list.update(9, 10));
            assert_eq!(list.generation.get_untracked(), 2);
            assert_eq!(list.delete_at(0), Some(0));
            assert_eq!(list.delete_at(9), None);
            assert!(list.delete(&3));
            assert!(!list.delete(&3));
            assert_eq!(list.items(), vec![10, 2]);
            assert_eq!(list.len(), 2);
        });
    }

    #[test]
    fn lists_validate_and_save_through_the_form() {
        with_runtime(|| {
            let form = FormController::new(Vec::<String>::new());
            let items = signal(Vec::<String>::new());
            let error = signal(None::<String>);
            let generation = signal(0u64);
            bind_field(
                Some(form.field(|tags: Vec<String>| tags)),
                move || items.get_untracked(),
                |v: &Vec<String>| check(v, Vec::is_empty, Some(Signal::from("empty")), None),
                error,
                move || items.set(Vec::new()),
            );
            let list = FormListRef {
                items,
                generation,
                on_changed: None,
            };
            assert_eq!(form.validate(), None);
            assert_eq!(error.get_untracked(), Some("empty".into()));
            list.add("rust".into());
            assert_eq!(form.validate(), Some(vec!["rust".to_string()]));
            form.reset();
            assert!(list.is_empty());
        });
    }
}
