//! Run, request, incident and breadcrumb lifecycle (`lib/src/ai_debug_controller.dart` and the
//! default SamuraiAI callbacks of `lib/adapter/ai_debugger_masamune_adapter.dart`).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{BuildHasher, RandomState};
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, SecondsFormat, Utc};
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use futures_util::future::{LocalBoxFuture, Shared};
use futures_util::{FutureExt, StreamExt};
use regex::Regex;
use serde_json::{Map, Value, json};
use tokio::time::Instant;

use crate::config::{AiDebugConfigError, AiDebugSpawner, AiDebuggerConfig};
use crate::redact::{redact, truncate};
use crate::settings::{AiDebugSettings, settings_key};
use crate::transport::AiDebugError;
use crate::types::{AiDebugContextSnapshot, AiDebugModel, AiDebugPermissionMode};
use crate::voice::protocol::AiDebugSpeechToken;

/// Log target of the debugger's own diagnostics; the logger adapter ignores it so failures do
/// not feed back into breadcrumbs.
pub(crate) const LOG_TARGET: &str = "masamune_whisker_ai_debugger";

/// Trace prefix of model loads (`masamune-whisker-model`).
pub const MODEL_LOAD_TRACE_PREFIX: &str = "masamune.model.load|";
/// Trace prefix of visible progress indicators (`masamune-whisker-ui`).
pub const INDICATOR_TRACE_PREFIX: &str = "masamune.indicator.show|";

const INCIDENT_BURST_WINDOW: Duration = Duration::from_secs(1);
const MAX_LOGS: usize = 200;
const LOG_FLUSH_COUNT: usize = 20;
const LOG_BATCH: usize = 100;
const LOG_FLUSH_DELAY: Duration = Duration::from_secs(2);

static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").expect("digits pattern"));

type SharedResult = Shared<LocalBoxFuture<'static, Result<(), AiDebugError>>>;
type CaptureFn = Rc<dyn Fn() -> LocalBoxFuture<'static, Option<Vec<u8>>>>;
type IncidentListener = Rc<dyn Fn(&str, &str)>;

/// Something reported from another thread (logger adapter, panic hook), handled on the UI thread.
#[derive(Debug)]
pub(crate) enum Message {
    Log {
        name: String,
        params: Value,
        severity: &'static str,
    },
    HandledError {
        message: String,
        stack: String,
    },
    Panic {
        message: String,
        stack: String,
    },
    TraceStart {
        id: u64,
        name: String,
        wall: DateTime<Utc>,
        at: Instant,
    },
    TraceStop {
        id: u64,
        name: String,
        at: Instant,
    },
}

struct Trace {
    at: Instant,
    category: Option<&'static str>,
    threshold: Option<Duration>,
    exceeded: bool,
}

struct State {
    run_id: String,
    started_at: DateTime<Utc>,
    settings: AiDebugSettings,
    settings_loaded: bool,
    registration: Option<SharedResult>,
    heartbeat: Option<SharedResult>,
    end: Option<Shared<LocalBoxFuture<'static, ()>>>,
    foreground: bool,
    heartbeat_generation: u64,
    flush_generation: u64,
    flush_scheduled: bool,
    fingerprints: HashSet<String>,
    last_incident: Option<Instant>,
    logs: VecDeque<Value>,
    traces: HashMap<u64, Trace>,
}

struct Inner {
    config: AiDebuggerConfig,
    state: RefCell<State>,
    spawner: RefCell<Option<AiDebugSpawner>>,
    capture: RefCell<Option<CaptureFn>>,
    incident_listener: RefCell<Option<IncidentListener>>,
    sender: UnboundedSender<Message>,
    receiver: RefCell<Option<UnboundedReceiver<Message>>>,
}

/// Talks to SamuraiAI: registers the run, keeps it alive, sends instructions and screenshots,
/// reports incidents and uploads breadcrumbs. Clones share one run.
///
/// Reporting and settings I/O run only in debug builds with an explicit dev or test flavor.
#[derive(Clone)]
pub struct AiDebugController(Rc<Inner>);

