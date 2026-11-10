#![doc = include_str!("../README.md")]

use std::any::{Any, TypeId, type_name};
use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use whisker::{Owner, spawn_local};

/// Failure of [`MasamuneAdapter::prepare`] or [`MasamuneAdapter::start`].
pub type AdapterError = Box<dyn std::error::Error>;

/// Work returned by [`MasamuneAdapter::start`].
pub type StartFuture = Pin<Box<dyn Future<Output = Result<(), AdapterError>>>>;

/// Settings and startup work of one plugin (Masamune's `MasamuneAdapter`).
///
/// `app()` runs again after some hot reloads and when Android recreates the activity, so both
/// hooks must be safe to repeat.
pub trait MasamuneAdapter: 'static {
    /// Synchronous setup, called by [`MasamuneAdapters::install`] in registration order.
    /// Errors are reported and the remaining adapters still run.
    fn prepare(&self) -> Result<(), AdapterError> {
        Ok(())
    }

    /// Work spawned once every adapter is prepared. It belongs to the owner that called
    /// [`MasamuneAdapters::install`], so it outlives screens and the boot screen.
    ///
    /// The part before the first `.await` runs before the first frame: keep it light.
    fn start(&self) -> Option<StartFuture> {
        None
    }

    /// The adapter of this type installed in the current runtime.
    fn primary() -> Option<Rc<Self>>
    where
        Self: Sized,
    {
        lookup::<Self>()
    }
}

struct Entry {
    type_id: TypeId,
    name: &'static str,
    adapter: Rc<dyn MasamuneAdapter>,
    any: Rc<dyn Any>,
}

struct Installed {
    scope: Owner,
    entries: Rc<[Entry]>,
}

whisker::runtime_local! {
    static INSTALLED: RefCell<Option<Installed>> = RefCell::new(None);
}

/// The adapters of an app (Masamune's `masamuneAdapters`).
///
/// ```no_run
/// # use masamune_whisker_adapter::{MasamuneAdapter, MasamuneAdapters};
/// # struct ExampleMasamuneAdapter;
/// # impl MasamuneAdapter for ExampleMasamuneAdapter {}
/// MasamuneAdapters::new()
///     .adapter(ExampleMasamuneAdapter)
///     .install();
/// ```
#[derive(Default)]
pub struct MasamuneAdapters {
    entries: Vec<Entry>,
}

impl MasamuneAdapters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an adapter. A second adapter of the same type panics in debug builds; release builds
    /// log an error and keep the first.
    pub fn adapter<A: MasamuneAdapter>(mut self, adapter: A) -> Self {
        let type_id = TypeId::of::<A>();
        let name = type_name::<A>();
        if self.entries.iter().any(|entry| entry.type_id == type_id) {
            if cfg!(debug_assertions) {
                panic!("{name} is added twice");
            }
            masamune_whisker_logger::error!("masamune adapter added twice: {name}");
            return self;
        }
        let adapter = Rc::new(adapter);
        self.entries.push(Entry {
            type_id,
            name,
            any: adapter.clone(),
            adapter,
        });
        self
    }

    /// Replaces this runtime's adapters, prepares them, then starts them.
    ///
    /// Call it in `#[whisker::main] fn app()` on the UI thread, after the logger is initialized.
    /// Installing again cancels the previous adapters' unfinished start work.
    pub fn install(self) {
        let entries: Rc<[Entry]> = self.entries.into();
        let previous = INSTALLED.with(|installed| installed.borrow_mut().take());
        if let Some(previous) = previous {
            previous.scope.dispose();
        }
        let scope = Owner::new(None);
        INSTALLED.with(|installed| {
            *installed.borrow_mut() = Some(Installed {
                scope,
                entries: entries.clone(),
            });
        });
        scope.with(|| {
            for entry in entries.iter() {
                if let Err(error) = entry.adapter.prepare() {
                    report(entry.name, "prepare", &*error);
                }
            }
            for entry in entries.iter() {
                let Some(work) = entry.adapter.start() else {
                    continue;
                };
                let name = entry.name;
                spawn_local(async move {
                    if let Err(error) = work.await {
                        report(name, "start", &*error);
                    }
                });
            }
        });
    }
}

fn lookup<T: MasamuneAdapter>() -> Option<Rc<T>> {
    let any = INSTALLED.with(|installed| {
        installed
            .borrow()
            .as_ref()?
            .entries
            .iter()
            .find(|entry| entry.type_id == TypeId::of::<T>())
            .map(|entry| entry.any.clone())
    })?;
    any.downcast::<T>().ok()
}

fn report(name: &str, hook: &str, error: &(dyn std::error::Error + 'static)) {
    let context = format!("masamune.adapter.{hook}|{name}");
    masamune_whisker_logger::report_error(error, Some(&context));
}
