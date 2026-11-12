//! Opts dev and test builds into the AI debugger and forwards its settings.
//!
//! `ai_debugger_config!` reads `MASAMUNE_AI_DEBUGGER_*` only with the `masamune_ai_debugger`
//! cfg, so prod binaries never contain the API key or the accounts. The values come from the
//! build environment (export `env/<flavor>.env` before building); nothing is read from files.

const AI_DEBUGGER_KEYS: [&str; 4] = [
    "MASAMUNE_AI_DEBUGGER_PROJECT_ID",
    "MASAMUNE_AI_DEBUGGER_ENDPOINT",
    "MASAMUNE_AI_DEBUGGER_API_KEY",
    "MASAMUNE_AI_DEBUGGER_ACCOUNTS",
];

fn main() {
    println!("cargo::rustc-check-cfg=cfg(masamune_ai_debugger)");
    println!("cargo::rerun-if-env-changed=KIWAME_SAMPLE_FLAVOR");
    for key in AI_DEBUGGER_KEYS {
        println!("cargo::rerun-if-env-changed={key}");
    }
    // Unset means prod (`Flavor::from_env(None)`), so only an explicit dev or test opts in.
    if !matches!(
        std::env::var("KIWAME_SAMPLE_FLAVOR").as_deref(),
        Ok("dev" | "test")
    ) {
        return;
    }
    println!("cargo::rustc-cfg=masamune_ai_debugger");
    for key in AI_DEBUGGER_KEYS {
        // Unexpanded `${VAR}` references mean the variable was not set.
        if let Ok(value) = std::env::var(key)
            && !value.trim().is_empty()
            && !value.starts_with("${")
        {
            println!("cargo::rustc-env={key}={value}");
        }
    }
}
