//! The floating debugger UI (`lib/src/ai_debug_overlay.dart`).
//!
//! Texts and accessibility identifiers/labels match the Flutter package so the same Maestro flows
//! (`ai_debugger_open`, `ai_debugger_auth_*`, `デバッグ匿名ログイン実行`, …) work on Whisker apps.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use futures_channel::oneshot;
use masamune_whisker_view_capture::ViewCapture;
use regex::Regex;
use whisker::css::{
    AlignItems, Border, Color, Display, FlexDirection, FlexWrap, FontWeight, JustifyContent,
    Overflow, PositionKind,
};
use whisker::event::{CustomEvent, TouchEvent};
use whisker::prelude::*;
use whisker::{Accessibility, AccessibilityRole, ElementRef, Owner, WhiskerValue};
use whisker_icons::{Icon, lucide};
use whisker_image::Image;
use whisker_input::{AutoCapitalize, Input, KeyboardType};
use whisker_safe_area::safe_area_insets;

use crate::config::AiDebugSpawner;
use crate::controller::{AiDebugController, LOG_TARGET};
use crate::settings::AiDebugSettings;
use crate::types::{
    AiDebugAccount, AiDebugAuthSnapshot, AiDebugAuthState, AiDebugCameraPicture, AiDebugModel,
    AiDebugPermissionMode, AiDebugPurchaseProduct,
};

const TOGGLE_SIZE: f32 = 52.0;
const CONTROL_SIZE: f32 = 40.0;
const CONTROL_ICON_SIZE: &str = "22";
const PANEL_HEIGHT: f32 = 360.0;
const KIWAME_EDIT: &str = "/dev:kiwame:edit";

const PANEL_COLOR: Color = Color::rgba(34, 34, 34, 0.9);
const DIALOG_COLOR: Color = Color::rgba(34, 34, 34, 0.97);
const CHIP_COLOR: Color = Color::rgba(255, 255, 255, 0.08);
const SELECTED_COLOR: Color = Color::rgba(138, 180, 248, 0.35);
const TEXT_COLOR: Color = Color::rgb(255, 255, 255);
const SUB_TEXT_COLOR: Color = Color::rgba(255, 255, 255, 0.7);
const ERROR_COLOR: Color = Color::rgb(255, 138, 128);
const ACCENT_COLOR: Color = Color::rgb(178, 255, 89);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PanelView {
    Prompt,
    Login,
    Purchase,
    Camera,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RequestType {
    BugFix,
    RequirementsEdit,
}

impl RequestType {
    fn label(self) -> &'static str {
        match self {
            Self::BugFix => "不具合修正",
            Self::RequirementsEdit => "要件修正",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::BugFix => lucide::Bug,
            Self::RequirementsEdit => lucide::NotebookPen,
        }
    }

    fn tone(self) -> Tone {
        match self {
            Self::BugFix => RED,
            Self::RequirementsEdit => TEAL,
        }
    }
}

/// A selector color: the Material base color and its lighter shade 300 for the icon.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Tone {
    base: (u8, u8, u8),
    light: &'static str,
}

const RED: Tone = Tone {
    base: (244, 67, 54),
    light: "#E57373",
};
const TEAL: Tone = Tone {
    base: (0, 150, 136),
    light: "#4DB6AC",
};
const BLUE: Tone = Tone {
    base: (33, 150, 243),
    light: "#64B5F6",
};
const ORANGE: Tone = Tone {
    base: (255, 152, 0),
    light: "#FFB74D",
};
const PURPLE: Tone = Tone {
    base: (156, 39, 176),
    light: "#BA68C8",
};
const GREEN: Tone = Tone {
    base: (76, 175, 80),
    light: "#81C784",
};

fn mode_icon(mode: AiDebugPermissionMode) -> &'static str {
    match mode {
        AiDebugPermissionMode::Plan => lucide::ClipboardList,
        AiDebugPermissionMode::BypassPermissions => lucide::Zap,
    }
}

fn mode_tone(mode: AiDebugPermissionMode) -> Tone {
    match mode {
        AiDebugPermissionMode::Plan => BLUE,
        AiDebugPermissionMode::BypassPermissions => ORANGE,
    }
}

fn model_icon(model: AiDebugModel) -> &'static str {
    match model {
        AiDebugModel::Astra => lucide::Award,
        AiDebugModel::Sol => lucide::Star,
        AiDebugModel::Terra => lucide::Zap,
        AiDebugModel::Luna => lucide::Wind,
    }
}

fn model_tone(model: AiDebugModel) -> Tone {
    match model {
        AiDebugModel::Astra => RED,
        AiDebugModel::Sol => PURPLE,
        AiDebugModel::Terra => ORANGE,
        AiDebugModel::Luna => GREEN,
    }
}

/// A screenshot kept in the panel (a PNG under the app cache).
#[derive(Clone, PartialEq, Debug)]
struct Shot {
    id: u64,
    path: String,
}

/// UI state shared by the overlay's parts.
#[derive(Clone, Copy)]
struct Ui {
    expanded: RwSignal<bool>,
    position: RwSignal<(f32, f32)>,
    screen: RwSignal<(f32, f32)>,
    view: RwSignal<PanelView>,
    request_type: RwSignal<RequestType>,
    text: RwSignal<String>,
    email: RwSignal<String>,
    password: RwSignal<String>,
    shots: RwSignal<Vec<Shot>>,
    preview: RwSignal<Option<String>>,
    settings: RwSignal<AiDebugSettings>,
    settings_open: RwSignal<bool>,
    draft: RwSignal<AiDebugSettings>,
    model_timeout: RwSignal<String>,
    indicator_timeout: RwSignal<String>,
    settings_error: RwSignal<Option<String>>,
    sending: RwSignal<bool>,
    busy: RwSignal<bool>,
    status: RwSignal<Option<String>>,
    error: RwSignal<Option<String>>,
    notification: RwSignal<Option<String>>,
    notification_generation: RwSignal<u64>,
    refresh: RwSignal<u64>,
    selected_product: RwSignal<Option<String>>,
    selected_picture: RwSignal<Option<String>>,
}

/// Everything the overlay's parts need.
#[derive(Clone, Copy)]
struct Ctx {
    controller: StoredValue<AiDebugController>,
    ui: Ui,
    capture: StoredValue<Rc<Capturer>>,
}

impl Ctx {
    fn controller(&self) -> AiDebugController {
        self.controller.get()
    }

    fn capture(&self) -> Rc<Capturer> {
        self.capture.get()
    }
}

/// Wraps the app with the AI debugger.
///
/// Renders only `children` outside [`masamune_whisker_logger::Flavor::Dev`]. In dev it adds the
/// floating button (tap: open, long press: screenshot and open, drag: move), the panel and the
/// `ai_debugger_auth_*` observation nodes, attaches the controller (breadcrumbs, incidents,
/// heartbeat) and captures `children` for screenshots (Android and iOS).
///
/// Put it at the root, outside `AppRoot` and the router, so it survives rebuilds:
///
/// ```ignore
/// render! {
///     AiDebugger(controller: debugger.clone()) {
///         AppRoot { Router(routes: routes()) {} }
///     }
/// }
/// ```
#[component]
pub fn ai_debugger(controller: AiDebugController, children: Children) -> Element {
    let children = children.clone();
    if !controller.enabled() {
        // A fragment cannot be a surface root, so keep the same container as the enabled tree.
        return render! {
            View(style: css!(
                flex_grow: 1.0,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                position: PositionKind::Relative,
            )) {
                {children()}
            }
        };
    }
    let settings = controller.settings();
    let ui = Ui {
        expanded: signal(false),
        position: signal((16.0, 80.0)),
        screen: signal((0.0, 0.0)),
        view: signal(PanelView::Prompt),
        request_type: signal(RequestType::BugFix),
        text: signal(String::new()),
        email: signal(String::new()),
        password: signal(String::new()),
        shots: signal(Vec::new()),
        preview: signal(None),
        settings: signal(settings.clone()),
        settings_open: signal(false),
        draft: signal(settings),
        model_timeout: signal(String::new()),
        indicator_timeout: signal(String::new()),
        settings_error: signal(None),
        sending: signal(false),
        busy: signal(false),
        status: signal(None),
        error: signal(None),
        notification: signal(None),
        notification_generation: signal(0),
        refresh: signal(0),
        selected_product: signal(None),
        selected_picture: signal(None),
    };
    let capture = Rc::new(Capturer::new());
    let ctx = Ctx {
        controller: StoredValue::new(controller.clone()),
        ui,
        capture: StoredValue::new(capture.clone()),
    };

    // Screen size for keeping the button inside the screen.
    if let Ok(info) = masamune_whisker_device_info::info() {
        ui.screen.set((info.width_dp as f32, info.height_dp as f32));
    }
    let metrics = masamune_whisker_device_info::on_metrics_changed(move |metrics| {
        ui.screen
            .set((metrics.width_dp as f32, metrics.height_dp as f32));
    });
    on_cleanup(move || drop(metrics));

    let owner = Owner::current();
    let spawner: AiDebugSpawner = Rc::new(move |future| {
        if let Some(owner) = owner.filter(|owner| owner.is_alive()) {
            owner.with(|| spawn_local(future));
        }
    });
    controller.attach(spawner);
    controller.set_capture({
        let capture = capture.clone();
        move || {
            let capture = capture.clone();
            async move {
                let path = capture.capture().await.ok()?;
                std::fs::read(path).ok()
            }
        }
    });
    controller.set_incident_listener(move |kind, _session| show_incident_notification(ui, kind));
    {
        let controller = controller.clone();
        on_mount(move || {
            spawn_local(async move {
                if let Err(error) = controller.resume().await {
                    ui.status.set(Some(error.to_string()));
                }
            });
        });
    }

    let capture_ref = capture.element;
    let on_captured = {
        let capture = capture.clone();
        move |event: CustomEvent| capture.captured(&event)
    };
    render! {
        View(style: css!(
            flex_grow: 1.0,
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            position: PositionKind::Relative,
        )) {
            ViewCapture(
                element_ref: capture_ref,
                on_captured: on_captured,
                style: css!(flex_grow: 1.0, display: Display::Flex, flex_direction: FlexDirection::Column),
            ) {
                {children()}
            }
            AuthObservation(ctx: ctx)
            Show(when: move || ui.notification.with(Option::is_some)) {
                IncidentNotification(ui: ui)
            }
            Floating(ctx: ctx)
            Show(when: move || ui.preview.with(Option::is_some)) {
                Preview(ui: ui)
            }
            Show(when: move || ui.settings_open.get()) {
                SettingsDialog(ctx: ctx)
            }
        }
    }
}

