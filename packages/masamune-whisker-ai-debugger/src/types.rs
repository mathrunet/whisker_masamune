//! Values shared by the controller and the overlay (`lib/src/ai_debug_post.dart`).

use std::future::Future;
use std::rc::Rc;

use futures_util::FutureExt;
use futures_util::future::LocalBoxFuture;
use serde_json::{Map, Value};

use crate::redact::Sanitizer;

/// Models available to AI Debugger sessions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AiDebugModel {
    /// Fastest model.
    Luna,
    /// Balanced model.
    Terra,
    /// High-capability model.
    #[default]
    Sol,
    /// Highest-capability model.
    Astra,
}

impl AiDebugModel {
    /// Every model, in the order of the settings chips.
    pub const ALL: [Self; 4] = [Self::Luna, Self::Terra, Self::Sol, Self::Astra];

    /// Wire name sent to SamuraiAI.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Luna => "luna",
            Self::Terra => "terra",
            Self::Sol => "sol",
            Self::Astra => "astra",
        }
    }

    /// Name shown in the overlay.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Luna => "Luna",
            Self::Terra => "Terra",
            Self::Sol => "Sol",
            Self::Astra => "Astra",
        }
    }

    /// Parses a wire name, accepting the legacy names (`haiku`, `sonnet`, `opus`, `mythos`).
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "luna" | "haiku" => Some(Self::Luna),
            "terra" | "sonnet" => Some(Self::Terra),
            "sol" | "opus" => Some(Self::Sol),
            "astra" | "mythos" => Some(Self::Astra),
            _ => None,
        }
    }

    /// Next model of the overlay's model button (Astra → Sol → Terra → Luna → Astra).
    pub const fn next(self) -> Self {
        match self {
            Self::Astra => Self::Sol,
            Self::Sol => Self::Terra,
            Self::Terra => Self::Luna,
            Self::Luna => Self::Astra,
        }
    }
}

/// Permission modes available to AI Debugger sessions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AiDebugPermissionMode {
    /// Investigate and wait for plan approval before editing.
    #[default]
    Plan,
    /// Investigate, edit and verify without a plan approval pause.
    BypassPermissions,
}

impl AiDebugPermissionMode {
    pub const ALL: [Self; 2] = [Self::Plan, Self::BypassPermissions];

    /// Wire name sent to SamuraiAI.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::BypassPermissions => "bypassPermissions",
        }
    }

    /// Name shown in the overlay.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Plan => "Plan",
            Self::BypassPermissions => "bypassPermissions",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "plan" => Some(Self::Plan),
            "bypassPermissions" => Some(Self::BypassPermissions),
            _ => None,
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Plan => Self::BypassPermissions,
            Self::BypassPermissions => Self::Plan,
        }
    }
}

/// A bounded snapshot of the app state attached to a debug request.
///
/// Whisker has no widget tree to walk, so `widget_tree` is only sent when the app supplies it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiDebugContextSnapshot {
    /// Human-readable current page name.
    pub page_name: Option<String>,
    /// Current route path or route name.
    pub route: Option<String>,
    /// Optional description of the visible UI.
    pub widget_tree: Option<String>,
    /// Explicitly selected JSON-compatible diagnostic values.
    pub values: Map<String, Value>,
}

impl AiDebugContextSnapshot {
    pub(crate) fn is_empty(&self) -> bool {
        self.page_name.as_deref().is_none_or(str::is_empty)
            && self.route.as_deref().is_none_or(str::is_empty)
            && self.widget_tree.as_deref().is_none_or(str::is_empty)
            && self.values.is_empty()
    }

