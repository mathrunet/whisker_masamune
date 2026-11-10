//! The app root: an owner for long-running work and app-wide rebuilds.

use std::cell::Cell;
use std::future::Future;

use whisker::Owner;
use whisker::css::{Display, FlexDirection, PositionKind};
use whisker::prelude::*;

thread_local! {
    static APP_OWNER: Cell<Option<Owner>> = const { Cell::new(None) };
}

whisker::runtime_local! {
    static GENERATION: RwSignal<u64> = signal(0);
}

/// Remembers the current owner as the app owner. [`AppRoot`] does this; call it yourself only
/// when you do not use `AppRoot`.
pub fn set_app_owner() {
    APP_OWNER.with(|owner| owner.set(Owner::current()));
}

/// Runs `f` inside the app owner, so effects and tasks created in it outlive the current screen.
pub fn with_app_owner<R>(f: impl FnOnce() -> R) -> R {
    match APP_OWNER.with(Cell::get).filter(|owner| owner.is_alive()) {
        Some(owner) => owner.with(f),
        None => f(),
    }
}

/// Starts async work that keeps running after the calling component goes away (requests,
/// purchases, account operations started from a button or a dialog).
pub fn spawn_app(future: impl Future<Output = ()> + 'static) {
    with_app_owner(|| spawn_local(future));
}

/// Number of rebuilds so far.
pub fn generation() -> ReadSignal<u64> {
    GENERATION.with(|signal| signal.read_only())
}

/// Recreates everything under [`AppRoot`] (the router starts again from `/`). Use after a
/// theme or language change so every screen picks up the new values. State provided above
/// `AppRoot` survives.
pub fn rebuild() {
    GENERATION.with(|signal| signal.update(|generation| *generation += 1));
}

/// Root of the app UI. Owns long-running work ([`spawn_app`]) and recreates its children on
/// [`rebuild`].
#[component]
pub fn app_root(children: Children) -> Element {
    set_app_owner();
    let generation = generation();
    let children = children.clone();
    render! {
        ForEach(
            each: move || vec![generation.get()],
            key: |generation: &u64| *generation,
            children: move |_: ReadSignal<u64>| {
                let children = children.clone();
                render! {
                    View(style: css!(
                        flex_grow: 1.0,
                        display: Display::Flex,
                        flex_direction: FlexDirection::Column,
                        position: PositionKind::Relative,
                    )) {
                        {children()}
                    }
                }
            },
        )
    }
}