// ---- Screenshots ------------------------------------------------------------------------------

/// Drives `masamune-whisker-view-capture` and waits for its `captured` event.
struct Capturer {
    element: ElementRef,
    pending: RefCell<Option<oneshot::Sender<Result<String, String>>>>,
}

impl Capturer {
    fn new() -> Self {
        Self {
            element: ElementRef::new(),
            pending: RefCell::new(None),
        }
    }

    /// Path of a PNG of the app (without the debugger).
    async fn capture(&self) -> Result<String, String> {
        let (sender, receiver) = oneshot::channel();
        *self.pending.borrow_mut() = Some(sender);
        let file_name = format!("ai_debugger_{}.png", chrono::Utc::now().timestamp_millis());
        let params = WhiskerValue::map([("fileName", WhiskerValue::String(file_name))]);
        if let Err(error) = self.element.command("capture", params) {
            self.pending.borrow_mut().take();
            return Err(error.to_string());
        }
        tokio::select! {
            result = receiver => result.unwrap_or_else(|_| Err("capture cancelled".into())),
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                self.pending.borrow_mut().take();
                Err("capture timed out".into())
            }
        }
    }

    fn captured(&self, event: &CustomEvent) {
        let field = |name: &str| match &event.detail {
            WhiskerValue::Map(map) => match map.get(name) {
                Some(WhiskerValue::String(value)) => Some(value.clone()),
                _ => None,
            },
            _ => None,
        };
        let result =
            field("path").ok_or_else(|| field("error").unwrap_or_else(|| "capture failed".into()));
        if let Some(sender) = self.pending.borrow_mut().take() {
            let _ = sender.send(result);
        }
    }
}

fn next_shot_id() -> u64 {
    thread_local! {
        static NEXT: Cell<u64> = const { Cell::new(1) };
    }
    NEXT.with(|next| next.replace(next.get() + 1))
}

/// Captures the app and adds it to the panel, dropping the oldest beyond the limit.
fn capture_screenshot(ctx: &Ctx, open: bool) {
    let ctx = *ctx;
    spawn_local(async move {
        match ctx.capture().capture().await {
            Ok(path) => {
                let max = ctx.controller().config().max_screenshots;
                ctx.ui.shots.update(|shots| {
                    shots.push(Shot {
                        id: next_shot_id(),
                        path,
                    });
                    while shots.len() > max {
                        shots.remove(0);
                    }
                });
                if open {
                    ctx.ui.expanded.set(true);
                }
            }
            Err(error) => ctx
                .ui
                .status
                .set(Some(format!("Screenshot failed: {error}"))),
        }
    });
}

// ---- Notification -----------------------------------------------------------------------------

fn show_incident_notification(ui: Ui, kind: &str) {
    let text = if kind == "performance" {
        "処理時間のしきい値超過を検出し、AIセッションを作成しました"
    } else {
        "想定外エラーを検出し、AIセッションを作成しました"
    };
    ui.notification.set(Some(text.into()));
    ui.notification_generation.update(|g| *g += 1);
    let generation = ui.notification_generation.get_untracked();
    spawn_local(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        if ui.notification_generation.get_untracked() == generation {
            ui.notification.set(None);
        }
    });
}

#[component]
fn incident_notification(ui: Ui) -> Element {
    let insets = safe_area_insets();
    let style = computed(move || {
        css!(
            position: PositionKind::Absolute,
            left: px(16),
            right: px(16),
            display: Display::Flex,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
            z_index: 2000,
        )
        .top(px(insets.get().top as f32 + 12.0))
    });
    let text = computed(move || ui.notification.get().unwrap_or_default());
    let accessibility = computed(move || Accessibility::new().label(text.get()));
    render! {
        View(style: style) {
            View(
                accessibility: accessibility,
                style: css!(
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    gap: px(10),
                    max_width: px(480),
                    padding: (px(12), px(14)),
                    border_radius: px(12),
                    background_color: Color::rgba(42, 42, 42, 0.94),
                ),
            ) {
                Text(value: "✦", style: label(ACCENT_COLOR, 18.0, false))
                Text(value: text, style: label(TEXT_COLOR, 13.0, true).flex_shrink(1.0))
            }
        }
    }
}

// ---- Auth observation -------------------------------------------------------------------------

