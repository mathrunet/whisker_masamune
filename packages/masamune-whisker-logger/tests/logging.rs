use masamune_whisker_logger::{
    Flavor, LevelFilter, debug, error, info, init, init_with_level, report_error, send_event,
    trace, warn,
};
use std::process::Command;

#[test]
fn flavor_values() {
    assert_eq!(Flavor::from_env(None), Ok(Flavor::Dev));
    assert_eq!(Flavor::from_env(Some("dev")), Ok(Flavor::Dev));
    assert_eq!(Flavor::from_env(Some("prod")), Ok(Flavor::Prod));
    assert!(Flavor::from_env(Some("production")).is_err());
    assert!(Flavor::from_env(Some("")).is_err());
}

#[test]
fn process_logging() {
    if let Ok(mode) = std::env::var("MASAMUNE_LOGGER_TEST_MODE") {
        if mode == "conflict" {
            struct Other;
            impl log::Log for Other {
                fn enabled(&self, _: &log::Metadata<'_>) -> bool {
                    true
                }
                fn log(&self, _: &log::Record<'_>) {}
                fn flush(&self) {}
            }
            static OTHER: Other = Other;
            log::set_logger(&OTHER).unwrap();
            log::set_max_level(LevelFilter::Error);
            assert!(init(Flavor::Dev).is_err());
            assert!(init(Flavor::Prod).is_err());
            assert_eq!(log::max_level(), LevelFilter::Error);
            return;
        }
        let flavor = if mode == "dev" {
            Flavor::Dev
        } else {
            Flavor::Prod
        };
        init(flavor).unwrap();
        init(flavor).unwrap();
        if mode == "off" {
            init_with_level(LevelFilter::Off).unwrap();
        }
        trace!("TRACE_MARKER");
        debug!("DEBUG_MARKER");
        info!("INFO_MARKER");
        warn!("WARN_MARKER");
        error!("ERROR_MARKER");
        send_event("EVENT_MARKER", [("key", 1)]);
        report_error(&std::fmt::Error, Some("REPORT_MARKER"));
        if mode != "dev" {
            debug!("{}", panic_value());
            info!("{}", panic_value());
        }
        if mode == "off" {
            error!("{}", panic_value());
        }
        return;
    }
    for mode in ["dev", "prod", "off", "conflict"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_logging", "--nocapture"])
            .env("MASAMUNE_LOGGER_TEST_MODE", mode)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stderr).unwrap();
        for marker in ["DEBUG", "INFO", "WARN", "ERROR", "TRACE", "EVENT", "REPORT"] {
            let expected = match mode {
                "dev" => marker != "TRACE",
                "prod" => matches!(marker, "WARN" | "ERROR" | "REPORT"),
                _ => false,
            };
            assert_eq!(
                text.contains(&format!("{marker}_MARKER")),
                expected,
                "{mode}: {text}"
            );
        }
    }
}

fn panic_value() -> &'static str {
    panic!("disabled log argument evaluated")
}
