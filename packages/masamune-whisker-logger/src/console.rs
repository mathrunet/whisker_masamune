use crate::{LogEvent, LoggerAdapter};
use log::{Level, LevelFilter, Record};
use std::error::Error;

/// Writes to stderr (and logcat on Android) or the browser console.
///
/// Events are shown only while the global level includes Debug, i.e. in dev.
#[derive(Clone, Copy, Debug, Default)]
pub struct ConsoleLoggerAdapter;

impl ConsoleLoggerAdapter {
    pub const fn new() -> Self {
        Self
    }
}

impl LoggerAdapter for ConsoleLoggerAdapter {
    fn record(&self, record: &Record<'_>) {
        output(
            record.level(),
            &format!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            ),
        );
    }

    fn send(&self, event: &LogEvent) {
        if log::max_level() >= LevelFilter::Debug {
            output(Level::Debug, &format!("[EVENT] {event}"));
        }
    }

    fn error(&self, error: &(dyn Error + 'static), context: Option<&str>) {
        if log::max_level() >= LevelFilter::Error {
            let line = match context {
                Some(context) => format!("[ERROR] {context}: {error}"),
                None => format!("[ERROR] {error}"),
            };
            output(Level::Error, &line);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn output(level: Level, line: &str) {
    let value = line.into();
    match level {
        Level::Error => web_sys::console::error_1(&value),
        Level::Warn => web_sys::console::warn_1(&value),
        Level::Info => web_sys::console::info_1(&value),
        Level::Debug | Level::Trace => web_sys::console::debug_1(&value),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn output(_level: Level, line: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr().lock(), "{line}");
    #[cfg(target_os = "android")]
    android_output(_level, line);
}

#[cfg(target_os = "android")]
fn android_output(level: Level, line: &str) {
    use std::ffi::{CString, c_char, c_int};
    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    let priority = match level {
        Level::Error => 6,
        Level::Warn => 5,
        Level::Info => 4,
        Level::Debug => 3,
        Level::Trace => 2,
    };
    let line = CString::new(line.replace('\0', "\\0")).expect("NUL bytes replaced");
    // SAFETY: Both pointers refer to valid NUL-terminated strings for the duration of the call.
    unsafe {
        __android_log_write(priority, c"masamune-app".as_ptr(), line.as_ptr());
    }
}