fn boot_id() -> &'static str {
    static BOOT_ID: OnceLock<String> = OnceLock::new();
    BOOT_ID.get_or_init(|| {
        use std::hash::{BuildHasher, RandomState};
        let micros = chrono::Utc::now().timestamp_micros().max(0) as u64;
        let random = RandomState::new().hash_one(micros);
        format!("{}-{}", base36(micros), base36(random))
    })
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = Vec::new();
    loop {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
        if value == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// 1×1 nodes `ai_debugger_auth_<name>` whose label is the value, present even while collapsed.
#[component]
fn auth_observation(ctx: Ctx) -> Element {
    let snapshot_fn = ctx.controller().config().auth_snapshot.clone();
    let refresh = ctx.ui.refresh;
    let values = computed(move || {
        refresh.get();
        let snapshot = match &snapshot_fn {
            Some(snapshot) => snapshot().unwrap_or_else(|error| {
                log::warn!(target: LOG_TARGET, "AI Debugger auth snapshot failed: {error}");
                AiDebugAuthSnapshot {
                    state: AiDebugAuthState::Error,
                    ..Default::default()
                }
            }),
            None => AiDebugAuthSnapshot::default(),
        };
        snapshot.observation(boot_id()).to_vec()
    });
    let insets = safe_area_insets();
    let style = computed(move || {
        css!(
            position: PositionKind::Absolute,
            right: px(0),
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
        )
        .top(px(insets.get().top as f32))
    });
    render! {
        View(style: style) {
            ForEach(
                each: move || values.get(),
                key: |(name, value): &(&'static str, String)| format!("{name}={value}"),
                children: move |entry: ReadSignal<(&'static str, String)>| {
                    let (name, value) = entry.get_untracked();
                    render! {
                        View(
                            accessibility: Accessibility::new()
                                .identifier(format!("ai_debugger_auth_{name}"))
                                .label(value),
                            style: css!(width: px(1), height: px(1)),
                        ) {}
                    }
                },
            )
        }
    }
}

// ---- Floating button / panel ------------------------------------------------------------------

fn panel_size(ui: &Ui) -> (f32, f32) {
    let (width, _) = ui.screen.get();
    let insets = safe_area_insets().get();
    let available = (width - insets.leading as f32 - insets.trailing as f32 - 16.0).max(160.0);
    (available.min(360.0), PANEL_HEIGHT)
}

fn item_size(ui: &Ui) -> (f32, f32) {
    if ui.expanded.get() {
        panel_size(ui)
    } else {
        (TOGGLE_SIZE, TOGGLE_SIZE)
    }
}

/// Keeps `position` inside the safe area of the screen (when the screen size is known).
fn clamp(position: (f32, f32), screen: (f32, f32), item: (f32, f32)) -> (f32, f32) {
    let insets = safe_area_insets().get_untracked();
    let (min_x, min_y) = (insets.leading as f32, insets.top as f32);
    let axis = |value: f32, min: f32, size: f32, end_inset: f32, item: f32| {
        if size <= 0.0 {
            return value.max(min);
        }
        value.clamp(min, (size - end_inset - item).max(min))
    };
    (
        axis(position.0, min_x, screen.0, insets.trailing as f32, item.0),
        axis(position.1, min_y, screen.1, insets.bottom as f32, item.1),
    )
}

/// Touch position and item position when a drag started.
type DragStart = Option<(f64, f64, f32, f32)>;

/// Moves the button or panel with touch drags; `dragged` suppresses the tap that ends a drag.
#[derive(Clone, Default)]
struct Drag {
    start: Rc<Cell<DragStart>>,
    dragged: Rc<Cell<bool>>,
}

impl Drag {
    fn start(&self, ui: Ui, event: &TouchEvent) {
        let (x, y) = ui.position.get_untracked();
        self.start.set(Some((event.detail.x, event.detail.y, x, y)));
        self.dragged.set(false);
    }

    fn moved(&self, ui: Ui, event: &TouchEvent) {
        let Some((touch_x, touch_y, x, y)) = self.start.get() else {
            return;
        };
        let (dx, dy) = (event.detail.x - touch_x, event.detail.y - touch_y);
        if !self.dragged.get() && dx.abs() < 6.0 && dy.abs() < 6.0 {
            return;
        }
        self.dragged.set(true);
        let item = item_size(&ui);
        ui.position.set(clamp(
            (x + dx as f32, y + dy as f32),
            ui.screen.get_untracked(),
            item,
        ));
    }

    fn end(&self) {
        self.start.set(None);
    }
}

#[component]
fn floating(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    // Clamp again when the screen, the safe area or the size changes.
    effect(move || {
        let item = item_size(&ui);
        let screen = ui.screen.get();
        let position = ui.position.get_untracked();
        let clamped = clamp(position, screen, item);
        if clamped != position {
            ui.position.set(clamped);
        }
    });
    let style = computed(move || {
        let (x, y) = ui.position.get();
        let (width, height) = item_size(&ui);
        css!(position: PositionKind::Absolute, z_index: 1500)
            .left(px(x))
            .top(px(y))
            .width(px(width))
            .height(px(height))
    });
    render! {
        View(style: style) {
            Show(when: move || ui.expanded.get(), fallback: {
                        move || render! { Toggle(ctx: ctx) }
            }) {
                Panel(ctx: ctx)
            }
        }
    }
}

#[component]
fn toggle(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    let drag = Drag::default();
    let on_touchstart = {
        let drag = drag.clone();
        move |event: TouchEvent| drag.start(ui, &event)
    };
    let on_touchmove = {
        let drag = drag.clone();
        move |event: TouchEvent| drag.moved(ui, &event)
    };
    let on_touchend = {
        let drag = drag.clone();
        move |_: TouchEvent| drag.end()
    };
    let on_tap = {
        let drag = drag.clone();
        move |_: TouchEvent| {
            if !drag.dragged.get() {
                ui.expanded.set(true);
            }
        }
    };
    let on_longpress = {
        let drag = drag.clone();
        move |_: TouchEvent| {
            if !drag.dragged.get() {
                capture_screenshot(&ctx, true);
            }
        }
    };
    render! {
        View(
            accessibility: Accessibility::new()
                .identifier("ai_debugger_open")
                .label("AI Debuggerを開く")
                .role(AccessibilityRole::Button),
            style: css!(
                width: px(TOGGLE_SIZE),
                height: px(TOGGLE_SIZE),
                border_radius: px(TOGGLE_SIZE / 2.0),
                display: Display::Flex,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                background_color: Color::rgba(0, 0, 0, 0.55),
            ),
            on_touchstart_catch: on_touchstart,
            on_touchmove_catch: on_touchmove,
            on_touchend_catch: on_touchend,
            on_tap_catch: on_tap,
            on_longpress_catch: on_longpress,
        ) {
            Text(value: "✦", style: label(SUB_TEXT_COLOR, 22.0, false))
        }
    }
}

/// Reads the app's login state; `None` when there is no debug login.
fn login_state(ctx: &Ctx) -> Option<Result<bool, String>> {
    ctx.controller()
        .config()
        .auth
        .as_ref()
        .map(|auth| (auth.is_logged_in)())
}

#[component]
fn panel(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    let auth_label = {
        computed(move || {
            ui.refresh.get();
            let state = login_state(&ctx)?;
            let value = match state {
                _ if ui.busy.get() => "unknown",
                Ok(true) => "authenticated",
                Ok(false) => "unauthenticated",
                Err(_) => "unknown",
            };
            Some(format!("AI Debugger auth {value}"))
        })
    };
    let accessibility = computed(move || match auth_label.get() {
        Some(label) => Accessibility::new().label(label),
        None => Accessibility::new(),
    });
    render! {
        View(
            accessibility: accessibility,
            style: css!(
                width: percent(100),
                height: percent(100),
                padding: px(12),
                border_radius: px(16),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                gap: px(8),
                overflow: Overflow::Hidden,
                background_color: PANEL_COLOR,
            ),
            on_tap_catch: |_| {},
        ) {
            Show(when: move || ui.view.get() == PanelView::Prompt) {
                PromptPanel(ctx: ctx)
            }
            Show(when: move || ui.view.get() == PanelView::Login) {
                LoginPanel(ctx: ctx)
            }
            Show(when: move || ui.view.get() == PanelView::Purchase) {
                PurchasePanel(ctx: ctx)
            }
            Show(when: move || ui.view.get() == PanelView::Camera) {
                CameraPanel(ctx: ctx)
            }
        }
    }
}

/// Title row: drag handle, optional back button and the close button.
#[component]
fn panel_header(ctx: Ctx, title: &'static str, #[prop(default = false)] back: bool) -> Element {
    let ui = ctx.ui;
    let drag = Drag::default();
    let on_touchstart = {
        let drag = drag.clone();
        move |event: TouchEvent| drag.start(ui, &event)
    };
    let on_touchmove = {
        let drag = drag.clone();
        move |event: TouchEvent| drag.moved(ui, &event)
    };
    let on_touchend = move |_: TouchEvent| drag.end();
    let leading = if back {
        render! {
            View(
                accessibility: button_a11y("AI入力へ戻る"),
                style: icon_button(),
                on_tap: move |_| {
                    if !ui.busy.get_untracked() {
                        ui.error.set(None);
                        ui.view.set(PanelView::Prompt);
                    }
                },
            ) {
                Text(value: "←", style: label(TEXT_COLOR, 16.0, false))
            }
        }
    } else {
        render! { Text(value: "✦", style: label(TEXT_COLOR, 16.0, false)) }
    };
    render! {
        View(
            style: css!(
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: px(8),
            ),
            on_touchstart: on_touchstart,
            on_touchmove: on_touchmove,
            on_touchend: on_touchend,
        ) {
            Fragment { {leading} }
            Text(value: title, style: label(TEXT_COLOR, 15.0, true).flex_grow(1.0))
            View(
                accessibility: Accessibility::new()
                    .identifier("ai_debugger_close")
                    .label("閉じる")
                    .role(AccessibilityRole::Button),
                style: icon_button(),
                on_tap: move |_| {
                    ui.view.set(PanelView::Prompt);
                    ui.expanded.set(false);
                },
            ) {
                Text(value: "×", style: label(TEXT_COLOR, 18.0, false))
            }
        }
    }
}

// ---- Prompt -------------------------------------------------------------------------------------

#[component]
fn prompt_panel(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    let controller = ctx.controller();
    let config = controller.config();
    let (has_auth, has_purchase, has_camera) = (
        config.auth.is_some(),
        config.purchase.is_some(),
        config.camera.is_some(),
    );
    let auth_text = {
        computed(move || {
            ui.refresh.get();
            match login_state(&ctx) {
                Some(Ok(true)) => "ログアウト".to_owned(),
                _ => "ログイン".to_owned(),
            }
        })
    };
    let auth_a11y = {
        computed(move || {
            ui.refresh.get();
            let identifier = match login_state(&ctx) {
                Some(Ok(true)) => "ai_debugger_auth_logout",
                _ => "ai_debugger_auth_login",
            };
            Accessibility::new()
                .identifier(identifier)
                .label("AIデバッガー認証")
                .role(AccessibilityRole::Button)
        })
    };
    let tools = if has_auth || has_purchase || has_camera {
        render! {
            View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, gap: px(6))) {
                Show(when: move || has_auth) {
                    View(
                        accessibility: auth_a11y,
                        style: chip_style(false),
                        on_tap: {
                                                move |_| open_authentication(&ctx)
                        },
                    ) {
                        Text(value: auth_text, style: label(TEXT_COLOR, 12.0, false))
                    }
                }
                Show(when: move || has_purchase) {
                    View(
                        accessibility: button_a11y("AIデバッガー課金管理"),
                        style: chip_style(false),
                        on_tap: {
                                                move |_| open_purchase(&ctx)
                        },
                    ) {
                        Text(value: "課金管理", style: label(TEXT_COLOR, 12.0, false))
                    }
                }
                Show(when: move || has_camera) {
                    View(
                        accessibility: button_a11y("AIデバッガーカメラ管理"),
                        style: chip_style(false),
                        on_tap: {
                                                move |_| open_camera(&ctx)
                        },
                    ) {
                        Text(value: "カメラ", style: label(TEXT_COLOR, 12.0, false))
                    }
                }
            }
        }
    } else {
        render! { Fragment {} }
    };
    let status = computed(move || ui.status.get().unwrap_or_default());
    let request_label = computed(move || ui.request_type.get().label().to_owned());
    let mode_label = computed(move || ui.settings.get().manual_permission_mode.label().to_owned());
    let model_label = computed(move || ui.settings.get().manual_model.label().to_owned());
    let request_icon = computed(move || ui.request_type.get().icon().to_owned());
    let request_color = computed(move || ui.request_type.get().tone().light.to_owned());
    let request_style = computed(move || control_button(Some(ui.request_type.get().tone())));
    let mode_icon_svg =
        computed(move || mode_icon(ui.settings.get().manual_permission_mode).to_owned());
    let mode_color = computed(move || {
        mode_tone(ui.settings.get().manual_permission_mode)
            .light
            .to_owned()
    });
    let mode_style =
        computed(move || control_button(Some(mode_tone(ui.settings.get().manual_permission_mode))));
    let model_icon_svg = computed(move || model_icon(ui.settings.get().manual_model).to_owned());
    let model_color = computed(move || model_tone(ui.settings.get().manual_model).light.to_owned());
    let model_style =
        computed(move || control_button(Some(model_tone(ui.settings.get().manual_model))));
    let send_icon = computed(move || {
        if ui.sending.get() {
            lucide::LoaderCircle
        } else {
            lucide::SendHorizontal
        }
        .to_owned()
    });
    render! {
        Fragment {
            PanelHeader(ctx: ctx, title: "AI Debugger")
            Fragment { {tools} }
            View(
                accessibility: Accessibility::new().label("AIデバッガー指示入力"),
                style: css!(flex_grow: 1.0, display: Display::Flex),
            ) {
                Input(
                    text: ui.text,
                    placeholder: "AIへの指示",
                    multiline: true,
                    style: input_style().flex_grow(1.0),
                    placeholder_color: "#9E9E9E",
                    caret_color: "#FFFFFF",
                )
            }
            Show(when: move || ui.status.with(Option::is_some)) {
                Text(value: status, style: label(SUB_TEXT_COLOR, 11.0, false))
            }
            View(style: css!(
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::Center,
                gap: px(6),
            )) {
                View(
                    accessibility: button_a11y("スクリーンショット"),
                    style: control_button(None),
                    on_tap: move |_| capture_screenshot(&ctx, false),
                ) {
                    Icon(svg: lucide::MonitorSmartphone, color: "#FFFFFF", size: CONTROL_ICON_SIZE)
                }
                ForEach(
                    each: move || ui.shots.get(),
                    key: |shot: &Shot| shot.id,
                    children: move |shot: ReadSignal<Shot>| {
                        let Shot { id, path } = shot.get_untracked();
                        let src = format!("file://{path}");
                        render! {
                            View(style: css!(width: px(48), height: px(48), position: PositionKind::Relative)) {
                                View(
                                    style: css!(width: px(48), height: px(48), border_radius: px(6), overflow: Overflow::Hidden),
                                    on_tap: move |_| ui.preview.set(Some(path.clone())),
                                ) {
                                    Image(src: src, style: css!(width: px(48), height: px(48)))
                                }
                                View(
                                    accessibility: button_a11y("スクリーンショットを削除"),
                                    style: css!(
                                        position: PositionKind::Absolute,
                                        top: px(-4),
                                        right: px(-4),
                                        width: px(18),
                                        height: px(18),
                                        border_radius: px(9),
                                        display: Display::Flex,
                                        justify_content: JustifyContent::Center,
                                        align_items: AlignItems::Center,
                                        background_color: Color::rgba(0, 0, 0, 0.8),
                                    ),
                                    on_tap: move |_| ui.shots.update(|shots| shots.retain(|s| s.id != id)),
                                ) {
                                    Text(value: "×", style: label(TEXT_COLOR, 11.0, false))
                                }
                            }
                        }
                    },
                )
                View(style: css!(flex_grow: 1.0)) {}
                View(
                    accessibility: button_a11y("設定"),
                    style: control_button(None),
                    on_tap: move |_| open_settings(&ctx),
                ) {
                    Icon(svg: lucide::Settings, color: "#FFFFFF", size: CONTROL_ICON_SIZE)
                }
                View(
                    accessibility: computed(move || button_a11y(&request_label.get())),
                    style: request_style,
                    on_tap: move |_| ui.request_type.update(|t| {
                        *t = match t {
                            RequestType::BugFix => RequestType::RequirementsEdit,
                            RequestType::RequirementsEdit => RequestType::BugFix,
                        }
                    }),
                ) {
                    Icon(svg: request_icon, color: request_color, size: CONTROL_ICON_SIZE)
                }
                View(
                    accessibility: computed(move || button_a11y(&format!("Mode {}", mode_label.get()))),
                    style: mode_style,
                    on_tap: move |_| update_manual(&ctx, |s| {
                        s.manual_permission_mode = s.manual_permission_mode.next();
                    }),
                ) {
                    Icon(svg: mode_icon_svg, color: mode_color, size: CONTROL_ICON_SIZE)
                }
                View(
                    accessibility: computed(move || button_a11y(&format!("Model {}", model_label.get()))),
                    style: model_style,
                    on_tap: move |_| update_manual(&ctx, |s| {
                        s.manual_model = s.manual_model.next();
                    }),
                ) {
                    Icon(svg: model_icon_svg, color: model_color, size: CONTROL_ICON_SIZE)
                }
                View(
                    accessibility: button_a11y("送信"),
                    style: control_button(None),
                    on_tap: move |_| send(&ctx),
                ) {
                    Icon(svg: send_icon, color: "#B2FF59", size: CONTROL_ICON_SIZE)
                }
            }
        }
    }
}