    /// The redacted and truncated JSON sent as `context`.
    pub fn to_json(&self) -> Value {
        let mut sanitizer = Sanitizer::new();
        let mut json = Map::new();
        if let Some(page_name) = self.page_name.as_deref().filter(|v| !v.is_empty()) {
            json.insert("pageName".into(), sanitizer.text(page_name, 200).into());
        }
        if let Some(route) = self.route.as_deref().filter(|v| !v.is_empty()) {
            json.insert("route".into(), sanitizer.text(route, 1000).into());
        }
        if let Some(tree) = self.widget_tree.as_deref().filter(|v| !v.is_empty()) {
            json.insert("widgetTree".into(), sanitizer.text(tree, 16000).into());
        }
        if !self.values.is_empty() {
            json.insert(
                "values".into(),
                sanitizer.value(&Value::Object(self.values.clone()), 0, None),
            );
        }
        Value::Object(json)
    }
}

/// A purchase product shown by the debug purchase panel.
#[derive(Clone, Debug, Eq)]
pub struct AiDebugPurchaseProduct {
    /// Identifier used to resolve the app's actual product.
    pub id: String,
    /// Label shown in the overlay.
    pub label: String,
}

impl AiDebugPurchaseProduct {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

impl PartialEq for AiDebugPurchaseProduct {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A picture shown by the debug camera panel.
#[derive(Clone, Debug, Eq)]
pub struct AiDebugCameraPicture {
    /// Identifier used to resolve the app's actual image.
    pub id: String,
    /// Label shown in the overlay.
    pub label: String,
}

impl AiDebugCameraPicture {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

impl PartialEq for AiDebugCameraPicture {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// A debug account selectable from the login panel. The password is never shown.
#[derive(Clone, Eq)]
pub struct AiDebugAccount {
    /// Identifier (typically an email) shown when there is no alias and passed to the login.
    pub id: String,
    /// Password passed to the login only.
    pub password: String,
    /// Optional non-secret automation alias matching `[A-Za-z0-9][A-Za-z0-9_-]{0,63}`.
    pub alias: Option<String>,
}

impl std::fmt::Debug for AiDebugAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiDebugAccount")
            .field("id", &self.id)
            .field("alias", &self.alias)
            .finish_non_exhaustive()
    }
}

impl PartialEq for AiDebugAccount {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl AiDebugAccount {
    pub fn new(id: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            password: password.into(),
            alias: None,
        }
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Parses `id1:pass1,id2:pass2` (`MASAMUNE_AI_DEBUGGER_ACCOUNTS`).
    ///
    /// Entries are trimmed and split at the first colon, so passwords may contain colons. Entries
    /// without a colon, with an empty id or an empty password are skipped; a repeated id keeps the
    /// first entry.
    pub fn parse(source: &str) -> Vec<Self> {
        let mut accounts: Vec<Self> = Vec::new();
        for entry in source.split(',').map(str::trim) {
            let Some((id, password)) = entry.split_once(':') else {
                continue;
            };
            let id = id.trim();
            if id.is_empty() || password.is_empty() || accounts.iter().any(|a| a.id == id) {
                continue;
            }
            accounts.push(Self::new(id, password));
        }
        accounts
    }
}

/// Authentication state observed from the app's real auth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AiDebugAuthState {
    #[default]
    Unknown,
    Initializing,
    Authenticated,
    Unauthenticated,
    Error,
}

impl AiDebugAuthState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Initializing => "initializing",
            Self::Authenticated => "authenticated",
            Self::Unauthenticated => "unauthenticated",
            Self::Error => "error",
        }
    }
}

/// Completion of the app's startup session restoration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AiDebugAuthRestoreState {
    #[default]
    Unsupported,
    Pending,
    Running,
    Completed,
    Failed,
}

impl AiDebugAuthRestoreState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// App startup readiness, independent of the current route.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AiDebugAuthBootState {
    #[default]
    Initializing,
    Ready,
    Error,
}

impl AiDebugAuthBootState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Initializing => "initializing",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

/// A read-only snapshot of the real authentication and startup state, exposed to Maestro.
///
/// Never derive `identity` from the selected debug account or the route: supply a stable,
/// non-secret opaque token derived from the authenticated user id. `login_count` counts every
/// explicit sign-in attempt since process start (including automatic and failed ones, excluding
/// restoration); leave it `None` unless every sign-in path is covered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiDebugAuthSnapshot {
    pub state: AiDebugAuthState,
    pub restore_state: AiDebugAuthRestoreState,
    pub boot_state: AiDebugAuthBootState,
    pub identity: Option<String>,
    pub login_count: Option<u64>,
}

