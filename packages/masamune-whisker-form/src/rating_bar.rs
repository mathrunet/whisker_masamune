//! Star rating (katana_form's `FormRatingBar`).

use masamune_whisker_appearance::{palette, text_with};
use masamune_whisker_ui::style::css_color;
use whisker::css::{AlignItems, Color, Overflow, PositionKind};
use whisker::event::TouchEvent;
use whisker::prelude::*;
use whisker_icons::{Icon, lucide};

use crate::controller::{FormField, bind_field, check};
use crate::style::{FormStyle, resolve};

/// Rating for a tap at `position` (0 to 1 across the whole bar) of `icon_count` stars: the
/// tapped star counts fully, or half when `allow_half` and its left half was tapped.
pub(crate) fn rating_at(position: f64, icon_count: u32, allow_half: bool) -> f64 {
    let count = icon_count.max(1) as f64;
    let x = position.clamp(0.0, 1.0) * count;
    let index = x.floor().min(count - 1.0);
    if allow_half && x - index < 0.5 {
        index + 0.5
    } else {
        index + 1.0
    }
}

/// `rating` kept between `min` (default 0) and `max` (default `icon_count`), on half steps when
/// `allow_half` and whole steps otherwise.
pub(crate) fn clamp_rating(
    rating: f64,
    min: Option<f64>,
    max: Option<f64>,
    icon_count: u32,
    allow_half: bool,
) -> f64 {
    let step = if allow_half { 2.0 } else { 1.0 };
    let rounded = (rating * step).round() / step;
    let max = max.unwrap_or(icon_count as f64);
    rounded.clamp(min.unwrap_or(0.0).min(max), max)
}

/// How much of the star at `index` is filled (0 to 1) for `rating`.
pub(crate) fn star_fill(rating: f64, index: u32) -> f64 {
    (rating - index as f64).clamp(0.0, 1.0)
}