/// Adds the `/dev:kiwame:edit` command to requirement edits unless the text already starts with it.
fn instruction_text(text: &str, request_type: RequestType) -> String {
    if request_type == RequestType::BugFix {
        return text.to_owned();
    }
    static COMMAND: OnceLock<Regex> = OnceLock::new();
    let command = COMMAND.get_or_init(|| Regex::new(r"^/\S+").expect("command pattern"));
    if command.find(text).map(|m| m.as_str()) == Some(KIWAME_EDIT) {
        return text.to_owned();
    }
    format!("{KIWAME_EDIT}\n\n{text}")
}

fn send(ctx: &Ctx) {
    let ui = ctx.ui;
    let text = ui.text.get_untracked().trim().to_owned();
    if text.is_empty() || ui.sending.get_untracked() {
        return;
    }
    let instruction = instruction_text(&text, ui.request_type.get_untracked());
    let shots = ui.shots.get_untracked();
    let controller = ctx.controller().clone();
    ui.sending.set(true);
    ui.status.set(None);
    spawn_local(async move {
        let images: Vec<Vec<u8>> = shots
            .iter()
            .filter_map(|shot| std::fs::read(&shot.path).ok())
            .collect();
        match controller.send(&instruction, &images, None, None).await {
            Ok(session) => {
                ui.text.set(String::new());
                ui.shots.set(Vec::new());
                ui.expanded.set(false);
                ui.status.set(Some(match session {
                    Some(id) => format!("Session: {id}"),
                    None => "送信しました".into(),
                }));
            }
            Err(error) => ui.status.set(Some(error.to_string())),
        }
        ui.sending.set(false);
    });
}