impl AiDebugAuthSnapshot {
    /// The `(name, value)` pairs of the `ai_debugger_auth_<name>` observation nodes.
    pub(crate) fn observation(&self, boot_id: &str) -> [(&'static str, String); 7] {
        let identity = match self.state {
            AiDebugAuthState::Authenticated => self
                .identity
                .clone()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "unsupported".into()),
            AiDebugAuthState::Unauthenticated => "none".into(),
            _ => "unsupported".into(),
        };
        [
            ("schema_version", "1".into()),
            ("state", self.state.as_str().into()),
            ("identity", identity),
            ("restore_state", self.restore_state.as_str().into()),
            ("boot_state", self.boot_state.as_str().into()),
            ("boot_id", boot_id.into()),
            (
                "login_count",
                self.login_count
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unsupported".into()),
            ),
        ]
    }
}

/// Error text returned by app callbacks; shown in the overlay as is.
pub type AiDebugCallbackResult<T = ()> = Result<T, String>;

type AsyncFn0 = Rc<dyn Fn() -> LocalBoxFuture<'static, AiDebugCallbackResult>>;
type AsyncFn2 = Rc<dyn Fn(String, String) -> LocalBoxFuture<'static, AiDebugCallbackResult>>;

/// Debug login callbacks. Every operation is given together, so the panel never shows a
/// half-working login.
#[derive(Clone)]
pub struct AiDebugAuth {
    pub(crate) login: AsyncFn2,
    pub(crate) logout: AsyncFn0,
    pub(crate) is_logged_in: Rc<dyn Fn() -> AiDebugCallbackResult<bool>>,
    pub(crate) anonymous_login: Option<AsyncFn0>,
}

impl AiDebugAuth {
    /// `login(email, password)`, `logout()` and `is_logged_in()`.
    ///
    /// `is_logged_in` is read inside a reactive computation, so reading the app's auth signals
    /// there refreshes the panel when the user changes.
    pub fn new<L, LF, O, OF>(
        login: L,
        logout: O,
        is_logged_in: impl Fn() -> AiDebugCallbackResult<bool> + 'static,
    ) -> Self
    where
        L: Fn(String, String) -> LF + 'static,
        LF: Future<Output = AiDebugCallbackResult> + 'static,
        O: Fn() -> OF + 'static,
        OF: Future<Output = AiDebugCallbackResult> + 'static,
    {
        Self {
            login: Rc::new(move |email, password| login(email, password).boxed_local()),
            logout: Rc::new(move || logout().boxed_local()),
            is_logged_in: Rc::new(is_logged_in),
            anonymous_login: None,
        }
    }

    /// Adds the anonymous login button.
    pub fn anonymous_login<F, Fut>(mut self, anonymous_login: F) -> Self
    where
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = AiDebugCallbackResult> + 'static,
    {
        self.anonymous_login = Some(Rc::new(move || anonymous_login().boxed_local()));
        self
    }
}

type ProductFn =
    Rc<dyn Fn(AiDebugPurchaseProduct) -> LocalBoxFuture<'static, AiDebugCallbackResult>>;

/// Debug purchase callbacks (Flutter's `Purchase.debugForcePurchase`).
#[derive(Clone)]
pub struct AiDebugPurchase {
    pub(crate) products: Rc<dyn Fn() -> Vec<AiDebugPurchaseProduct>>,
    pub(crate) purchase: ProductFn,
    pub(crate) cancel: ProductFn,
    pub(crate) is_purchased: Rc<dyn Fn(&AiDebugPurchaseProduct) -> bool>,
}

