//! Full-screen pages (Flutter's `TransitionQuery.fullscreen` / `fullscreenDialog`).
//!
//! A full-screen page is declared in two places:
//!
//! - the route opens it with [`fullscreen_transition`] (it rises from below while fading in), and
//! - the page wraps its content in [`Fullscreen`], so every [`AppBar`](super::app_bar::AppBar)
//!   inside shows a close button (X) at the top start by default, including while it is loading.
//!
//! ```ignore
//! Route(path: "items/:id", component: ItemPage, transition: fullscreen_transition())
//!
//! #[component]
//! fn item_page() -> Element {
//!     render! {
//!         Fullscreen {
//!             Scaffold(app_bar: move || render! { AppBar(title: "") {} }) { /* … */ }
//!         }
//!     }
//! }
//! ```

use whisker::prelude::*;
use whisker_router::RouteTransition;

use super::page_transition::fade_upwards_transition;

/// Context marking the subtree of a full-screen page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FullscreenScope;

/// Route transition for full-screen pages: the page rises a little from below while fading in
/// (same as [`fade_upwards_transition`]). Pair it with [`Fullscreen`] in the page.
pub fn fullscreen_transition() -> RouteTransition {
    fade_upwards_transition()
}

/// Marks the current component's subtree as a full-screen page. [`Fullscreen`] calls this; use it
/// directly when wrapping the content is not convenient.
pub fn provide_fullscreen() {
    provide_context(FullscreenScope);
}

/// Whether the caller is inside a full-screen page ([`Fullscreen`] / [`provide_fullscreen`]).
pub fn use_fullscreen() -> bool {
    use_context::<FullscreenScope>().is_some()
}

/// Declares a full-screen page. Every [`AppBar`](super::app_bar::AppBar) inside shows a close
/// button (X) at the top start by default instead of a back button.
#[component]
pub fn fullscreen(children: Children) -> Element {
    provide_fullscreen();
    let projected = children.clone();
    render! {
        Fragment {
            {projected()}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisker::runtime::reactive::Owner;

    fn with_runtime<F: FnOnce() -> T, T>(f: F) -> T {
        let runtime =
            whisker::runtime::RuntimeContext::new(whisker::runtime::RuntimeWakeHandle::new(|| {}));
        runtime.enter(|| {
            let owner = Owner::new(None);
            let out = owner.with(f);
            owner.dispose();
            out
        })
    }

    #[test]
    fn not_fullscreen_by_default() {
        with_runtime(|| assert!(!use_fullscreen()));
    }

    #[test]
    fn fullscreen_after_provide() {
        with_runtime(|| {
            provide_fullscreen();
            assert!(use_fullscreen());
        });
    }

    #[test]
    fn transition_fades_upwards() {
        assert_eq!(fullscreen_transition().name(), "fade-upwards");
    }
}