fn update_manual(ctx: &Ctx, change: impl FnOnce(&mut AiDebugSettings)) {
    let mut settings = ctx.ui.settings.get_untracked();
    change(&mut settings);
    ctx.ui.settings.set(settings.clone());
    if let Err(error) = ctx.controller().update_settings(settings) {
        ctx.ui
            .status
            .set(Some(format!("設定の保存に失敗しました: {error}")));
    }
}

// ---- Login --------------------------------------------------------------------------------------

fn open_authentication(ctx: &Ctx) {
    let ui = ctx.ui;
    if ui.busy.get_untracked() {
        return;
    }
    let Some(auth) = ctx.controller().config().auth.clone() else {
        return;
    };
    match (auth.is_logged_in)() {
        Err(error) => {
            ui.error.set(Some(error));
            ui.view.set(PanelView::Login);
        }
        Ok(false) => {
            ui.error.set(None);
            ui.view.set(PanelView::Login);
        }
        Ok(true) => {
            ui.busy.set(true);
            ui.error.set(None);
            spawn_local(async move {
                if let Err(error) = (auth.logout)().await {
                    ui.error.set(Some(error));
                }
                ui.refresh.update(|r| *r += 1);
                ui.busy.set(false);
            });
        }
    }
}

/// Runs a login and returns to the prompt once the app reports a signed-in user.
fn run_login(
    ctx: &Ctx,
    login: impl std::future::Future<Output = Result<(), String>> + 'static,
    clear_password: bool,
) {
    let ui = ctx.ui;
    let Some(auth) = ctx.controller().config().auth.clone() else {
        return;
    };
    ui.busy.set(true);
    ui.error.set(None);
    spawn_local(async move {
        match login.await {
            Ok(()) => match (auth.is_logged_in)() {
                Ok(true) => {
                    if clear_password {
                        ui.password.set(String::new());
                    }
                    ui.view.set(PanelView::Prompt);
                }
                Ok(false) => ui
                    .error
                    .set(Some("ログイン状態を確認できませんでした".into())),
                Err(error) => ui.error.set(Some(error)),
            },
            Err(error) => ui.error.set(Some(error)),
        }
        ui.refresh.update(|r| *r += 1);
        ui.busy.set(false);
    });
}

fn login_with_form(ctx: &Ctx) {
    let ui = ctx.ui;
    if ui.busy.get_untracked() {
        return;
    }
    let Some(auth) = ctx.controller().config().auth.clone() else {
        return;
    };
    let email = ui.email.get_untracked().trim().to_owned();
    let password = ui.password.get_untracked();
    if email.is_empty() || password.is_empty() {
        ui.error
            .set(Some("メールアドレスとパスワードを入力してください".into()));
        return;
    }
    run_login(ctx, (auth.login)(email, password), true);
}

fn login_with_account(ctx: &Ctx, account: &AiDebugAccount) {
    if ctx.ui.busy.get_untracked() {
        return;
    }
    let Some(auth) = ctx.controller().config().auth.clone() else {
        return;
    };
    run_login(
        ctx,
        (auth.login)(account.id.clone(), account.password.clone()),
        false,
    );
}

fn login_anonymously(ctx: &Ctx) {
    if ctx.ui.busy.get_untracked() {
        return;
    }
    let Some(anonymous) = ctx
        .controller()
        .config()
        .auth
        .as_ref()
        .and_then(|auth| auth.anonymous_login.clone())
    else {
        return;
    };
    run_login(ctx, anonymous(), true);
}

/// `ai_debugger_auth_account_<alias or account_N>`, unless the alias is invalid or not unique.
fn account_identifier(accounts: &[AiDebugAccount], index: usize) -> Option<String> {
    static ALIAS: OnceLock<Regex> = OnceLock::new();
    let pattern =
        ALIAS.get_or_init(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9_-]{0,63}$").expect("alias"));
    let alias_of = |i: usize, account: &AiDebugAccount| {
        account
            .alias
            .clone()
            .unwrap_or_else(|| format!("account_{i}"))
    };
    let alias = alias_of(index, &accounts[index]);
    let count = accounts
        .iter()
        .enumerate()
        .filter(|(i, account)| alias_of(*i, account) == alias)
        .count();
    (pattern.is_match(&alias) && count == 1).then(|| format!("ai_debugger_auth_account_{alias}"))
}

#[component]
fn login_panel(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    let accounts = ctx.controller().config().accounts.clone();
    let has_anonymous = ctx
        .controller()
        .config()
        .auth
        .as_ref()
        .is_some_and(|auth| auth.anonymous_login.is_some());
    let error = computed(move || ui.error.get().unwrap_or_default());
    let running = move || ui.busy.get();
    let login_text = computed(move || {
        if running() {
            "ログイン中…"
        } else {
            "ログイン"
        }
        .to_owned()
    });
    let anonymous_text = computed(move || {
        if running() {
            "ログイン中…"
        } else {
            "匿名ログイン"
        }
        .to_owned()
    });
    let body = if accounts.is_empty() {
        render! {
            Fragment {
                View(accessibility: Accessibility::new().label("デバッグログイン メールアドレス")) {
                    Input(
                        text: ui.email,
                        placeholder: "メールアドレス",
                        keyboard_type: KeyboardType::Email,
                        auto_capitalize: AutoCapitalize::None,
                        autocorrect: false,
                        spell_check: false,
                        style: input_style().height(px(40)),
                        placeholder_color: "#9E9E9E",
                    )
                }
                View(accessibility: Accessibility::new().label("デバッグログイン パスワード")) {
                    Input(
                        text: ui.password,
                        placeholder: "パスワード",
                        secure: true,
                        auto_capitalize: AutoCapitalize::None,
                        autocorrect: false,
                        spell_check: false,
                        style: input_style().height(px(40)),
                        placeholder_color: "#9E9E9E",
                    )
                }
                View(
                    accessibility: button_a11y("デバッグログイン実行"),
                    style: action_button(),
                    on_tap: move |_| login_with_form(&ctx),
                ) {
                    Text(value: login_text, style: label(TEXT_COLOR, 13.0, true))
                }
            }
        }
    } else {
        let indexed: Vec<(usize, AiDebugAccount)> = accounts.iter().cloned().enumerate().collect();
        render! {
            ScrollView(style: css!(max_height: px(180))) {
                ForEach(
                    each: move || indexed.clone(),
                    key: |(index, _): &(usize, AiDebugAccount)| *index,
                    children: move |entry: ReadSignal<(usize, AiDebugAccount)>| {
                        let (index, account) = entry.get_untracked();
                        let mut a11y = Accessibility::new()
                            .label(format!(
                                "デバッグアカウントログイン {}",
                                account.alias.clone().unwrap_or_else(|| account.id.clone())
                            ))
                            .role(AccessibilityRole::Button);
                        if let Some(identifier) = account_identifier(&accounts, index) {
                            a11y = a11y.identifier(identifier);
                        }
                        let shown = account.alias.clone().unwrap_or_else(|| account.id.clone());
                                        render! {
                            View(
                                accessibility: a11y,
                                style: list_row(),
                                on_tap: move |_| login_with_account(&ctx, &account),
                            ) {
                                Text(value: shown, style: label(TEXT_COLOR, 13.0, false))
                            }
                        }
                    },
                )
            }
        }
    };
    render! {
        Fragment {
            PanelHeader(ctx: ctx, title: "デバッグログイン", back: true)
            Fragment { {body} }
            Show(when: move || has_anonymous) {
                View(
                    accessibility: button_a11y("デバッグ匿名ログイン実行"),
                    style: action_button(),
                    on_tap: {
                                        move |_| login_anonymously(&ctx)
                    },
                ) {
                    Text(value: anonymous_text, style: label(TEXT_COLOR, 13.0, true))
                }
            }
            Show(when: move || ui.error.with(Option::is_some)) {
                Text(value: error, style: label(ERROR_COLOR, 12.0, false))
            }
        }
    }
}

// ---- Purchase -----------------------------------------------------------------------------------

struct PurchaseState {
    products: Vec<AiDebugPurchaseProduct>,
    purchased: Vec<String>,
}

fn purchase_state(ctx: &Ctx) -> Result<PurchaseState, String> {
    let controller = ctx.controller();
    let Some(purchase) = controller.config().purchase.as_ref() else {
        return Ok(PurchaseState {
            products: Vec::new(),
            purchased: Vec::new(),
        });
    };
    let products = (purchase.products)();
    for (index, product) in products.iter().enumerate() {
        if products[..index].iter().any(|p| p.id == product.id) {
            return Err(format!("課金商品IDが重複しています: {}", product.id));
        }
    }
    let purchased = products
        .iter()
        .filter(|product| (purchase.is_purchased)(product))
        .map(|product| product.id.clone())
        .collect();
    Ok(PurchaseState {
        products,
        purchased,
    })
}