impl AiDebugPurchase {
    pub fn new<P, PF, C, CF>(
        products: impl Fn() -> Vec<AiDebugPurchaseProduct> + 'static,
        purchase: P,
        cancel: C,
        is_purchased: impl Fn(&AiDebugPurchaseProduct) -> bool + 'static,
    ) -> Self
    where
        P: Fn(AiDebugPurchaseProduct) -> PF + 'static,
        PF: Future<Output = AiDebugCallbackResult> + 'static,
        C: Fn(AiDebugPurchaseProduct) -> CF + 'static,
        CF: Future<Output = AiDebugCallbackResult> + 'static,
    {
        Self {
            products: Rc::new(products),
            purchase: Rc::new(move |product| purchase(product).boxed_local()),
            cancel: Rc::new(move |product| cancel(product).boxed_local()),
            is_purchased: Rc::new(is_purchased),
        }
    }

    /// Forces purchases with `masamune-whisker-billing` (`debug_force_purchase`): the product ids
    /// are store product ids.
    #[cfg(feature = "billing")]
    pub fn billing(products: Vec<AiDebugPurchaseProduct>) -> Self {
        use masamune_whisker_billing as billing;
        Self::new(
            move || products.clone(),
            |product: AiDebugPurchaseProduct| async move {
                billing::debug_force_purchase(&product.id);
                Ok(())
            },
            |product: AiDebugPurchaseProduct| async move {
                billing::debug_force_unpurchase(&product.id);
                Ok(())
            },
            |product| billing::debug_is_purchased(&product.id),
        )
    }
}

type PictureFn = Rc<dyn Fn(AiDebugCameraPicture) -> LocalBoxFuture<'static, AiDebugCallbackResult>>;

/// Debug camera callbacks (Flutter's `Camera.setDebugPicture`).
#[derive(Clone)]
pub struct AiDebugCamera {
    pub(crate) pictures: Rc<dyn Fn() -> Vec<AiDebugCameraPicture>>,
    pub(crate) set: PictureFn,
    pub(crate) unset: AsyncFn0,
    pub(crate) is_set: Rc<dyn Fn(&AiDebugCameraPicture) -> bool>,
}

impl AiDebugCamera {
    pub fn new<S, SF, U, UF>(
        pictures: impl Fn() -> Vec<AiDebugCameraPicture> + 'static,
        set: S,
        unset: U,
        is_set: impl Fn(&AiDebugCameraPicture) -> bool + 'static,
    ) -> Self
    where
        S: Fn(AiDebugCameraPicture) -> SF + 'static,
        SF: Future<Output = AiDebugCallbackResult> + 'static,
        U: Fn() -> UF + 'static,
        UF: Future<Output = AiDebugCallbackResult> + 'static,
    {
        Self {
            pictures: Rc::new(pictures),
            set: Rc::new(move |picture| set(picture).boxed_local()),
            unset: Rc::new(move || unset().boxed_local()),
            is_set: Rc::new(is_set),
        }
    }

    /// Sets `masamune-whisker-image-picker`'s debug picture: the camera then returns the image
    /// of the selected picture.
    #[cfg(feature = "image-picker")]
    pub fn image_picker(
        pictures: Vec<(
            AiDebugCameraPicture,
            masamune_whisker_image_picker::PickedImage,
        )>,
    ) -> Self {
        use masamune_whisker_image_picker as picker;
        let images = Rc::new(pictures);
        let list = images.clone();
        let lookup = images.clone();
        let current = images;
        Self::new(
            move || list.iter().map(|(picture, _)| picture.clone()).collect(),
            move |picture: AiDebugCameraPicture| {
                let image = lookup
                    .iter()
                    .find(|(p, _)| *p == picture)
                    .map(|(_, image)| image.clone());
                async move {
                    let image = image.ok_or_else(|| format!("unknown picture: {}", picture.id))?;
                    picker::set_debug_picture(image);
                    Ok(())
                }
            },
            || async {
                picker::unset_debug_picture();
                Ok(())
            },
            move |picture| {
                let active = picker::debug_picture();
                current
                    .iter()
                    .any(|(p, image)| p == picture && Some(image) == active.as_ref())
            },
        )
    }
}

/// Supplies the current page, route and selected state at send time.
pub type AiDebugContextProvider =
    Rc<dyn Fn() -> LocalBoxFuture<'static, Option<AiDebugContextSnapshot>>>;
