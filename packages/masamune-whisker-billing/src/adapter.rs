use masamune_whisker_adapter::{AdapterError, MasamuneAdapter, StartFuture};
use whisker::prelude::*;

use crate::{
    NativeError, NativeResult, Product, StoreFailure, query_inapp_products, query_products,
};

/// Billing settings for [`masamune_whisker_adapter::MasamuneAdapters`] (Masamune's
/// `PurchaseMasamuneAdapter`): the app's products, loaded after startup into [`products`].
///
/// It does not connect, subscribe to [`crate::on_purchases`] or verify purchases: subscribe before
/// [`crate::connect`] so iOS delivers the unfinished transactions to the handler.
#[derive(Debug, Clone)]
pub struct BillingMasamuneAdapter {
    /// Subscription product ids, in display order.
    pub product_ids: Vec<String>,
    /// One-time / consumable product ids, in display order.
    pub consumable_ids: Vec<String>,
    /// Load the products after startup.
    pub load_on_start: bool,
}

impl BillingMasamuneAdapter {
    pub fn new(product_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            product_ids: product_ids.into_iter().map(Into::into).collect(),
            consumable_ids: Vec::new(),
            load_on_start: true,
        }
    }

    /// Adds one-time / consumable product ids loaded alongside subscriptions.
    pub fn consumables(mut self, ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.consumable_ids = ids.into_iter().map(Into::into).collect();
        self
    }

    /// Controls whether products are loaded automatically after startup.
    pub fn load_on_start(mut self, enabled: bool) -> Self {
        self.load_on_start = enabled;
        self
    }
}

impl MasamuneAdapter for BillingMasamuneAdapter {
    fn start(&self) -> Option<StartFuture> {
        if !self.load_on_start {
            return None;
        }
        Some(Box::pin(async {
            if let Err(error) = load_products().await {
                if let Some(failure) =
                    StoreFailure::from_native(&error).filter(StoreFailure::is_availability_failure)
                {
                    masamune_whisker_logger::info!("billing products unavailable: {failure}");
                } else {
                    return Err(error.into());
                }
            }
            Ok::<(), AdapterError>(())
        }))
    }
}

whisker::runtime_local! {
    static PRODUCTS: RwSignal<Vec<Product>> = signal(Vec::new());
}

/// The products of the installed [`BillingMasamuneAdapter`] found in the store, in its order.
/// Empty until [`load_products`] finishes.
pub fn products() -> ReadSignal<Vec<Product>> {
    PRODUCTS.with(|products| products.read_only())
}

/// Looks up the products of the installed [`BillingMasamuneAdapter`] and stores them in
/// [`products`]. Unknown ids are left out.
pub async fn load_products() -> NativeResult<Vec<Product>> {
    let adapter = BillingMasamuneAdapter::primary()
        .ok_or_else(|| NativeError("BillingMasamuneAdapter is not installed".into()))?;
    let sub_ids: Vec<&str> = adapter.product_ids.iter().map(String::as_str).collect();
    let inapp_ids: Vec<&str> = adapter.consumable_ids.iter().map(String::as_str).collect();
    let mut found = if sub_ids.is_empty() {
        Vec::new()
    } else {
        query_products(&sub_ids).await?
    };
    if !inapp_ids.is_empty() {
        found.extend(query_inapp_products(&inapp_ids).await?);
    }
    let order: Vec<&str> = sub_ids.iter().chain(inapp_ids.iter()).copied().collect();
    found.sort_by_key(|product| order.iter().position(|id| *id == product.id));
    PRODUCTS.with(|products| products.set(found.clone()));
    Ok(found)
}