fn open_purchase(ctx: &Ctx) {
    ctx.ui.error.set(None);
    ctx.ui.refresh.update(|r| *r += 1);
    ctx.ui.view.set(PanelView::Purchase);
}

fn run_operation(
    ui: Ui,
    operation: impl std::future::Future<Output = Result<(), String>> + 'static,
) {
    if ui.busy.get_untracked() {
        return;
    }
    ui.busy.set(true);
    ui.error.set(None);
    spawn_local(async move {
        if let Err(error) = operation.await {
            ui.error.set(Some(error));
        }
        ui.refresh.update(|r| *r += 1);
        ui.busy.set(false);
    });
}

#[component]
fn purchase_panel(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    let state = {
        computed(move || {
            ui.refresh.get();
            purchase_state(&ctx).map(|state| (state.products, state.purchased))
        })
    };
    let purchased = computed(move || match state.get() {
        Ok((products, purchased)) => products
            .into_iter()
            .filter(|p| purchased.contains(&p.id))
            .collect(),
        Err(_) => Vec::new(),
    });
    let available = computed(move || match state.get() {
        Ok((products, purchased)) => products
            .into_iter()
            .filter(|p| !purchased.contains(&p.id))
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    });
    let selected = computed(move || {
        let available = available.get();
        let current = ui.selected_product.get();
        available
            .iter()
            .find(|p| Some(&p.id) == current.as_ref())
            .or(available.first())
            .map(|p| p.id.clone())
    });
    let empty_text = computed(move || match state.get() {
        Ok((products, _)) if products.is_empty() => "課金アイテムがありません".to_owned(),
        Ok(_) => "すべてのアイテムが課金中です".to_owned(),
        Err(_) => String::new(),
    });
    let error = computed(move || {
        ui.error
            .get()
            .or_else(|| state.get().err())
            .unwrap_or_default()
    });
    let button_text = computed(move || {
        if ui.busy.get() {
            "処理中…"
        } else {
            "課金する"
        }
        .to_owned()
    });
    render! {
        Fragment {
            PanelHeader(ctx: ctx, title: "デバッグ課金", back: true)
            ScrollView(style: css!(flex_grow: 1.0)) {
                Text(value: "課金中", style: label(TEXT_COLOR, 13.0, true))
                Show(when: move || purchased.with(Vec::is_empty)) {
                    Text(value: "課金中のアイテムはありません", style: label(SUB_TEXT_COLOR, 12.0, false))
                }
                ForEach(
                    each: move || purchased.get(),
                    key: |product: &AiDebugPurchaseProduct| product.id.clone(),
                    children: move |product: ReadSignal<AiDebugPurchaseProduct>| {
                        let product = product.get_untracked();
                                        let label_text = product.label.clone();
                        let a11y = button_a11y(&format!("課金解除 {}", product.id));
                        render! {
                            View(style: list_row().justify_content(JustifyContent::SpaceBetween)) {
                                Text(value: label_text, style: label(TEXT_COLOR, 13.0, false))
                                View(
                                    accessibility: a11y,
                                    style: icon_button(),
                                    on_tap: move |_| {
                                        let Some(purchase) = ctx.controller().config().purchase.clone() else {
                                            return;
                                        };
                                        run_operation(ctx.ui, (purchase.cancel)(product.clone()));
                                    },
                                ) {
                                    Text(value: "×", style: label(TEXT_COLOR, 14.0, false))
                                }
                            }
                        }
                    },
                )
                Show(when: move || available.with(Vec::is_empty)) {
                    Text(value: empty_text, style: label(SUB_TEXT_COLOR, 12.0, false))
                }
                View(
                    accessibility: Accessibility::new().label("デバッグ課金 商品選択"),
                    style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, gap: px(6)),
                ) {
                    ForEach(
                        each: move || available.get(),
                        key: |product: &AiDebugPurchaseProduct| product.id.clone(),
                        children: move |product: ReadSignal<AiDebugPurchaseProduct>| {
                            let product = product.get_untracked();
                            let id = product.id.clone();
                            let shown = product.label.clone();
                            let is_selected = {
                                let id = id.clone();
                                computed(move || selected.get().as_ref() == Some(&id))
                            };
                            render! {
                                View(
                                    accessibility: computed(move || Accessibility::new()
                                        .label(product.label.clone())
                                        .role(AccessibilityRole::Radio)),
                                    style: computed(move || chip_style(is_selected.get())),
                                    on_tap: move |_| ui.selected_product.set(Some(id.clone())),
                                ) {
                                    Text(value: shown, style: label(TEXT_COLOR, 12.0, false))
                                }
                            }
                        },
                    )
                }
            }
            Show(when: move || available.with(|a| !a.is_empty())) {
                View(
                    accessibility: button_a11y("デバッグ強制課金実行"),
                    style: action_button(),
                    on_tap: {
                                        move |_| {
                            let Some(purchase) = ctx.controller().config().purchase.clone() else {
                                return;
                            };
                            let Some(id) = selected.get_untracked() else {
                                return;
                            };
                            let Some(product) = available.get_untracked().into_iter().find(|p| p.id == id) else {
                                return;
                            };
                            run_operation(ctx.ui, (purchase.purchase)(product));
                        }
                    },
                ) {
                    Text(value: button_text, style: label(TEXT_COLOR, 13.0, true))
                }
            }
            Show(when: move || !error.get().is_empty()) {
                Text(value: error, style: label(ERROR_COLOR, 12.0, false))
            }
        }
    }
}

// ---- Camera -------------------------------------------------------------------------------------

struct CameraState {
    pictures: Vec<AiDebugCameraPicture>,
    active: Option<AiDebugCameraPicture>,
}

fn camera_state(ctx: &Ctx) -> Result<CameraState, String> {
    let controller = ctx.controller();
    let Some(camera) = controller.config().camera.as_ref() else {
        return Ok(CameraState {
            pictures: Vec::new(),
            active: None,
        });
    };
    let pictures = (camera.pictures)();
    for (index, picture) in pictures.iter().enumerate() {
        if pictures[..index].iter().any(|p| p.id == picture.id) {
            return Err(format!("カメラ画像IDが重複しています: {}", picture.id));
        }
    }
    let active: Vec<_> = pictures
        .iter()
        .filter(|picture| (camera.is_set)(picture))
        .cloned()
        .collect();
    if active.len() > 1 {
        return Err("設定中のカメラ画像が複数あります。".into());
    }
    Ok(CameraState {
        pictures,
        active: active.into_iter().next(),
    })
}

fn open_camera(ctx: &Ctx) {
    ctx.ui.error.set(None);
    ctx.ui.refresh.update(|r| *r += 1);
    ctx.ui.view.set(PanelView::Camera);
}

