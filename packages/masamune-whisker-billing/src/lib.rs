//! In-app subscriptions with one API for Google Play Billing (Android) and StoreKit 2 (iOS).
//!
//! Typical flow:
//!
//! 1. Subscribe to [`on_purchases`] early (purchase results and purchases completed outside the app,
//!    such as approved pending purchases, renewals and refunds on iOS, arrive there).
//! 2. [`connect`], then [`query_products`] to show prices.
//! 3. [`purchase`] opens the store's purchase sheet; the outcome arrives as a [`PurchasesUpdate`].
//! 4. Send [`Purchase::token`] to your server for verification, then [`finish`] the purchase.
//! 5. [`query_purchases`] lists the active subscriptions (for restoring and for retrying verification of
//!    purchases that were never finished).
//!
//! | | Android (Play Billing) | iOS (StoreKit 2) |
//! |---|---|---|
//! | [`Purchase::token`] | `purchaseToken` | `jwsRepresentation` (signed transaction JWS) |
//! | [`Purchase::transaction_id`] | `orderId` | `Transaction.id` |
//! | [`Purchase::original_id`] | `orderId` without the `..N` renewal suffix | `Transaction.originalID` |
//! | [`finish`] | `acknowledgePurchase` | `Transaction.finish()` |
//! | [`query_purchases`] | `queryPurchasesAsync(SUBS)` | `Transaction.currentEntitlements` |
//! | [`on_purchases`] | `PurchasesUpdatedListener` | `Transaction.updates` and results of [`purchase`] |
//!
//! Unfinished purchases are refunded by Google Play after three days; on iOS they are delivered again
//! through [`on_purchases`] on every launch until finished.
//!
//! In development, [`debug_force_purchase`] / [`debug_force_unpurchase`] simulate a purchase without
//! the store (used by `masamune-whisker-ai-debugger`). Forced purchases carry
//! [`DEBUG_PURCHASE_TOKEN`] as their token.

mod debug;

pub use debug::{
    DEBUG_PURCHASE_TOKEN, debug_force_purchase, debug_force_unpurchase, debug_is_purchased,
    debug_purchases,
};
use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Billing")
}

/// Store a purchase or product comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    /// Google Play.
    Android,
    /// App Store.
    Ios,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Outcome of a store operation, shared by both platforms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Succeeded (for a purchase: completed or pending, see [`Purchase::state`]).
    Ok,
    /// The user closed the purchase sheet.
    UserCanceled,
    /// The user already owns the product (Android `ITEM_ALREADY_OWNED`).
    AlreadyOwned,
    /// The user does not own the product (Android `ITEM_NOT_OWNED`).
    NotOwned,
    /// The product cannot be bought (unknown id, not sold in the storefront, ineligible offer).
    ItemUnavailable,
    /// Purchases are not possible on this device or account (Android `BILLING_UNAVAILABLE`, iOS purchases
    /// disabled by Screen Time).
    BillingUnavailable,
    /// The store service is temporarily unreachable.
    ServiceUnavailable,
    /// The connection to the store service was lost (Android `SERVICE_DISCONNECTED`).
    ServiceDisconnected,
    /// A network error occurred.
    NetworkError,
    /// The request is invalid (bad arguments, invalid offer).
    DeveloperError,
    /// The feature is not supported on this device.
    NotSupported,
    /// Any other error.
    Error,
    #[default]
    #[serde(other)]
    Unknown,
}

/// Result of [`purchase`]: whether the purchase sheet could be shown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingResult {
    #[serde(default)]
    pub status: Status,
    /// Platform code (Android `BillingResponseCode`, iOS `NSError` code; `0` on success).
    #[serde(default)]
    pub code: i64,
    #[serde(default)]
    pub message: String,
}

impl BillingResult {
    pub fn is_ok(&self) -> bool {
        self.status == Status::Ok
    }
}

/// How a pricing phase repeats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recurrence {
    /// Repeats until the subscription is canceled (the regular price).
    Infinite,
    /// Repeats [`PricingPhase::cycles`] times (an introductory price or a free trial).
    Finite,
    /// Charged once.
    NonRecurring,
    #[default]
    #[serde(other)]
    Unknown,
}

/// One price period of an offer (for example "free for 1 week", then "¥500 / month").
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingPhase {
    /// Localized price (for example `"¥500"`, or the store's "free" text for a free trial).
    #[serde(default)]
    pub price: String,
    /// Price in micro units (`1_000_000` = 1 currency unit). `0` for a free trial.
    #[serde(default)]
    pub price_micros: i64,
    /// ISO 4217 currency code.
    #[serde(default)]
    pub currency: String,
    /// Length of one billing period in ISO 8601 (`"P1W"`, `"P1M"`, `"P1Y"` …).
    #[serde(default)]
    pub period: String,
    /// Number of periods for [`Recurrence::Finite`] phases (`0` otherwise).
    #[serde(default)]
    pub cycles: i64,
    #[serde(default)]
    pub recurrence: Recurrence,
}