impl AiDebugController {
    /// Fails when the initial provider is not one of the configured providers.
    pub fn new(mut config: AiDebuggerConfig) -> Result<Self, AiDebugConfigError> {
        let mut providers: Vec<String> = Vec::new();
        for provider in config.providers.iter().map(|p| p.trim()) {
            if !provider.is_empty() && !providers.iter().any(|p| p == provider) {
                providers.push(provider.to_owned());
            }
        }
        config.providers = providers;
        if let Some(provider) = &config.settings.provider
            && !config.providers.contains(provider)
        {
            return Err(AiDebugConfigError(format!(
                "the initial provider must be included in providers: {provider}"
            )));
        }
        let (sender, receiver) = unbounded();
        let settings = config.settings.clone();
        Ok(Self(Rc::new(Inner {
            config,
            state: RefCell::new(State {
                run_id: create_run_id(),
                started_at: Utc::now(),
                settings,
                settings_loaded: false,
                registration: None,
                heartbeat: None,
                end: None,
                foreground: false,
                heartbeat_generation: 0,
                flush_generation: 0,
                flush_scheduled: false,
                fingerprints: HashSet::new(),
                last_incident: None,
                logs: VecDeque::new(),
                traces: HashMap::new(),
            }),
            spawner: RefCell::new(None),
            capture: RefCell::new(None),
            incident_listener: RefCell::new(None),
            sender,
            receiver: RefCell::new(Some(receiver)),
        })))
    }

    /// Whether the debugger runs (debug builds with an explicit dev or test flavor).
    pub fn enabled(&self) -> bool {
        self.0.config.enabled()
    }

    pub fn config(&self) -> &AiDebuggerConfig {
        &self.0.config
    }

    pub fn project_id(&self) -> &str {
        &self.0.config.project_id
    }

    /// Whether the overlay shows the voice input button: the crate was built with the `voice`
    /// feature, the debugger is enabled (debug build, dev or test flavor) and
    /// [`AiDebuggerConfig::voice_input`] was not turned off.
    pub fn voice_input_available(&self) -> bool {
        cfg!(feature = "voice") && self.enabled() && self.0.config.voice_input
    }

    /// Provider choices after trimming and removing duplicates.
    pub fn providers(&self) -> &[String] {
        &self.0.config.providers
    }

    /// Id of the current run (`app-<micros>-<hex>`); replaced when the server forgot the run.
    pub fn run_id(&self) -> String {
        self.0.state.borrow().run_id.clone()
    }

    pub fn started_at(&self) -> DateTime<Utc> {
        self.0.state.borrow().started_at
    }

    /// Starts handling reports from the logger adapter and the panic hook, and lets the
    /// controller run timers. The overlay calls it on mount; call it yourself without the overlay.
    pub fn attach(&self, spawner: AiDebugSpawner) {
        if !self.enabled() {
            return;
        }
        *self.0.spawner.borrow_mut() = Some(spawner);
        let Some(receiver) = self.0.receiver.borrow_mut().take() else {
            return;
        };
        // Created outside the task so a task that is cancelled (or never runs) still gives the
        // queue back for the next attach, such as a remounted overlay.
        let mut queue = Queue {
            controller: self.clone(),
            receiver: Some(receiver),
        };
        self.spawn(async move {
            while let Some(message) = queue.next().await {
                queue.controller.handle(message);
            }
        });
    }

    /// Lets incidents attach a screenshot (PNG bytes).
    pub fn set_capture<F, Fut>(&self, capture: F)
    where
        F: Fn() -> Fut + 'static,
        Fut: std::future::Future<Output = Option<Vec<u8>>> + 'static,
    {
        *self.0.capture.borrow_mut() = Some(Rc::new(move || capture().boxed_local()));
    }

    /// Called with `(kind, session_id)` when an incident created an AI session.
    pub fn set_incident_listener(&self, listener: impl Fn(&str, &str) + 'static) {
        *self.0.incident_listener.borrow_mut() = Some(Rc::new(listener));
    }