#[component]
fn camera_panel(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    let state = {
        computed(move || {
            ui.refresh.get();
            camera_state(&ctx).map(|state| (state.pictures, state.active))
        })
    };
    let active = computed(move || state.get().ok().and_then(|(_, active)| active));
    let available = computed(move || match state.get() {
        Ok((pictures, active)) => pictures
            .into_iter()
            .filter(|p| Some(p) != active.as_ref())
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    });
    let selected = computed(move || {
        let available = available.get();
        let current = ui.selected_picture.get();
        available
            .iter()
            .find(|p| Some(&p.id) == current.as_ref())
            .or(available.first())
            .map(|p| p.id.clone())
    });
    let active_label = computed(move || active.get().map(|p| p.label).unwrap_or_default());
    let unset_a11y = computed(move || {
        button_a11y(&format!(
            "カメラ画像解除 {}",
            active.get().map(|p| p.id).unwrap_or_default()
        ))
    });
    let empty_text = computed(move || match state.get() {
        Ok((pictures, _)) if pictures.is_empty() => "カメラ画像がありません".to_owned(),
        Ok(_) => "選択できるカメラ画像がありません".to_owned(),
        Err(_) => String::new(),
    });
    let error = computed(move || {
        ui.error
            .get()
            .or_else(|| state.get().err())
            .unwrap_or_default()
    });
    let button_text = computed(move || {
        if ui.busy.get() {
            "処理中…"
        } else {
            "設定する"
        }
        .to_owned()
    });
    render! {
        Fragment {
            PanelHeader(ctx: ctx, title: "デバッグカメラ", back: true)
            ScrollView(style: css!(flex_grow: 1.0)) {
                Text(value: "設定中", style: label(TEXT_COLOR, 13.0, true))
                Show(when: move || active.with(Option::is_none), fallback: {
                                move || {
                                        render! {
                            View(style: list_row().justify_content(JustifyContent::SpaceBetween)) {
                                Text(value: active_label, style: label(TEXT_COLOR, 13.0, false))
                                View(
                                    accessibility: unset_a11y,
                                    style: icon_button(),
                                    on_tap: move |_| {
                                        let Some(camera) = ctx.controller().config().camera.clone() else {
                                            return;
                                        };
                                        run_operation(ctx.ui, (camera.unset)());
                                    },
                                ) {
                                    Text(value: "×", style: label(TEXT_COLOR, 14.0, false))
                                }
                            }
                        }
                    }
                }) {
                    Text(value: "デバッグ画像は設定されていません", style: label(SUB_TEXT_COLOR, 12.0, false))
                }
                Show(when: move || available.with(Vec::is_empty)) {
                    Text(value: empty_text, style: label(SUB_TEXT_COLOR, 12.0, false))
                }
                View(
                    accessibility: Accessibility::new().label("デバッグカメラ 画像選択"),
                    style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, gap: px(6)),
                ) {
                    ForEach(
                        each: move || available.get(),
                        key: |picture: &AiDebugCameraPicture| picture.id.clone(),
                        children: move |picture: ReadSignal<AiDebugCameraPicture>| {
                            let picture = picture.get_untracked();
                            let id = picture.id.clone();
                            let shown = picture.label.clone();
                            let is_selected = {
                                let id = id.clone();
                                computed(move || selected.get().as_ref() == Some(&id))
                            };
                            render! {
                                View(
                                    accessibility: computed(move || Accessibility::new()
                                        .label(picture.label.clone())
                                        .role(AccessibilityRole::Radio)),
                                    style: computed(move || chip_style(is_selected.get())),
                                    on_tap: move |_| ui.selected_picture.set(Some(id.clone())),
                                ) {
                                    Text(value: shown, style: label(TEXT_COLOR, 12.0, false))
                                }
                            }
                        },
                    )
                }
            }
            Show(when: move || available.with(|a| !a.is_empty())) {
                View(
                    accessibility: button_a11y("デバッグカメラ画像設定実行"),
                    style: action_button(),
                    on_tap: {
                                        move |_| {
                            let Some(camera) = ctx.controller().config().camera.clone() else {
                                return;
                            };
                            let Some(id) = selected.get_untracked() else {
                                return;
                            };
                            let Some(picture) = available.get_untracked().into_iter().find(|p| p.id == id) else {
                                return;
                            };
                            run_operation(ctx.ui, (camera.set)(picture));
                        }
                    },
                ) {
                    Text(value: button_text, style: label(TEXT_COLOR, 13.0, true))
                }
            }
            Show(when: move || !error.get().is_empty()) {
                Text(value: error, style: label(ERROR_COLOR, 12.0, false))
            }
        }
    }
}

// ---- Preview ------------------------------------------------------------------------------------

#[component]
fn preview(ui: Ui) -> Element {
    let src = computed(move || {
        ui.preview
            .get()
            .map(|path| format!("file://{path}"))
            .unwrap_or_default()
    });
    render! {
        View(
            style: css!(
                position: PositionKind::Absolute,
                top: px(0), left: px(0), right: px(0), bottom: px(0),
                z_index: 2500,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                background_color: Color::rgba(0, 0, 0, 0.85),
            ),
            on_tap_catch: move |_| ui.preview.set(None),
        ) {
            Image(src: src, style: css!(width: percent(90), height: percent(75)))
            View(
                accessibility: button_a11y("プレビューを閉じる"),
                style: icon_button(),
                on_tap_catch: move |_| ui.preview.set(None),
            ) {
                Text(value: "×", style: label(TEXT_COLOR, 22.0, false))
            }
        }
    }
}

// ---- Settings -----------------------------------------------------------------------------------

fn seconds_text(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds.fract() == 0.0 {
        format!("{}", seconds as u64)
    } else {
        format!("{seconds:.1}")
    }
}

fn parse_seconds(text: &str) -> Option<Duration> {
    let seconds: f64 = text.trim().parse().ok()?;
    (seconds > 0.0 && seconds <= 86400.0).then(|| Duration::from_millis((seconds * 1000.0) as u64))
}

fn open_settings(ctx: &Ctx) {
    let ui = ctx.ui;
    let settings = ctx.controller().settings();
    ui.settings.set(settings.clone());
    ui.model_timeout
        .set(seconds_text(settings.model_load_timeout));
    ui.indicator_timeout
        .set(seconds_text(settings.indicator_timeout));
    ui.draft.set(settings);
    ui.settings_error.set(None);
    ui.settings_open.set(true);
}

fn save_settings(ctx: &Ctx) {
    let ui = ctx.ui;
    let (Some(model), Some(indicator)) = (
        parse_seconds(&ui.model_timeout.get_untracked()),
        parse_seconds(&ui.indicator_timeout.get_untracked()),
    ) else {
        ui.settings_error
            .set(Some("0より大きく86400以下の秒数を入力してください".into()));
        return;
    };
    let mut draft = ui.draft.get_untracked();
    draft.model_load_timeout = model;
    draft.indicator_timeout = indicator;
    match ctx.controller().update_settings(draft) {
        Ok(()) => {
            ui.settings.set(ctx.controller().settings());
            ui.settings_open.set(false);
        }
        Err(error) => ui
            .settings_error
            .set(Some(format!("設定の保存に失敗しました: {error}"))),
    }
}

#[component]
fn settings_dialog(ctx: Ctx) -> Element {
    let ui = ctx.ui;
    let providers: Vec<Option<String>> = std::iter::once(None)
        .chain(ctx.controller().providers().iter().cloned().map(Some))
        .collect();
    let has_providers = providers.len() > 1;
    let providers = StoredValue::new(providers);
    let error = computed(move || ui.settings_error.get().unwrap_or_default());
    render! {
        View(
            style: css!(
                position: PositionKind::Absolute,
                top: px(0), left: px(0), right: px(0), bottom: px(0),
                z_index: 2400,
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                background_color: Color::rgba(0, 0, 0, 0.5),
            ),
            on_tap_catch: |_| {},
        ) {
            View(style: css!(
                width: percent(90),
                max_width: px(420),
                max_height: percent(85),
                padding: px(16),
                gap: px(10),
                border_radius: px(16),
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                background_color: DIALOG_COLOR,
            )) {
                View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, align_items: AlignItems::Center)) {
                    Text(value: "AI Debugger設定", style: label(TEXT_COLOR, 16.0, true).flex_grow(1.0))
                    View(
                        accessibility: button_a11y("設定を閉じる"),
                        style: icon_button(),
                        on_tap: move |_| ui.settings_open.set(false),
                    ) {
                        Text(value: "×", style: label(TEXT_COLOR, 18.0, false))
                    }
                }
                ScrollView(style: css!(flex_shrink: 1.0)) {
                    Show(when: move || has_providers) {
                        Text(value: "プロバイダー", style: label(TEXT_COLOR, 13.0, true))
                        View(style: wrap_row()) {
                            ForEach(
                                each: move || providers.get(),
                                key: |provider: &Option<String>| provider.clone().unwrap_or_default(),
                                children: move |provider: ReadSignal<Option<String>>| {
                                    let provider = provider.get_untracked();
                                    let shown = provider.clone().unwrap_or_else(|| "自動".into());
                                    let a11y = button_a11y(&format!(
                                        "AI Debugger provider {}",
                                        provider.clone().unwrap_or_else(|| "automatic".into())
                                    ));
                                    let selected = {
                                        let provider = provider.clone();
                                        computed(move || ui.draft.get().provider == provider)
                                    };
                                    render! {
                                        View(
                                            accessibility: a11y,
                                            style: computed(move || chip_style(selected.get())),
                                            on_tap: move |_| {
                                                let provider = provider.clone();
                                                ui.draft.update(|d| d.provider = provider);
                                            },
                                        ) {
                                            Text(value: shown, style: label(TEXT_COLOR, 12.0, false))
                                        }
                                    }
                                },
                            )
                        }
                    }
                    SessionSection(ui: ui, title: "エラー時", performance: false)
                    SessionSection(ui: ui, title: "計測超過時", performance: true)
                    Text(value: "超過判定時間（秒）", style: label(TEXT_COLOR, 13.0, true))
                    View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, gap: px(8))) {
                        TimeoutField(text: ui.model_timeout, title: "モデル読込")
                        TimeoutField(text: ui.indicator_timeout, title: "インジケーター")
                    }
                }
                Show(when: move || !error.get().is_empty()) {
                    Text(value: error, style: label(ERROR_COLOR, 12.0, false))
                }
                View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, justify_content: JustifyContent::FlexEnd, gap: px(8))) {
                    View(
                        accessibility: button_a11y("キャンセル"),
                        style: chip_style(false),
                        on_tap: move |_| ui.settings_open.set(false),
                    ) {
                        Text(value: "キャンセル", style: label(TEXT_COLOR, 13.0, false))
                    }
                    View(
                        accessibility: button_a11y("保存"),
                        style: chip_style(true),
                        on_tap: move |_| save_settings(&ctx),
                    ) {
                        Text(value: "保存", style: label(TEXT_COLOR, 13.0, true))
                    }
                }
            }
        }
    }
}

