//! Screen skeleton (Flutter's `Scaffold`).
//!
//! From top to bottom: `app_bar`, the body (children), `bottom_sheet` and
//! `bottom_navigation_bar`. The scaffold owns the keyboard and safe-area spacing:
//!
//! - Body: shrinks when the keyboard opens.
//! - `bottom_sheet`: rises with the keyboard (input bars, "Next" buttons).
//! - `bottom_navigation_bar`: stays behind the keyboard (tab bars).
//! - `overlay`: a layer over the whole screen (sheets, dialogs).
//!
//! Apps run edge-to-edge, so the OS never resizes the window. The keyboard height already
//! includes the navigation bar, so the bottom safe area is not added while it is open.
//!
//! Safe areas take the color of the bar next to them: `AppBar` paints the top safe area with
//! its own color, and the bottom safe area is painted with `bottom_navigation_bar_color` or
//! `bottom_sheet_color`.
//!
//! Nested scaffolds (a screen inside a tab) do not reserve space their parent already used.

use std::rc::Rc;

use masamune_whisker_appearance::palette;
use whisker::css::{Color, Display, PositionKind};
use whisker::prelude::*;
use whisker_keyboard::keyboard_height;
use whisker_safe_area::safe_area_insets;

use crate::style::column;

/// Content of a scaffold slot. Pass `move || render! { … }`, like `Show`'s `fallback`.
#[derive(Clone, Default)]
pub struct Slot(Option<Rc<dyn Fn() -> Element>>);

impl<F: Fn() -> Element + 'static> From<F> for Slot {
    fn from(f: F) -> Self {
        Slot(Some(Rc::new(f)))
    }
}

impl Slot {
    pub fn is_some(&self) -> bool {
        self.0.is_some()
    }

    pub fn render(&self) -> Option<Element> {
        self.0.as_ref().map(|f| f())
    }
}

/// Space nobody has used yet (top/bottom safe area and the keyboard).
#[derive(Clone, Copy)]
pub struct ScaffoldInsets {
    pub top: ReadSignal<f32>,
    pub bottom: ReadSignal<f32>,
    pub keyboard: ReadSignal<f32>,
}

impl ScaffoldInsets {
    fn root() -> Self {
        let insets = safe_area_insets();
        let keyboard = keyboard_height();
        Self {
            top: computed(move || insets.get().top as f32),
            bottom: computed(move || insets.get().bottom as f32),
            keyboard: computed(move || keyboard.get() as f32),
        }
    }
}

/// Space left by the enclosing scaffold (the whole screen outside any scaffold). Call it in a
/// component body.
pub fn use_scaffold_insets() -> ScaffoldInsets {
    use_context::<ScaffoldInsets>().unwrap_or_else(ScaffoldInsets::root)
}