impl PricingPhase {
    pub fn is_free(&self) -> bool {
        self.price_micros == 0
    }
}

/// Kind of an [`Offer`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferKind {
    /// The regular price (Android base plan without an offer; on iOS the product price).
    Base,
    /// A developer-defined Play offer on top of a base plan (Android only), identified by [`Offer::id`].
    Developer,
    /// The App Store introductory offer (iOS only). StoreKit applies it automatically to eligible users.
    Introductory,
    #[default]
    #[serde(other)]
    Unknown,
}

/// A way to buy a subscription.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    #[serde(default)]
    pub kind: OfferKind,
    /// Play offer id (`None` for base plans and on iOS).
    #[serde(default)]
    pub id: Option<String>,
    /// Play base plan id (`None` on iOS).
    #[serde(default)]
    pub base_plan_id: Option<String>,
    /// Play offer token passed to [`purchase`] (empty on iOS).
    #[serde(default)]
    pub token: String,
    /// Whether the offer starts with a free phase.
    #[serde(default)]
    pub free_trial: bool,
    /// Price phases in order; the last one is the regular price.
    #[serde(default)]
    pub phases: Vec<PricingPhase>,
}

/// A subscription product.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: String,
    /// Display name (Android `ProductDetails.name`, iOS `displayName`).
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Localized regular price.
    #[serde(default)]
    pub price: String,
    /// Regular price in micro units.
    #[serde(default)]
    pub price_micros: i64,
    /// ISO 4217 currency code.
    #[serde(default)]
    pub currency: String,
    /// Subscription period in ISO 8601 (`None` for non-subscription products).
    #[serde(default)]
    pub period: Option<String>,
    /// Android: every base plan and offer. iOS: the base price and, when configured, the introductory offer.
    #[serde(default)]
    pub offers: Vec<Offer>,
}

impl Product {
    /// The regular-price offer.
    pub fn base_offer(&self) -> Option<&Offer> {
        self.offers.iter().find(|o| o.kind == OfferKind::Base)
    }

    /// The Play offer with the given id (Android only).
    pub fn offer(&self, id: &str) -> Option<&Offer> {
        self.offers.iter().find(|o| o.id.as_deref() == Some(id))
    }
}

/// State of a purchase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseState {
    /// Paid; verify and [`finish`] it.
    Purchased,
    /// Waiting for payment or approval (Android pending purchases, iOS Ask to Buy / SCA). The completed
    /// purchase arrives later through [`on_purchases`]. On iOS pending purchases only carry the product id.
    Pending,
    /// Refunded or revoked (iOS `revocationDate`).
    Revoked,
    #[default]
    #[serde(other)]
    Unspecified,
}

/// A purchase (transaction).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    #[serde(default)]
    pub product_id: String,
    /// Android `orderId` (empty while pending) / iOS transaction id.
    #[serde(default)]
    pub transaction_id: String,
    /// Id shared by every renewal of the subscription: Android `orderId` without the `..N` renewal suffix
    /// (the purchase token when there is no order id) / iOS `originalID`.
    #[serde(default)]
    pub original_id: String,
    /// What the server verifies: Android purchase token / iOS StoreKit 2 signed transaction (JWS).
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub platform: Platform,
    #[serde(default)]
    pub state: PurchaseState,
    /// Android: acknowledged. iOS: the transaction has been finished.
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub auto_renewing: bool,
    /// Purchase (or latest renewal) time in Unix milliseconds.
    #[serde(default)]
    pub purchase_time: i64,
    /// Expiry in Unix milliseconds when the store reports it (iOS only).
    #[serde(default)]
    pub expires_time: Option<i64>,
}

impl Purchase {
    pub fn is_purchased(&self) -> bool {
        self.state == PurchaseState::Purchased
    }
}

/// Purchase notification delivered to [`on_purchases`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchasesUpdate {
    #[serde(default)]
    pub status: Status,
    /// Platform code (Android `BillingResponseCode`, iOS `NSError` code; `0` on success).
    #[serde(default)]
    pub code: i64,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub purchases: Vec<Purchase>,
}

/// Connects to the store. Called implicitly by the other functions, but calling it at startup lets
/// iOS start listening to `Transaction.updates` early. Fails on Android when Play Billing is unavailable.
pub async fn connect() -> NativeResult<bool> {
    module().call_async("connect", &json!({})).await
}

