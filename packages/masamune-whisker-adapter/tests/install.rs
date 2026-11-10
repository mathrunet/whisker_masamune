use std::cell::RefCell;
use std::rc::Rc;

use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, MasamuneAdapters, StartFuture};
use whisker::Owner;
use whisker::runtime::{RuntimeContext, RuntimeWakeHandle};

type Log = Rc<RefCell<Vec<String>>>;

fn context() -> RuntimeContext {
    RuntimeContext::new(RuntimeWakeHandle::new(|| {}))
}

fn entries(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

struct First {
    log: Log,
    value: u32,
}

impl MasamuneAdapter for First {
    fn prepare(&self) -> Result<(), AdapterError> {
        self.log
            .borrow_mut()
            .push(format!("first.prepare {}", self.value));
        Ok(())
    }

    fn start(&self) -> Option<StartFuture> {
        let log = self.log.clone();
        Some(Box::pin(async move {
            log.borrow_mut().push("first.start".into());
            Ok(())
        }))
    }
}

struct Second {
    log: Log,
}

impl MasamuneAdapter for Second {
    fn prepare(&self) -> Result<(), AdapterError> {
        self.log.borrow_mut().push("second.prepare".into());
        Err("second failed".into())
    }

    fn start(&self) -> Option<StartFuture> {
        let log = self.log.clone();
        Some(Box::pin(async move {
            log.borrow_mut().push("second.start".into());
            Err("second start failed".into())
        }))
    }
}

/// Start work that never finishes; records its drop (cancellation).
struct Pending {
    log: Log,
}

struct Dropped(Log);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.borrow_mut().push("pending.dropped".into());
    }
}

impl MasamuneAdapter for Pending {
    fn start(&self) -> Option<StartFuture> {
        let dropped = Dropped(self.log.clone());
        Some(Box::pin(async move {
            let _dropped = dropped;
            std::future::pending::<()>().await;
            Ok(())
        }))
    }
}

#[test]
fn prepares_in_order_then_starts_and_keeps_going_after_errors() {
    let log = Log::default();
    let runtime = context();
    runtime.enter(|| {
        let root = Owner::new(None);
        root.with(|| {
            MasamuneAdapters::new()
                .adapter(First {
                    log: log.clone(),
                    value: 1,
                })
                .adapter(Second { log: log.clone() })
                .install();
        });
        assert_eq!(entries(&log), ["first.prepare 1", "second.prepare"]);
        whisker::run_until_stalled();
        assert_eq!(
            entries(&log),
            [
                "first.prepare 1",
                "second.prepare",
                "first.start",
                "second.start"
            ]
        );
        root.dispose();
    });
    runtime.shutdown();
}

#[test]
fn primary_returns_the_installed_adapter() {
    let log = Log::default();
    let runtime = context();
    runtime.enter(|| {
        assert!(First::primary().is_none());
        let root = Owner::new(None);
        root.with(|| {
            MasamuneAdapters::new()
                .adapter(First {
                    log: log.clone(),
                    value: 7,
                })
                .install();
        });
        assert_eq!(First::primary().map(|adapter| adapter.value), Some(7));
        assert!(Second::primary().is_none());
        root.dispose();
    });
    runtime.shutdown();
}

#[test]
fn start_outlives_other_owners_but_not_the_installing_owner() {
    let log = Log::default();
    let runtime = context();
    runtime.enter(|| {
        let root = Owner::new(None);
        let screen = root.with(|| {
            MasamuneAdapters::new()
                .adapter(Pending { log: log.clone() })
                .install();
            Owner::new(None)
        });
        whisker::run_until_stalled();
        screen.dispose();
        whisker::run_until_stalled();
        assert!(entries(&log).is_empty());
        root.dispose();
        assert_eq!(entries(&log), ["pending.dropped"]);
    });
    runtime.shutdown();
}

#[test]
fn installing_again_replaces_adapters_and_cancels_previous_start() {
    let log = Log::default();
    let runtime = context();
    runtime.enter(|| {
        let root = Owner::new(None);
        root.with(|| {
            MasamuneAdapters::new()
                .adapter(Pending { log: log.clone() })
                .adapter(First {
                    log: log.clone(),
                    value: 1,
                })
                .install();
        });
        whisker::run_until_stalled();
        log.borrow_mut().clear();
        root.with(|| {
            MasamuneAdapters::new()
                .adapter(First {
                    log: log.clone(),
                    value: 2,
                })
                .install();
        });
        assert_eq!(entries(&log), ["pending.dropped", "first.prepare 2"]);
        assert_eq!(First::primary().map(|adapter| adapter.value), Some(2));
        assert!(Pending::primary().is_none());
        root.dispose();
    });
    runtime.shutdown();
}

#[test]
fn runtimes_are_isolated() {
    let log = Log::default();
    let installed = context();
    let other = context();
    installed.enter(|| {
        MasamuneAdapters::new()
            .adapter(First {
                log: log.clone(),
                value: 1,
            })
            .install();
    });
    other.enter(|| assert!(First::primary().is_none()));
    installed.enter(|| assert!(First::primary().is_some()));
    installed.shutdown();
    other.shutdown();
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "added twice")]
fn duplicate_types_panic_in_debug_builds() {
    let log = Log::default();
    let _ = MasamuneAdapters::new()
        .adapter(First {
            log: log.clone(),
            value: 1,
        })
        .adapter(First { log, value: 2 });
}
