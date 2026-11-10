//! アプリ内課金（Google Play Billing。定期購読の商品取得・購入・購入履歴・承認）。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult, Subscription};

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Billing")
}

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingPhase {
    #[serde(default)]
    pub formatted_price: String,
    #[serde(default)]
    pub price_amount_micros: i64,
    #[serde(default)]
    pub price_currency_code: String,
    #[serde(default)]
    pub billing_period: String,
    #[serde(default)]
    pub billing_cycle_count: i64,
    #[serde(default)]
    pub recurrence_mode: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    #[serde(default)]
    pub offer_token: String,
    #[serde(default)]
    pub base_plan_id: String,
    #[serde(default)]
    pub offer_id: Option<String>,
    #[serde(default)]
    pub phases: Vec<PricingPhase>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetails {
    pub product_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub offers: Vec<Offer>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Purchase {
    #[serde(default)]
    pub product_ids: Vec<String>,
    #[serde(default)]
    pub purchase_token: String,
    #[serde(default)]
    pub order_id: Option<String>,
    /// 0: 不明, 1: 購入済み, 2: 保留中。
    #[serde(default)]
    pub purchase_state: i64,
    #[serde(default)]
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchasesUpdate {
    #[serde(default)]
    pub response_code: i64,
    #[serde(default)]
    pub debug_message: String,
    #[serde(default)]
    pub purchases: Vec<Purchase>,
}

/// BillingClient のレスポンスコード。
pub const OK: i64 = 0;
pub const USER_CANCELED: i64 = 1;
pub const ITEM_ALREADY_OWNED: i64 = 7;

pub async fn connect() -> NativeResult<bool> {
    module().call_async("connect", &json!({})).await
}

pub async fn query_products(ids: &[&str]) -> NativeResult<Vec<ProductDetails>> {
    module()
        .call_async("queryProducts", &json!({ "ids": ids }))
        .await
}

/// 購入画面を出す。結果は `on_purchases` に届く。戻り値は起動結果のレスポンスコード。
pub async fn purchase(
    product_id: &str,
    offer_token: Option<&str>,
    old_purchase_token: Option<&str>,
) -> NativeResult<i64> {
    let result: Value = module()
        .call_async(
        "purchase",
        &json!({ "productId": product_id, "offerToken": offer_token.unwrap_or(""), "oldPurchaseToken": old_purchase_token.unwrap_or("") }),
    )
    .await?;
    Ok(result
        .get("responseCode")
        .and_then(Value::as_i64)
        .unwrap_or(-1))
}

pub async fn query_purchases() -> NativeResult<Vec<Purchase>> {
    module().call_async("queryPurchases", &json!({})).await
}

pub async fn acknowledge(purchase_token: &str) -> NativeResult<i64> {
    module()
        .call_async("acknowledge", &json!({ "purchaseToken": purchase_token }))
        .await
}

/// 購入結果の通知を受け取る。
pub fn on_purchases(handler: impl Fn(PurchasesUpdate) + 'static) -> Subscription {
    module().subscribe("purchases", handler)
}