    pub(crate) fn sender(&self) -> Option<UnboundedSender<Message>> {
        self.enabled().then(|| self.0.sender.clone())
    }

    pub(crate) fn spawn(&self, future: impl std::future::Future<Output = ()> + 'static) {
        let spawner = self.0.spawner.borrow().clone();
        if let Some(spawner) = spawner {
            spawner(future.boxed_local());
        }
    }

    // ---- Settings -----------------------------------------------------------------------------

    /// Current settings, loading the saved ones on first use.
    pub fn settings(&self) -> AiDebugSettings {
        if !self.enabled() {
            return self.0.config.settings.clone();
        }
        if !self.0.state.borrow().settings_loaded {
            let loaded = self.load_saved_settings();
            let mut state = self.0.state.borrow_mut();
            state.settings = loaded;
            state.settings_loaded = true;
        }
        self.0.state.borrow().settings.clone()
    }

    fn load_saved_settings(&self) -> AiDebugSettings {
        let config = &self.0.config;
        let fallback = &config.settings;
        let source = match config
            .settings_store
            .load(&settings_key(&config.project_id))
        {
            Ok(Some(source)) if !source.is_empty() => source,
            Ok(_) => return fallback.clone(),
            Err(error) => {
                log::debug!(target: LOG_TARGET, "AI Debugger settings load failed: {error}");
                return fallback.clone();
            }
        };
        match serde_json::from_str::<Value>(&source) {
            Ok(json @ Value::Object(_)) => {
                self.normalize(AiDebugSettings::from_json(&json, fallback))
            }
            _ => fallback.clone(),
        }
    }

    fn normalize(&self, mut settings: AiDebugSettings) -> AiDebugSettings {
        if let Some(provider) = &settings.provider
            && !self.providers().contains(provider)
        {
            settings.provider = self.0.config.settings.provider.clone();
        }
        settings
    }

    /// Saves settings for this project.
    pub fn update_settings(&self, settings: AiDebugSettings) -> Result<(), AiDebugError> {
        if !self.enabled() {
            return Ok(());
        }
        self.settings();
        let settings = self.normalize(settings);
        self.0.state.borrow_mut().settings = settings.clone();
        let config = &self.0.config;
        config
            .settings_store
            .save(
                &settings_key(&config.project_id),
                &settings.to_json().to_string(),
            )
            .map_err(AiDebugError::Transport)
    }

    // ---- Run lifecycle ------------------------------------------------------------------------

    /// Registers the run once; a failed registration is retried by the next call.
    pub async fn register(&self) -> Result<(), AiDebugError> {
        if !self.enabled() {
            return Ok(());
        }
        let existing = self.0.state.borrow().registration.clone();
        let registration = match existing {
            Some(registration) => registration,
            None => {
                let this = self.clone();
                let registration = async move { this.post_register().await }
                    .boxed_local()
                    .shared();
                self.0.state.borrow_mut().registration = Some(registration.clone());
                registration
            }
        };
        let result = registration.clone().await;
        if result.is_err() {
            let mut state = self.0.state.borrow_mut();
            if state
                .registration
                .as_ref()
                .is_some_and(|r| r.ptr_eq(&registration))
            {
                state.registration = None;
            }
        }
        result
    }

    async fn post_register(&self) -> Result<(), AiDebugError> {
        let (run_id, started_at) = {
            let state = self.0.state.borrow();
            (state.run_id.clone(), state.started_at)
        };
        self.post(
            "/api/app-debug/runs",
            json!({
                "runId": run_id,
                "projectId": self.0.config.project_id,
                "platform": platform_name(),
                "flavor": "debug",
                "startedAt": iso(started_at),
                "maxSessionsPerHour": self.0.config.max_sessions_per_hour,
            }),
        )
        .await
        .map(|_| ())
    }