/// Model and permission chips of the error or performance incidents.
#[component]
fn session_section(ui: Ui, title: &'static str, performance: bool) -> Element {
    let models: Vec<AiDebugModel> = AiDebugModel::ALL.to_vec();
    let modes: Vec<AiDebugPermissionMode> = AiDebugPermissionMode::ALL.to_vec();
    render! {
        Fragment {
            Text(value: title, style: label(TEXT_COLOR, 13.0, true))
            View(style: wrap_row()) {
                ForEach(
                    each: move || models.clone(),
                    key: |model: &AiDebugModel| model.as_str(),
                    children: move |model: ReadSignal<AiDebugModel>| {
                        let model = model.get_untracked();
                        let selected = computed(move || {
                            let draft = ui.draft.get();
                            if performance { draft.performance_model == model } else { draft.error_model == model }
                        });
                        render! {
                            View(
                                accessibility: button_a11y(&format!("{title} Model {}", model.label())),
                                style: computed(move || chip_style(selected.get())),
                                on_tap: move |_| ui.draft.update(|d| {
                                    if performance { d.performance_model = model } else { d.error_model = model }
                                }),
                            ) {
                                Text(value: model.label(), style: label(TEXT_COLOR, 12.0, false))
                            }
                        }
                    },
                )
                ForEach(
                    each: move || modes.clone(),
                    key: |mode: &AiDebugPermissionMode| mode.as_str(),
                    children: move |mode: ReadSignal<AiDebugPermissionMode>| {
                        let mode = mode.get_untracked();
                        let selected = computed(move || {
                            let draft = ui.draft.get();
                            if performance {
                                draft.performance_permission_mode == mode
                            } else {
                                draft.error_permission_mode == mode
                            }
                        });
                        render! {
                            View(
                                accessibility: button_a11y(&format!("{title} Mode {}", mode.label())),
                                style: computed(move || chip_style(selected.get())),
                                on_tap: move |_| ui.draft.update(|d| {
                                    if performance {
                                        d.performance_permission_mode = mode
                                    } else {
                                        d.error_permission_mode = mode
                                    }
                                }),
                            ) {
                                Text(value: mode.label(), style: label(TEXT_COLOR, 12.0, false))
                            }
                        }
                    },
                )
            }
        }
    }
}

#[component]
fn timeout_field(text: RwSignal<String>, title: &'static str) -> Element {
    render! {
        View(
            accessibility: Accessibility::new().label(title),
            style: css!(flex_grow: 1.0, display: Display::Flex, flex_direction: FlexDirection::Column, gap: px(4)),
        ) {
            Text(value: title, style: label(SUB_TEXT_COLOR, 11.0, false))
            View(style: css!(display: Display::Flex, flex_direction: FlexDirection::Row, align_items: AlignItems::Center, gap: px(4))) {
                Input(
                    text: text,
                    keyboard_type: KeyboardType::Decimal,
                    style: input_style().height(px(36)).flex_grow(1.0),
                )
                Text(value: "秒", style: label(SUB_TEXT_COLOR, 12.0, false))
            }
        }
    }
}

// ---- Styles -------------------------------------------------------------------------------------

fn label(color: Color, size: f32, bold: bool) -> Css {
    css!(
        color: color,
        font_size: px(size),
        font_weight: if bold { FontWeight::Bold } else { FontWeight::Normal },
    )
}

fn button_a11y(label: &str) -> Accessibility {
    Accessibility::new()
        .label(label)
        .role(AccessibilityRole::Button)
}

fn icon_button() -> Css {
    css!(
        min_width: px(32),
        height: px(32),
        padding: (px(0), px(6)),
        border_radius: px(16),
        display: Display::Flex,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
    )
}

/// The square buttons of the prompt's bottom row; `tone` adds the tinted selector frame.
fn control_button(tone: Option<Tone>) -> Css {
    let style = css!(
        width: px(CONTROL_SIZE),
        height: px(CONTROL_SIZE),
        border_radius: px(10),
        display: Display::Flex,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
    );
    match tone {
        Some(Tone {
            base: (r, g, b), ..
        }) => style.background_color(Color::rgba(r, g, b, 0.16)).border(
            Border::new()
                .width(px(1))
                .solid()
                .color(Color::rgba(r, g, b, 0.75)),
        ),
        None => style,
    }
}

fn chip_style(selected: bool) -> Css {
    css!(
        padding: (px(6), px(10)),
        border_radius: px(14),
        display: Display::Flex,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        background_color: if selected { SELECTED_COLOR } else { CHIP_COLOR },
    )
}

fn action_button() -> Css {
    css!(
        height: px(40),
        border_radius: px(20),
        display: Display::Flex,
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        background_color: SELECTED_COLOR,
    )
}

fn list_row() -> Css {
    css!(
        min_height: px(40),
        padding: (px(8), px(10)),
        border_radius: px(10),
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        background_color: CHIP_COLOR,
        margin_bottom: px(6),
    )
}

fn wrap_row() -> Css {
    css!(
        display: Display::Flex,
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        gap: px(6),
        margin_bottom: px(8),
    )
}

fn input_style() -> Css {
    css!(
        padding: (px(8), px(10)),
        border_radius: px(10),
        font_size: px(14),
        color: TEXT_COLOR,
        background_color: Color::rgba(255, 255, 255, 0.08),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_edits_get_the_kiwame_command() {
        assert_eq!(instruction_text("fix", RequestType::BugFix), "fix");
        assert_eq!(
            instruction_text("change copy", RequestType::RequirementsEdit),
            "/dev:kiwame:edit\n\nchange copy"
        );
        assert_eq!(
            instruction_text("/dev:kiwame:edit already", RequestType::RequirementsEdit),
            "/dev:kiwame:edit already"
        );
        assert_eq!(
            instruction_text("/other cmd", RequestType::RequirementsEdit),
            "/dev:kiwame:edit\n\n/other cmd"
        );
    }

    #[test]
    fn account_identifiers() {
        let accounts = vec![
            AiDebugAccount::new("a@x.com", "p").alias("alice"),
            AiDebugAccount::new("b@x.com", "p"),
            AiDebugAccount::new("c@x.com", "p").alias("bad alias"),
            AiDebugAccount::new("d@x.com", "p").alias("dup"),
            AiDebugAccount::new("e@x.com", "p").alias("dup"),
        ];
        assert_eq!(
            account_identifier(&accounts, 0).as_deref(),
            Some("ai_debugger_auth_account_alice")
        );
        assert_eq!(
            account_identifier(&accounts, 1).as_deref(),
            Some("ai_debugger_auth_account_account_1")
        );
        assert_eq!(account_identifier(&accounts, 2), None);
        assert_eq!(account_identifier(&accounts, 3), None);
    }

    #[test]
    fn seconds_round_trip() {
        assert_eq!(seconds_text(Duration::from_secs(5)), "5");
        assert_eq!(seconds_text(Duration::from_millis(2500)), "2.5");
        assert_eq!(parse_seconds("2.5"), Some(Duration::from_millis(2500)));
        assert_eq!(parse_seconds("0"), None);
        assert_eq!(parse_seconds("86401"), None);
        assert_eq!(parse_seconds("x"), None);
    }

    #[test]
    fn auth_observation_values() {
        let snapshot = AiDebugAuthSnapshot {
            state: AiDebugAuthState::Unauthenticated,
            login_count: Some(0),
            ..Default::default()
        };
        let values = snapshot.observation("boot");
        assert_eq!(values[0], ("schema_version", "1".to_owned()));
        assert_eq!(values[2], ("identity", "none".to_owned()));
        assert_eq!(values[5], ("boot_id", "boot".to_owned()));
        assert_eq!(values[6], ("login_count", "0".to_owned()));
        let authenticated = AiDebugAuthSnapshot {
            state: AiDebugAuthState::Authenticated,
            identity: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(
            authenticated.observation("b")[2],
            ("identity", "unsupported".to_owned())
        );
        assert!(boot_id().contains('-'));
        assert_eq!(base36(35), "z");
        assert_eq!(base36(36), "10");
    }
}
