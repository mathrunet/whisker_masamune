//! AdMob（アダプティブバナーと全画面広告）。
//!
//! App ID はアプリのマニフェスト（`com.google.android.gms.ads.APPLICATION_ID`）に設定する。

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde_json::json;
use whisker::Signal;
use whisker::event::CustomEvent;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Admob")
}

/// 画面幅に合わせたアンカー型アダプティブバナーの高さ（dp）。
pub fn banner_height(width_dp: i32) -> NativeResult<i64> {
    module().call("bannerHeight", &json!({ "widthDp": width_dp }))
}

/// 全画面広告を読み込んで表示し、閉じたら返る。表示できたら `true`（在庫なしは `false`）。
pub async fn show_interstitial(ad_unit_id: &str) -> NativeResult<bool> {
    module()
        .call_async("showInterstitial", &json!({ "adUnitId": ad_unit_id }))
        .await
}

/// バナー広告の要素。`width_dp` の幅で読み込み、`loaded` か `failed`（`{message}`）を送る。
#[doc(hidden)]
// whisker 0.16 の module_element は属性を文字列で渡すため、幅も文字列で受け渡す。
#[whisker::module_element(name = "masamune-whisker-admob:Banner", measurement = None)]
pub fn ad_banner(
    ad_unit_id: Signal<String>,
    width_dp: Signal<String>,
    style: Style,
    on_loaded: CustomEvent,
    on_failed: CustomEvent,
) {
}

/// 生成される Host 起動処理（Web・desktop）から参照される要素スキーマ。
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [ad_banner_schema::element_provider()],
    )
}
