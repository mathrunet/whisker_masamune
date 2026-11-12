# masamune-whisker-ai-debugger

AI debugger for dev and test builds of Whisker apps, ported from Flutter's `masamune_ai_debugger`. A floating
button opens a panel that sends instructions and screenshots to SamuraiAI, and unexpected errors,
panics and slow operations become AI sessions automatically. It speaks the same
`/api/app-debug/*` API and exposes the same accessibility identifiers and labels as the Flutter
package, so the existing SamuraiAI backend and Maestro flows work unchanged.

Only debug builds with `Flavor::Dev` or `Flavor::Test` enable it (test builds keep it so automated
E2E runs can report to SamuraiAI). In release builds or prod the overlay renders only its children, the logger adapter
and panic hook drop everything, and nothing is sent.

## Setup

Compile-time values, read in the app crate by `ai_debugger_config!`:

| Variable | Value |
|---|---|
| `MASAMUNE_AI_DEBUGGER_PROJECT_ID` | SamuraiAI project id (the app path with `/` replaced by `-`) |
| `MASAMUNE_AI_DEBUGGER_ENDPOINT` | SamuraiAI base URL (`/api/app-debug/…` is appended) |
| `MASAMUNE_AI_DEBUGGER_API_KEY` | Sent as `x-api-key` |
| `MASAMUNE_AI_DEBUGGER_ACCOUNTS` | Optional `id1:pass1,id2:pass2` debug accounts |

The values are read only when the app crate opts in with the `masamune_ai_debugger` cfg in a
debug build. Emit it from the app's `build.rs` for every flavor except prod (dev and test), so production binaries never
contain the API key or the accounts, even when the variables are set in the build environment:

```rust,ignore
// build.rs
println!("cargo::rustc-check-cfg=cfg(masamune_ai_debugger)");
println!("cargo::rerun-if-env-changed=MY_APP_FLAVOR");
// Unset means prod, so only an explicit dev or test flavor opts in.
if matches!(std::env::var("MY_APP_FLAVOR").as_deref(), Ok("dev" | "test")) {
    println!("cargo::rustc-cfg=masamune_ai_debugger");
}
```

The API key is compiled into dev and test builds: never distribute them. Set `<APP>_FLAVOR=prod` for
production builds. Development requires an explicit `<APP>_FLAVOR=dev` (or `test` for automated
test builds).

```rust,ignore
use masamune_whisker_ai_debugger::{AiDebugController, AiDebugger, ai_debugger_config};
use masamune_whisker_logger::{ConsoleLoggerAdapter, Flavor, LoggerConfig};
use whisker::prelude::*;

thread_local! {
    static DEBUGGER: AiDebugController = {
        let flavor = Flavor::from_env(option_env!("MY_APP_FLAVOR")).expect("flavor");
        AiDebugController::new(ai_debugger_config!(flavor)).expect("ai debugger")
    };
}

fn prepare() {
    let debugger = DEBUGGER.with(Clone::clone);
    let flavor = Flavor::from_env(option_env!("MY_APP_FLAVOR")).expect("flavor");
    LoggerConfig::new(flavor)
        .adapter(ConsoleLoggerAdapter::new())
        .adapter(debugger.logger_adapter())
        .init()
        .expect("logger");
    // After other hooks (such as Crashlytics' `record_panics`), so every hook runs.
    debugger.install_panic_hook();
}

#[whisker::main]
fn app() -> Element {
    prepare();
    let debugger = DEBUGGER.with(Clone::clone);
    render! {
        AiDebugger(controller: debugger) {
            Root {}
        }
    }
}
```

Put `AiDebugger` at the root, outside `AppRoot` and the router, so it survives rebuilds and
captures the whole app. Screenshots use `masamune-whisker-view-capture` (Android and iOS).

## What it does

- **Panel**: tap the button to open it, long-press to take a screenshot and open it, drag to move
  it (the panel moves by its title row). Type an instruction, attach up to six screenshots, and
  pick the request type (`不具合修正` / `要件修正`, which prefixes `/dev:kiwame:edit`), the mode
  (`Plan` / `bypassPermissions`) and the model (`Astra` / `Sol` / `Terra` / `Luna`) with the
  bottom-row icon buttons (each selector shows its value as a tinted Lucide icon; the accessibility
  labels keep the text names, e.g. `Mode Plan`, `Model Sol`). The settings
  dialog chooses the provider, the model and mode for error and performance incidents, and the
  slow-operation thresholds. Settings are saved per project with `whisker-local-store`.
- **Unexpected errors**: panics (`install_panic_hook`) and `report_error` calls (through the
  logger adapter) become `exception` incidents with a screenshot. Set
  `report_handled_errors(false)` to send handled errors as `error` breadcrumbs only.
- **Slow operations**: `masamune-whisker-model` traces `CachedStore::load` / `query` and
  `masamune-whisker-ui` traces visible progress indicators. Model loads over 5 seconds and
  indicators over 10 seconds become `performance` incidents (thresholds are adjustable).
- **Breadcrumbs**: `send_event`, log records (Info and above) and finished traces are batched to
  `/events` before requests and incidents.
- **Run lifecycle**: the run registers once, sends a heartbeat every 30 seconds, and is recreated
  when the server answers 404/410.