/// Looks up subscription products. Unknown ids are left out.
pub async fn query_products(ids: &[&str]) -> NativeResult<Vec<Product>> {
    module()
        .call_async("queryProducts", &json!({ "ids": ids }))
        .await
}

/// Shows the purchase sheet. Returns whether it could be shown; the outcome (purchased, pending, canceled
/// or failed) is delivered to [`on_purchases`] on both platforms, so subscribe before calling this.
///
/// - `offer`: the [`Offer`] to buy. On Android its token selects the base plan or offer (the first offer
///   when `None`). On iOS it is ignored: StoreKit applies the introductory offer to eligible users itself.
/// - `replacing`: the current subscription when switching plans. On Android it is replaced with time
///   proration. On iOS it is ignored because StoreKit upgrades, downgrades and crossgrades within a
///   subscription group automatically (put the products in the same group in App Store Connect).
pub async fn purchase(
    product_id: &str,
    offer: Option<&Offer>,
    replacing: Option<&Purchase>,
) -> NativeResult<BillingResult> {
    module()
        .call_async(
            "purchase",
            &json!({
                "productId": product_id,
                "offerToken": offer.map(|o| o.token.as_str()).unwrap_or(""),
                "oldPurchaseToken": replacing.map(|p| p.token.as_str()).unwrap_or(""),
            }),
        )
        .await
}

/// Lists the purchases the user currently owns: active subscriptions (Android `queryPurchasesAsync`
/// for subscriptions, iOS `Transaction.currentEntitlements`, which excludes expired and revoked ones).
///
/// Forced purchases ([`debug_force_purchase`]) are included, and are returned alone when the store
/// cannot be queried (for example on a simulator).
pub async fn query_purchases() -> NativeResult<Vec<Purchase>> {
    match module().call_async("queryPurchases", &json!({})).await {
        Ok(purchases) => Ok(debug::merge(purchases)),
        Err(_) if !debug_purchases().is_empty() => Ok(debug_purchases()),
        Err(error) => Err(error),
    }
}

/// Marks a purchase as delivered after the server verified it (Android acknowledge, iOS
/// `Transaction.finish()`). Does nothing when it is already finished.
pub async fn finish(purchase: &Purchase) -> NativeResult<()> {
    if purchase.finished {
        return Ok(());
    }
    let _: serde_json::Value = module()
        .call_async(
            "finish",
            &json!({ "token": purchase.token, "transactionId": purchase.transaction_id }),
        )
        .await?;
    Ok(())
}

/// Receives purchase results and purchases that change outside [`purchase`], plus forced purchases
/// ([`debug_force_purchase`] / [`debug_force_unpurchase`]).
pub fn on_purchases(handler: impl Fn(PurchasesUpdate) + 'static) -> Subscription {
    let handler: std::rc::Rc<dyn Fn(PurchasesUpdate)> = std::rc::Rc::new(handler);
    let native = module().subscribe("purchases", {
        let handler = handler.clone();
        move |update| handler(update)
    });
    Subscription::new((native, debug::listen(handler)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_purchase_and_unknown_values() {
        let purchase: Purchase = serde_json::from_value(json!({
            "productId": "monthly",
            "transactionId": "GPA.1..0",
            "originalId": "GPA.1",
            "token": "t",
            "platform": "android",
            "state": "purchased",
            "finished": false,
            "autoRenewing": true,
            "purchaseTime": 1,
            "expiresTime": null,
        }))
        .unwrap();
        assert!(purchase.is_purchased());
        assert_eq!(purchase.platform, Platform::Android);
        let update: PurchasesUpdate =
            serde_json::from_value(json!({ "status": "something_new", "purchases": [{ "state": "later", "platform": "web" }] }))
                .unwrap();
        assert_eq!(update.status, Status::Unknown);
        assert_eq!(update.purchases[0].state, PurchaseState::Unspecified);
        assert_eq!(update.purchases[0].platform, Platform::Unknown);
    }

    #[test]
    fn finds_offers() {
        let product: Product = serde_json::from_value(json!({
            "id": "monthly",
            "offers": [
                { "kind": "base", "token": "a", "phases": [{ "price": "¥500", "priceMicros": 500000000, "recurrence": "infinite" }] },
                { "kind": "developer", "id": "trial", "token": "b", "freeTrial": true },
            ],
        }))
        .unwrap();
        assert_eq!(product.base_offer().map(|o| o.token.as_str()), Some("a"));
        assert_eq!(product.offer("trial").map(|o| o.token.as_str()), Some("b"));
        assert!(!product.offers[0].phases[0].is_free());
    }
}