/// Row of stars (katana_form's `FormRatingBar`). Tapping a star sets the rating to it (its left
/// half gives a half star with `allow_half`); the rating stays between `min` and `max`.
#[component]
pub fn form_rating_bar(
    /// The form this field belongs to and its `on_saved` (see [`crate::FormController::field`]).
    form: Option<FormField<Option<f64>>>,
    /// Two-way bound rating. Without it the field keeps its own rating, starting at
    /// `initial_value`.
    value: Option<RwSignal<Option<f64>>>,
    /// Initial rating, also restored by `FormController::reset`.
    #[prop(default = None)]
    initial_value: Option<f64>,
    /// Number of stars.
    #[prop(default = 5)]
    icon_count: u32,
    /// Size of each star.
    #[prop(default = 40.0)]
    icon_size: f32,
    /// Allows half stars.
    #[prop(default = true)]
    allow_half: bool,
    /// Lowest rating a tap gives.
    #[prop(default = None)]
    min: Option<f64>,
    /// Highest rating a tap gives (default `icon_count`).
    #[prop(default = None)]
    max: Option<f64>,
    /// Color of the filled part; defaults to [`FormStyle::active_color`].
    active_color: Option<Color>,
    /// Color of the empty stars; defaults to the palette's `disabled`.
    inactive_color: Option<Color>,
    /// Shows the rating as a number after the stars.
    #[prop(default = false)]
    show_label: bool,
    /// Error shown when nothing is rated while validating.
    empty_error_text: Option<Signal<String>>,
    /// Returns an error message for an invalid rating, or `None`.
    validator: Option<Callback<Option<f64>, Option<String>>>,
    /// Called with the new rating.
    on_changed: Option<Callback<Option<f64>>>,
    /// `false` makes the bar read-only (katana_form's `readOnly`).
    #[prop(default = Signal::from(true))]
    enabled: Signal<bool>,
    /// Look of the label and the error; defaults to the provided [`FormStyle`].
    style: Option<FormStyle>,
) -> Element {
    let style = resolve(style);
    let reset_value = match value {
        Some(value) if initial_value.is_none() => value.get_untracked(),
        _ => initial_value,
    };
    let rating = value.unwrap_or_else(|| signal(initial_value));
    let error = signal(None::<String>);
    bind_field(
        form,
        move || rating.get_untracked(),
        move |v: &Option<f64>| check(v, Option::is_none, empty_error_text, validator),
        error,
        move || rating.set(reset_value),
    );
    let active = css_color(active_color.unwrap_or(style.active_color));
    let inactive = css_color(inactive_color.unwrap_or(palette().disabled));
    let stars = StoredValue::new((0..icon_count).collect::<Vec<u32>>());
    let colors = StoredValue::new((active, inactive));
    let size = format!("{icon_size}");
    let size = StoredValue::new(size);
    let on_tap = move |event: TouchEvent| {
        if !enabled.try_get_untracked().unwrap_or(false) {
            return;
        }
        let (Some(offset), Some(bounds)) = (event.offset, event.size) else {
            return;
        };
        if bounds.width <= 0.0 {
            return;
        }
        let picked = rating_at(offset.x / bounds.width, icon_count, allow_half);
        let picked = Some(clamp_rating(picked, min, max, icon_count, allow_half));
        rating.set(picked);
        error.set(None);
        if let Some(on_changed) = on_changed {
            on_changed.run(picked);
        }
    };
    let label = computed(move || {
        rating
            .get()
            .map(|r| {
                if r.fract() == 0.0 {
                    format!("{r:.0}")
                } else {
                    format!("{r:.1}")
                }
            })
            .unwrap_or_default()
    });
    let accessibility = computed(move || {
        Accessibility::new()
            .label(format!("{} / {icon_count}", label.get()))
            .role(AccessibilityRole::Adjustable)
    });
    let error_value = computed(move || error.get().unwrap_or_default());
    render! {
        View(
            style: css!(display: Display::Flex, flex_direction: FlexDirection::Column, gap: px(4)),
        ) {
            View(
                style: css!(
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    gap: px(8),
                ),
            ) {
                View(
                    style: css!(display: Display::Flex, flex_direction: FlexDirection::Row),
                    accessibility: accessibility,
                    on_tap: on_tap,
                ) {
                    ForEach(
                        each: move || stars.get(),
                        key: |i: &u32| *i,
                        children: move |i: ReadSignal<u32>| {
                            let i = i.get_untracked();
                            let filled = computed(move || {
                                css!(
                                    position: PositionKind::Absolute,
                                    top: px(0),
                                    left: px(0),
                                    height: px(icon_size),
                                    width: px(icon_size * star_fill(rating.get().unwrap_or(0.0), i) as f32),
                                    overflow: Overflow::Hidden,
                                )
                            });
                            render! {
                                View(
                                    style: css!(
                                        position: PositionKind::Relative,
                                        width: px(icon_size),
                                        height: px(icon_size),
                                    ),
                                ) {
                                    Icon(svg: lucide::Star, color: colors.get().1, size: size.get())
                                    View(style: filled) {
                                        Icon(
                                            svg: lucide::Star,
                                            color: colors.get().0,
                                            size: size.get(),
                                        )
                                    }
                                }
                            }
                        },
                    )
                }
                Show(when: move || show_label) {
                    Text(value: label, style: style.text_style(style.text_color))
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
    fn taps_give_whole_or_half_stars() {
        assert_eq!(rating_at(0.0, 5, true), 0.5);
        assert_eq!(rating_at(0.15, 5, true), 1.0);
        assert_eq!(rating_at(0.5, 5, true), 3.0);
        assert_eq!(rating_at(0.45, 5, true), 2.5);
        assert_eq!(rating_at(0.45, 5, false), 3.0);
        assert_eq!(rating_at(1.0, 5, false), 5.0);
        assert_eq!(rating_at(2.0, 5, true), 5.0);
    }

    #[test]
    fn ratings_are_clamped_and_stepped() {
        assert_eq!(clamp_rating(0.5, Some(1.0), None, 5, true), 1.0);
        assert_eq!(clamp_rating(5.0, None, Some(4.0), 5, true), 4.0);
        assert_eq!(clamp_rating(2.3, None, None, 5, true), 2.5);
        assert_eq!(clamp_rating(2.3, None, None, 5, false), 2.0);
        assert_eq!(clamp_rating(9.0, None, None, 5, false), 5.0);
    }

    #[test]
    fn stars_fill_up_to_the_rating() {
        assert_eq!(star_fill(2.5, 0), 1.0);
        assert_eq!(star_fill(2.5, 2), 0.5);
        assert_eq!(star_fill(2.5, 3), 0.0);
    }
}