Identical incidents (digits ignored) are sent once per launch, and incidents less than one second
apart are dropped. Instructions, messages, stack traces and context values are redacted (bearer
tokens, `token=` / `password=`-style assignments and email addresses) and truncated before sending.

## Context

Whisker has no widget tree to walk; supply the page, route and selected state yourself:

```rust,ignore
let config = ai_debugger_config!(flavor).context_provider(|| async {
    Some(AiDebugContextSnapshot {
        page_name: Some("Home".into()),
        route: Some(current_path()),
        ..Default::default()
    })
});
```

## Debug login, purchase and camera

Each panel appears when its callbacks are given. Callbacks return `Result<_, String>`; the error
text is shown in the panel.

```rust,ignore
use whisker_firebase::auth::Auth;

let config = ai_debugger_config!(flavor)
    .auth(
        AiDebugAuth::new(
            |email, password| async move {
                let auth = Auth::instance().map_err(|e| e.to_string())?;
                auth.sign_in_with_email_and_password(&email, &password)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            },
            || async { Auth::instance().and_then(|a| a.sign_out()).map_err(|e| e.to_string()) },
            // Read inside a reactive computation: reading a signal keeps the panel current.
            || Ok(Auth::instance().map_err(|e| e.to_string())?.user_signal().get().is_some()),
        )
        .anonymous_login(|| async {
            let auth = Auth::instance().map_err(|e| e.to_string())?;
            auth.sign_in_anonymously().await.map(|_| ()).map_err(|e| e.to_string())
        }),
    )
    .auth_snapshot(|| Ok(app_auth_snapshot()))
    // Features `billing` / `image-picker`:
    .purchase(AiDebugPurchase::billing(vec![
        AiDebugPurchaseProduct::new("premium_monthly", "Premium (monthly)"),
    ]))
    .camera(AiDebugCamera::image_picker(vec![(
        AiDebugCameraPicture::new("receipt", "Receipt"),
        PickedImage { path: receipt_path(), width: 1080, height: 1440 },
    )]));
```

- `AiDebugPurchase::billing` (feature `billing`) uses `masamune_whisker_billing::debug_force_purchase`:
  the forced purchase reaches `on_purchases` and `query_purchases` without the store, with the
  token `DEBUG_PURCHASE_TOKEN`.
- `AiDebugCamera::image_picker` (feature `image-picker`) uses
  `masamune_whisker_image_picker::set_debug_picture`: the camera returns the prepared image.
- With `MASAMUNE_AI_DEBUGGER_ACCOUNTS`, the login panel lists the accounts (alias or id, never the
  password) instead of the email/password form.
- `auth_snapshot` exposes the real auth state to Maestro. Never derive `identity` from the debug
  account; use a stable non-secret token of the signed-in user.

## Maestro

| Identifier / label | Element |
|---|---|
| `ai_debugger_open` / `AI Debuggerを開く` | Floating button |
| `ai_debugger_close` / `閉じる` | Close button of the panel |
| `ai_debugger_auth_login` / `ai_debugger_auth_logout` (label `AIデバッガー認証`) | Login / logout button |
| `ai_debugger_auth_account_<alias or account_N>` | Account row (left out for invalid or duplicate aliases) |
| `ai_debugger_auth_{schema_version,state,identity,restore_state,boot_state,boot_id,login_count}` | 1×1 observation nodes whose label is the value, present while collapsed |
| `AI Debugger auth unknown\|authenticated\|unauthenticated` | Panel label when debug login is configured |
| `AIデバッガー指示入力`, `スクリーンショット`, `設定`, `送信`, `Mode <mode>`, `Model <model>` | Prompt panel |
| `デバッグログイン メールアドレス` / `パスワード` / `実行`, `デバッグ匿名ログイン実行`, `デバッグアカウントログイン <alias or id>` | Login panel |
| `デバッグ課金 商品選択`, `デバッグ強制課金実行`, `課金解除 <id>` | Purchase panel |
| `デバッグカメラ 画像選択`, `デバッグカメラ画像設定実行`, `カメラ画像解除 <id>` | Camera panel |
| `AI Debugger provider <name or automatic>`, `設定を閉じる`, `プレビューを閉じる`, `AI入力へ戻る` | Settings, preview and back buttons |

## Testing

`AiDebuggerConfig::transport` replaces the HTTP client and `settings_store` the saved settings
(`MemorySettingsStore`). `AiDebugController::attach` takes the task spawner; tests can pass
`tokio::task::spawn_local` inside a `LocalSet`.

## Differences from the Flutter package

- Assignment redaction keeps the key (`password=[REDACTED]`) instead of a literal `$1`.
- No automatic widget tree; use `context_provider`.
- No app lifecycle events in Whisker: the run stays alive with heartbeats and the server expires
  it after the app stops.
- Purchase and camera pictures are chosen from chips instead of drop-downs.

## Security boundary

The debugger runs only in debug builds with an explicit `dev` or `test` flavor. Unset flavor is
production. Release builds disable the controller, overlay, settings I/O and reporting
even when passed `Flavor::Dev` or `Flavor::Test`. The configuration macro does not read compile-time API
keys or test accounts in release builds. Do not distribute development builds or pass
credentials through separate literals in application code.