#[component]
pub fn scaffold(
    /// Top bar. The bar reserves and paints the top safe area itself (`AppBar`).
    #[prop(default = Slot::default())]
    app_bar: Slot,
    /// Area that rises with the keyboard. Reserves the bottom safe area when there is no
    /// `bottom_navigation_bar`.
    #[prop(default = Slot::default())]
    bottom_sheet: Slot,
    /// Background of `bottom_sheet` and the safe area below it. Transparent shows
    /// `background_color`.
    #[prop(default = Color::rgba(0, 0, 0, 0.0))]
    bottom_sheet_color: Color,
    /// Area that stays behind the keyboard (tab bars). Reserves the bottom safe area.
    #[prop(default = Slot::default())]
    bottom_navigation_bar: Slot,
    /// Background of `bottom_navigation_bar` and the safe area below it. Pass the bar's own
    /// background (the default matches `BottomNavigationBar`).
    #[prop(default = palette().bar)]
    bottom_navigation_bar_color: Color,
    /// Layer over the whole screen.
    #[prop(default = Slot::default())]
    overlay: Slot,
    #[prop(default = palette().background)] background_color: Color,
    /// Lifts the body and `bottom_sheet` by the keyboard height (same name as Flutter).
    #[prop(default = true)]
    resize_to_avoid_bottom_inset: bool,
    /// Without `app_bar`, pads the body by the top safe area.
    #[prop(default = true)]
    safe_area_top: bool,
    /// Without bottom areas, pads the body by the bottom safe area.
    #[prop(default = false)]
    safe_area_bottom: bool,
    /// Body (shrinks when the keyboard opens).
    children: Children,
) -> Element {
    let parent = use_scaffold_insets();
    let has_app_bar = app_bar.is_some();
    let has_sheet = bottom_sheet.is_some();
    let has_nav = bottom_navigation_bar.is_some();
    let pad_top = !has_app_bar && safe_area_top;
    let pad_bottom = !has_sheet && !has_nav && safe_area_bottom;

    let keyboard = computed(move || {
        if resize_to_avoid_bottom_inset {
            parent.keyboard.get()
        } else {
            0.0
        }
    });
    let keyboard_visible = computed(move || keyboard.get() > 0.0);
    // While the keyboard is open the bottom safe area is behind it.
    let free_bottom = computed(move || {
        if keyboard_visible.get() {
            0.0
        } else {
            parent.bottom.get()
        }
    });

    // Space handed down to nested screens.
    provide_context(ScaffoldInsets {
        top: computed(move || {
            if has_app_bar || pad_top {
                0.0
            } else {
                parent.top.get()
            }
        }),
        bottom: computed(move || {
            if has_sheet || has_nav || pad_bottom {
                0.0
            } else {
                free_bottom.get()
            }
        }),
        keyboard: computed(move || {
            if resize_to_avoid_bottom_inset {
                0.0
            } else {
                parent.keyboard.get()
            }
        }),
    });

    let root_style = computed(move || {
        column()
            .flex_grow(1.0)
            .position(PositionKind::Relative)
            .background_color(background_color)
            .padding_bottom(px(keyboard.get()))
    });
    let body_style = computed(move || {
        column()
            .position(PositionKind::Relative)
            .flex_grow(1.0)
            .flex_shrink(1.0)
            .flex_basis(px(0))
            .padding_top(px(if pad_top { parent.top.get() } else { 0.0 }))
            .padding_bottom(px(if pad_bottom { free_bottom.get() } else { 0.0 }))
    });
    let sheet_style = computed(move || {
        column()
            .flex_shrink(0.0)
            .padding_bottom(px(if has_nav { 0.0 } else { free_bottom.get() }))
            .background_color(bottom_sheet_color)
    });
    let nav_style = computed(move || {
        let nav = column()
            .flex_shrink(0.0)
            .padding_bottom(px(parent.bottom.get()))
            .background_color(bottom_navigation_bar_color);
        if keyboard_visible.get() {
            nav.display(Display::None)
        } else {
            nav
        }
    });

    let projected = children.clone();
    let (app_bar, bottom_sheet, bottom_navigation_bar, overlay) = (
        app_bar.clone(),
        bottom_sheet.clone(),
        bottom_navigation_bar.clone(),
        overlay.clone(),
    );
    render! {
        View(style: root_style) {
            {app_bar.render()}
            View(style: body_style) {
                {projected()}
            }
            {has_sheet.then(|| render! { View(style: sheet_style) { {bottom_sheet.render()} } })}
            {has_nav.then(|| render! { View(style: nav_style) { {bottom_navigation_bar.render()} } })}
            {overlay.render()}
        }
    }
}

/// Pads its children by the safe area the enclosing scaffold left (plus `padding`).
#[component]
pub fn safe_area(
    #[prop(default = true)] top: bool,
    #[prop(default = true)] bottom: bool,
    #[prop(default = 0.0)] padding: f32,
    children: Children,
) -> Element {
    let insets = use_scaffold_insets();
    let style = computed(move || {
        column()
            .flex_grow(1.0)
            .padding_top(px(padding + if top { insets.top.get() } else { 0.0 }))
            .padding_bottom(px(padding + if bottom { insets.bottom.get() } else { 0.0 }))
            .padding_left(px(padding))
            .padding_right(px(padding))
    });
    let projected = children.clone();
    render! {
        View(style: style) {
            {projected()}
        }
    }
}