    /// Sends a heartbeat now and then every `heartbeat_interval` until [`Self::pause`].
    pub async fn resume(&self) -> Result<(), AiDebugError> {
        if !self.enabled() {
            return Ok(());
        }
        let generation = {
            let mut state = self.0.state.borrow_mut();
            state.foreground = true;
            state.heartbeat_generation += 1;
            state.heartbeat_generation
        };
        let result = self.heartbeat().await;
        self.schedule_heartbeat(generation);
        result
    }

    fn schedule_heartbeat(&self, generation: u64) {
        let this = self.clone();
        let interval = self.0.config.heartbeat_interval;
        self.spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                {
                    let state = this.0.state.borrow();
                    if !state.foreground || state.heartbeat_generation != generation {
                        return;
                    }
                }
                if let Err(error) = this.heartbeat().await {
                    log::debug!(target: LOG_TARGET, "AI Debugger heartbeat failed: {error}");
                }
            }
        });
    }

    /// Stops heartbeats while the app is in the background.
    pub fn pause(&self) {
        let mut state = self.0.state.borrow_mut();
        state.foreground = false;
        state.heartbeat_generation += 1;
    }

    /// Sends one heartbeat; a run the server no longer knows (404/410) is replaced and registered
    /// again.
    pub async fn heartbeat(&self) -> Result<(), AiDebugError> {
        if !self.enabled() {
            return Ok(());
        }
        let existing = self.0.state.borrow().heartbeat.clone();
        let request = match existing {
            Some(request) => request,
            None => {
                let this = self.clone();
                let request = async move { this.send_heartbeat().await }
                    .boxed_local()
                    .shared();
                self.0.state.borrow_mut().heartbeat = Some(request.clone());
                request
            }
        };
        let result = request.clone().await;
        let mut state = self.0.state.borrow_mut();
        if state.heartbeat.as_ref().is_some_and(|r| r.ptr_eq(&request)) {
            state.heartbeat = None;
        }
        result
    }

    async fn send_heartbeat(&self) -> Result<(), AiDebugError> {
        match self.post_heartbeat().await {
            Err(error) if matches!(error.status(), Some(404 | 410)) => {
                self.rotate_run();
                self.post_heartbeat().await
            }
            result => result,
        }
    }

    async fn post_heartbeat(&self) -> Result<(), AiDebugError> {
        self.register().await?;
        let path = format!("/api/app-debug/runs/{}/heartbeat", self.run_id());
        self.post(&path, json!({})).await.map(|_| ())
    }

    fn rotate_run(&self) {
        let mut state = self.0.state.borrow_mut();
        state.run_id = create_run_id();
        state.started_at = Utc::now();
        state.registration = None;
        state.end = None;
    }

    /// Ends the run (best effort). Safe to call more than once.
    pub async fn end(&self) {
        if !self.enabled() {
            return;
        }
        let existing = self.0.state.borrow().end.clone();
        let request = match existing {
            Some(request) => request,
            None => {
                self.pause();
                let this = self.clone();
                let request = async move { this.finish_run().await }
                    .boxed_local()
                    .shared();
                self.0.state.borrow_mut().end = Some(request.clone());
                request
            }
        };
        request.await;
    }

    async fn finish_run(&self) {
        let heartbeat = {
            let mut state = self.0.state.borrow_mut();
            state.flush_generation += 1;
            state.flush_scheduled = false;
            state.heartbeat.clone()
        };
        if let Some(heartbeat) = heartbeat {
            let _ = heartbeat.await;
        }
        if !self.0.state.borrow().logs.is_empty() {
            self.flush_logs().await;
        }
        if self.0.state.borrow().registration.is_none() {
            return;
        }
        let path = format!("/api/app-debug/runs/{}/end", self.run_id());
        let _ = self.post(&path, json!({})).await;
    }

    // ---- Requests -----------------------------------------------------------------------------

    /// Uploads a PNG and returns the stored name (empty when the server sent none).
    pub async fn upload(&self, png: &[u8], name: Option<String>) -> Result<String, AiDebugError> {
        if !self.enabled() {
            return Ok(String::new());
        }
        self.register().await?;
        let name =
            name.unwrap_or_else(|| format!("screenshot-{}.png", Utc::now().timestamp_millis()));
        let data = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        );
        let path = format!("/api/app-debug/runs/{}/screenshots", self.run_id());
        let result = self
            .post(&path, json!({ "name": name, "data": data }))
            .await?;
        Ok(result
            .get("screenshot")
            .and_then(|s| s.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned())
    }

    /// Sends an instruction with up to 10 screenshots (PNG bytes) and returns the session id.
    /// `model` / `permission_mode` default to the manual settings.
    pub async fn send(
        &self,
        instruction: &str,
        screenshots: &[Vec<u8>],
        model: Option<AiDebugModel>,
        permission_mode: Option<AiDebugPermissionMode>,
    ) -> Result<Option<String>, AiDebugError> {
        if !self.enabled() {
            return Ok(None);
        }
        let settings = self.settings();
        let context = self.capture_context().await;
        self.register().await?;
        self.flush_logs().await;
        let mut names = Vec::new();
        for image in screenshots.iter().take(10) {
            let name = self.upload(image, None).await?;
            if !name.is_empty() {
                names.push(name);
            }
        }
        let mut body = Map::new();
        body.insert(
            "instruction".into(),
            truncate(&redact(instruction), 16000).into(),
        );
        body.insert("screenshotNames".into(), json!(names));
        if let Some(provider) = &settings.provider {
            body.insert("provider".into(), provider.clone().into());
        }
        body.insert(
            "model".into(),
            model.unwrap_or(settings.manual_model).as_str().into(),
        );
        body.insert(
            "permissionMode".into(),
            permission_mode
                .unwrap_or(settings.manual_permission_mode)
                .as_str()
                .into(),
        );
        if let Some(context) = context {
            body.insert("context".into(), context.to_json());
        }
        let path = format!("/api/app-debug/runs/{}/request", self.run_id());
        let result = self.post(&path, Value::Object(body)).await?;
        Ok(result
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned))
    }

    /// Issues a single-use ElevenLabs Scribe token for the voice input
    /// (`POST /api/app-debug/runs/{runId}/speech-token`). A run the server forgot (404/410) is
    /// recreated once. The server answers 412 while no ElevenLabs API key is registered in
    /// SamuraiAI.
    pub async fn speech_token(&self) -> Result<AiDebugSpeechToken, AiDebugError> {
        if !self.enabled() {
            return Err(AiDebugError::Config(
                "AI Debugger is only available in debug builds with the dev or test flavor".into(),
            ));
        }
        match self.post_speech_token().await {
            Err(error) if matches!(error.status(), Some(404 | 410)) => {
                self.rotate_run();
                self.post_speech_token().await
            }
            result => result,
        }
    }

    async fn post_speech_token(&self) -> Result<AiDebugSpeechToken, AiDebugError> {
        self.register().await?;
        let path = format!("/api/app-debug/runs/{}/speech-token", self.run_id());
        let result = self.post(&path, json!({})).await?;
        AiDebugSpeechToken::from_json(&result)
    }

    /// The app's context, or `None` when it supplied nothing.
    pub async fn capture_context(&self) -> Option<AiDebugContextSnapshot> {
        if !self.enabled() {
            return None;
        }
        let provider = self.0.config.context_provider.clone()?;
        provider().await.filter(|snapshot| !snapshot.is_empty())
    }

    // ---- Incidents ----------------------------------------------------------------------------

    /// Reports an unexpected error with the error model and mode.
    pub async fn report_error(&self, message: &str, stack: &str) {
        if !self.enabled() {
            return;
        }
        let settings = self.settings();
        self.report_incident(Incident {
            kind: "exception",
            message: redact(message),
            stack: redact(stack),
            metadata: Map::new(),
            provider: settings.provider,
            model: settings.error_model,
            permission_mode: settings.error_permission_mode,
        })
        .await;
    }

    /// Reports a trace that exceeded its threshold with the performance model and mode.
    pub async fn report_performance_trace(
        &self,
        category: &str,
        trace_name: &str,
        started_at: DateTime<Utc>,
        elapsed: Duration,
        threshold: Duration,
    ) {
        if !self.enabled() {
            return;
        }
        let settings = self.settings();
        let trace_name = redact(trace_name);
        let mut metadata = Map::new();
        metadata.insert("category".into(), category.into());
        metadata.insert("traceName".into(), trace_name.clone().into());
        metadata.insert("startedAt".into(), iso(started_at).into());
        metadata.insert("elapsedMs".into(), (elapsed.as_millis() as u64).into());
        metadata.insert("thresholdMs".into(), (threshold.as_millis() as u64).into());
        self.report_incident(Incident {
            kind: "performance",
            message: format!("Performance threshold exceeded: {trace_name}"),
            stack: String::new(),
            metadata,
            provider: settings.provider,
            model: settings.performance_model,
            permission_mode: settings.performance_permission_mode,
        })
        .await;
    }

    async fn report_incident(&self, incident: Incident) {
        let source = DIGITS
            .replace_all(&format!("{}:{}", incident.kind, incident.message), "N")
            .into_owned();
        let fingerprint = truncate(&source, 300);
        let now = Instant::now();
        {
            let mut state = self.0.state.borrow_mut();
            if state.fingerprints.contains(&fingerprint) {
                return;
            }
            if state
                .last_incident
                .is_some_and(|last| now.duration_since(last) < INCIDENT_BURST_WINDOW)
            {
                return;
            }
            state.fingerprints.insert(fingerprint);
            state.last_incident = Some(now);
        }
        let context = self.capture_context().await;
        if let Err(error) = self.send_incident(incident, context).await {
            log::debug!(target: LOG_TARGET, "AI Debugger error report failed: {error}");
        }
    }

    async fn send_incident(
        &self,
        incident: Incident,
        context: Option<AiDebugContextSnapshot>,
    ) -> Result<(), AiDebugError> {
        let timestamp = Utc::now();
        self.register().await?;
        self.flush_logs().await;
        let capture = self.0.capture.borrow().clone();
        if let Some(capture) = capture
            && let Some(image) = capture().await
        {
            let name = format!("{}-{}.png", incident.kind, timestamp.timestamp_millis());
            self.upload(&image, Some(name)).await?;
        }
        let mut body = Map::new();
        body.insert("kind".into(), incident.kind.into());
        body.insert("message".into(), incident.message.into());
        body.insert("stackTrace".into(), incident.stack.into());
        body.insert("timestamp".into(), iso(timestamp).into());
        if !incident.metadata.is_empty() {
            body.insert("metadata".into(), Value::Object(incident.metadata));
        }
        if let Some(provider) = incident.provider {
            body.insert("provider".into(), provider.into());
        }
        body.insert("model".into(), incident.model.as_str().into());
        body.insert(
            "permissionMode".into(),
            incident.permission_mode.as_str().into(),
        );
        if let Some(context) = context {
            body.insert("context".into(), context.to_json());
        }
        let path = format!("/api/app-debug/runs/{}/incidents", self.run_id());
        let result = self.post(&path, Value::Object(body)).await?;
        if result.get("sessionCreated") == Some(&Value::Bool(true)) {
            let session_id = match result.get("sessionId") {
                Some(Value::String(id)) => id.clone(),
                Some(Value::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            let listener = self.0.incident_listener.borrow().clone();
            if let Some(listener) = listener
                && !session_id.is_empty()
            {
                listener(incident.kind, &session_id);
            }
        }
        Ok(())
    }

    // ---- Breadcrumbs --------------------------------------------------------------------------

    /// Queues a breadcrumb; queues of 20 are sent at once, smaller ones after 2 seconds.
    pub fn add_log(&self, name: &str, params: Value, severity: &str) {
        if !self.enabled() {
            return;
        }
        let params = if params.is_null() { json!({}) } else { params };
        let entry = json!({
            "message": redact(&format!("{name} {params}")),
            "severity": severity,
            "timestamp": iso(Utc::now()),
        });
        let (flush_now, schedule) = {
            let mut state = self.0.state.borrow_mut();
            state.logs.push_back(entry);
            while state.logs.len() > MAX_LOGS {
                state.logs.pop_front();
            }
            if state.logs.len() >= LOG_FLUSH_COUNT {
                (true, None)
            } else if state.flush_scheduled {
                (false, None)
            } else {
                state.flush_scheduled = true;
                state.flush_generation += 1;
                (false, Some(state.flush_generation))
            }
        };
        if flush_now {
            let this = self.clone();
            self.spawn(async move { this.flush_logs().await });
        } else if let Some(generation) = schedule {
            let this = self.clone();
            self.spawn(async move {
                tokio::time::sleep(LOG_FLUSH_DELAY).await;
                {
                    let mut state = this.0.state.borrow_mut();
                    if state.flush_generation != generation {
                        return;
                    }
                    state.flush_scheduled = false;
                }
                this.flush_logs().await;
            });
        }
    }

    /// Sends up to 100 queued breadcrumbs; a failed batch goes back to the front of the queue.
    pub async fn flush_logs(&self) {
        if !self.enabled() {
            return;
        }
        let batch: Vec<Value> = {
            let mut state = self.0.state.borrow_mut();
            let count = state.logs.len().min(LOG_BATCH);
            state.logs.drain(..count).collect()
        };
        if batch.is_empty() {
            return;
        }
        let result = match self.register().await {
            Ok(()) => {
                let path = format!("/api/app-debug/runs/{}/events", self.run_id());
                self.post(&path, json!({ "events": batch }))
                    .await
                    .map(|_| ())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            let mut state = self.0.state.borrow_mut();
            for entry in batch.into_iter().rev() {
                state.logs.push_front(entry);
            }
            state.logs.truncate(MAX_LOGS);
            log::debug!(target: LOG_TARGET, "AI Debugger log upload failed: {error}");
        }
    }

    /// Breadcrumbs waiting to be sent.
    pub fn pending_logs(&self) -> usize {
        self.0.state.borrow().logs.len()
    }

    // ---- Reports from other threads ------------------------------------------------------------

    fn handle(&self, message: Message) {
        match message {
            Message::Log {
                name,
                params,
                severity,
            } => self.add_log(&name, params, severity),
            Message::HandledError { message, stack } => {
                if self.0.config.report_handled_errors {
                    let this = self.clone();
                    self.spawn(async move { this.report_error(&message, &stack).await });
                } else {
                    self.add_log(
                        "error",
                        json!({ "error": message, "stackTrace": stack }),
                        "error",
                    );
                }
            }
            Message::Panic { message, stack } => {
                let this = self.clone();
                self.spawn(async move { this.report_error(&message, &stack).await });
            }
            Message::TraceStart { id, name, wall, at } => self.trace_started(id, name, wall, at),
            Message::TraceStop { id, name, at } => self.trace_stopped(id, name, at),
        }
    }

    fn trace_started(&self, id: u64, name: String, wall: DateTime<Utc>, at: Instant) {
        let category = if name.starts_with(MODEL_LOAD_TRACE_PREFIX) {
            Some("model_load")
        } else if name.starts_with(INDICATOR_TRACE_PREFIX) {
            Some("indicator")
        } else {
            None
        };
        let threshold = category.map(|category| {
            let settings = self.settings();
            if category == "model_load" {
                settings.model_load_timeout
            } else {
                settings.indicator_timeout
            }
        });
        self.0.state.borrow_mut().traces.insert(
            id,
            Trace {
                at,
                category,
                threshold,
                exceeded: false,
            },
        );
        let (Some(category), Some(threshold)) = (category, threshold) else {
            return;
        };
        if threshold.is_zero() {
            return;
        }
        let this = self.clone();
        self.spawn(async move {
            tokio::time::sleep_until(at + threshold).await;
            {
                let mut state = this.0.state.borrow_mut();
                let Some(trace) = state.traces.get_mut(&id) else {
                    return;
                };
                if trace.exceeded {
                    return;
                }
                trace.exceeded = true;
            }
            let elapsed = Instant::now().duration_since(at);
            this.report_performance_trace(category, &name, wall, elapsed, threshold)
                .await;
        });
    }

    fn trace_stopped(&self, id: u64, name: String, at: Instant) {
        let Some(trace) = self.0.state.borrow_mut().traces.remove(&id) else {
            return;
        };
        let mut params = Map::new();
        params.insert(
            "duration_ms".into(),
            (at.duration_since(trace.at).as_millis() as u64).into(),
        );
        if let Some(category) = trace.category {
            params.insert("category".into(), category.into());
        }
        if let Some(threshold) = trace.threshold {
            params.insert("threshold_ms".into(), (threshold.as_millis() as u64).into());
        }
        if trace.category.is_some() {
            params.insert("threshold_exceeded".into(), trace.exceeded.into());
        }
        self.add_log(&name, Value::Object(params), "info");
    }

    // ---- Transport ----------------------------------------------------------------------------

    async fn post(&self, path: &str, body: Value) -> Result<Map<String, Value>, AiDebugError> {
        if !self.enabled() {
            return Ok(Map::new());
        }
        let config = &self.0.config;
        if config.project_id.is_empty() {
            return Err(AiDebugError::Config(
                "MASAMUNE_AI_DEBUGGER_PROJECT_ID is not configured".into(),
            ));
        }
        let base = config.endpoint.trim_end_matches('/');
        if base.is_empty() {
            return Err(AiDebugError::Config(
                "MASAMUNE_AI_DEBUGGER_ENDPOINT is not configured".into(),
            ));
        }
        if config.api_key.is_empty() {
            return Err(AiDebugError::Config(
                "MASAMUNE_AI_DEBUGGER_API_KEY is not configured".into(),
            ));
        }
        let headers = vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("x-api-key".to_owned(), config.api_key.clone()),
        ];
        config
            .transport
            .post(format!("{base}{path}"), headers, body)
            .await
    }
}

/// The report queue while a task handles it; returned to the controller when dropped.
struct Queue {
    controller: AiDebugController,
    receiver: Option<UnboundedReceiver<Message>>,
}

impl Queue {
    async fn next(&mut self) -> Option<Message> {
        self.receiver.as_mut()?.next().await
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        if let Some(receiver) = self.receiver.take()
            && let Ok(mut slot) = self.controller.0.receiver.try_borrow_mut()
        {
            *slot = Some(receiver);
        }
    }
}

struct Incident {
    kind: &'static str,
    message: String,
    stack: String,
    metadata: Map<String, Value>,
    provider: Option<String>,
    model: AiDebugModel,
    permission_mode: AiDebugPermissionMode,
}

fn create_run_id() -> String {
    let micros = Utc::now().timestamp_micros();
    let random = RandomState::new().hash_one(micros) as u32;
    format!("app-{micros}-{random:x}")
}

/// UTC ISO 8601 with milliseconds, like Dart's `toIso8601String()`.
fn iso(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// `defaultTargetPlatform.name` of the Flutter package.
fn platform_name() -> &'static str {
    match std::env::consts::OS {
        "android" => "android",
        "ios" => "iOS",
        "macos" => "macOS",
        "windows" => "windows",
        "linux" => "linux",
        _ if cfg!(target_arch = "wasm32") => "web",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_ids_are_unique() {
        let a = create_run_id();
        let b = create_run_id();
        assert!(a.starts_with("app-"));
        assert_ne!(a, b);
    }

    #[test]
    fn iso_has_milliseconds() {
        let time = DateTime::parse_from_rfc3339("2026-01-02T03:04:05.678Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(iso(time), "2026-01-02T03:04:05.678Z");
    }
}
