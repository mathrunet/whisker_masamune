//! Pull to refresh (Flutter's `RefreshIndicator` and `UniversalListView.onRefresh`).
//!
//! [`RefreshScrollView`] is a vertical `ScrollView` whose native pull-to-refresh control
//! (iOS `UIRefreshControl`, Android `SwipeRefreshLayout`) runs an async [`RefreshCallback`] and
//! keeps the spinner up until the returned future finishes. Other hosts show a plain
//! `ScrollView`.
//!
//! While a refresh runs it holds a `masamune.indicator.show|refresh` trace
//! (`masamune_whisker_logger::start_trace`), like `CircularProgressIndicator`, so slow-operation
//! detectors such as `masamune-whisker-ai-debugger` notice spinners that stay up too long.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use whisker::prelude::*;

use crate::style::column;

/// Trace held while a refresh runs.
pub const REFRESH_TRACE_NAME: &str = "masamune.indicator.show|refresh";

type RefreshFuture = Pin<Box<dyn Future<Output = ()>>>;

/// The `on_refresh` of [`RefreshScrollView`]: an async function run when the user pulls the
/// content down. Made from any `Fn() -> impl Future<Output = ()>` closure, such as
/// `move || async move { reload().await }`.
pub struct RefreshCallback(Rc<dyn Fn() -> RefreshFuture>);

impl Clone for RefreshCallback {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<F, Fut> From<F> for RefreshCallback
where
    F: Fn() -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn from(f: F) -> Self {
        Self(Rc::new(move || Box::pin(f())))
    }
}

impl RefreshCallback {
    /// Starts the function and returns its future.
    pub fn run(&self) -> RefreshFuture {
        (self.0)()
    }
}

/// Marks `refreshing` as running. Returns `false` (and changes nothing) when a refresh is
/// already running, so one pull never starts a second load.
fn begin_refresh(refreshing: RwSignal<bool>) -> bool {
    if refreshing.get_untracked() {
        return false;
    }
    refreshing.set(true);
    true
}

/// Vertical scroll view with pull to refresh (Flutter's `RefreshIndicator`, or
/// `UniversalListView` with `onRefresh`).
///
/// Pulling the content down calls `on_refresh` and shows the native spinner until its future
/// finishes; pulls while it runs are ignored. Without `on_refresh` this is a plain
/// `ScrollView`. On hosts other than iOS and Android there is no pull gesture.
///
/// The children sit in a column (`content_style`) that is at least as tall as the viewport
/// (`style`), so an empty-state message centered in it fills the screen and can still be
/// pulled. Put lists and their empty state inside one `RefreshScrollView`; in swipeable tabs
/// use one per `TabPage`.
///
/// ```ignore
/// render! {
///     RefreshScrollView(
///         style: css!(flex_grow: 1.0),
///         on_refresh: move || async move { items.reload().await },
///     ) {
///         /* rows, or an empty-state message */
///     }
/// }
/// ```
#[component]
pub fn refresh_scroll_view(
    /// Called when the user pulls to refresh; the spinner stays until its future finishes.
    /// `None` disables pull to refresh.
    on_refresh: Option<RefreshCallback>,
    /// Style of the scrolling viewport (size it here, for example `flex_grow: 1.0`).
    #[prop(default = Css::new().into())]
    style: Signal<Css>,
    /// Style of the column holding the children (padding, gap, alignment). It grows to the
    /// viewport height by default.
    #[prop(default = Css::new().into())]
    content_style: Signal<Css>,
    /// Shows the platform scroll indicator.
    #[prop(default = Signal::from(true))]
    show_scroll_indicator: Signal<bool>,
    children: Children,
) -> Element {
    let projected = children.clone();
    let style = computed(move || style.get());
    let content_style = computed(move || {
        column()
            .flex_grow(1.0)
            .flex_shrink(0.0)
            .merge(content_style.get())
    });
    let Some(callback) = on_refresh.clone() else {
        return render! {
            ScrollView(style: style, show_scroll_indicator: show_scroll_indicator) {
                View(style: content_style) {
                    {projected()}
                }
            }
        };
    };
    let refreshing = RwSignal::new(false);
    let pull = move || {
        if !begin_refresh(refreshing) {
            return;
        }
        let future = callback.run();
        spawn_local(async move {
            let trace = masamune_whisker_logger::start_trace(REFRESH_TRACE_NAME);
            future.await;
            drop(trace);
            let _ = refreshing.try_set(false);
        });
    };
    render! {
        ScrollView(
            style: style,
            show_scroll_indicator: show_scroll_indicator,
            refreshing: refreshing,
            on_refresh: pull,
        ) {
            View(style: content_style) {
                {projected()}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::task::{Context, Poll, Waker};
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
    fn callback_runs_the_async_closure() {
        let calls = Rc::new(Cell::new(0));
        let counted = Rc::clone(&calls);
        let callback = RefreshCallback::from(move || {
            let counted = Rc::clone(&counted);
            async move { counted.set(counted.get() + 1) }
        });
        let copy = callback.clone();
        let mut future = copy.run();
        // The body runs only when the future is polled.
        assert_eq!(calls.get(), 0);
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(future.as_mut().poll(&mut cx), Poll::Ready(()));
        assert_eq!(calls.get(), 1);
        let _ = callback.run().as_mut().poll(&mut cx);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn a_running_refresh_ignores_new_pulls() {
        with_runtime(|| {
            let refreshing = RwSignal::new(false);
            assert!(begin_refresh(refreshing));
            assert!(refreshing.get_untracked());
            assert!(!begin_refresh(refreshing));
            refreshing.set(false);
            assert!(begin_refresh(refreshing));
        });
    }
}
