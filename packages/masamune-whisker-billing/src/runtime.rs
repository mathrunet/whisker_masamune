use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    BillingResult, NativeError, NativeResult, Offer, Platform, Product, ProductKind, Purchase,
    PurchaseState, PurchasesUpdate, Status, Subscription,
};

type PurchaseListener = Rc<dyn Fn(PurchasesUpdate)>;

#[derive(Default)]
struct RuntimeState {
    products: Vec<Product>,
    purchases: Vec<Purchase>,
    consumed_tokens: Vec<String>,
    finished_ids: Vec<String>,
    next_id: u64,
    listeners: Vec<(u64, PurchaseListener)>,
}

/// In-memory billing adapter for `test` flavor and unit tests (no store SDK or network calls).
///
/// Clones share the same state.
#[derive(Clone, Default)]
pub struct RuntimeBillingAdapter {
    state: Rc<RefCell<RuntimeState>>,
}

impl RuntimeBillingAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds catalog products returned by [`Self::query_products`].
    pub fn with_products(self, products: impl IntoIterator<Item = Product>) -> Self {
        self.state.borrow_mut().products = products.into_iter().collect();
        self
    }

    /// Seeds existing owned or unfinished purchases returned by [`Self::query_purchases`].
    pub fn with_purchases(self, purchases: impl IntoIterator<Item = Purchase>) -> Self {
        self.state.borrow_mut().purchases = purchases.into_iter().collect();
        self
    }

    /// Looks up seeded products matching `ids` (and optional `kind` filter).
    pub fn query_products(&self, ids: &[&str], kind: Option<ProductKind>) -> Vec<Product> {
        let state = self.state.borrow();
        state
            .products
            .iter()
            .filter(|product| {
                ids.contains(&product.id.as_str())
                    && kind.is_none_or(|expected| product.kind == expected)
            })
            .cloned()
            .collect()
    }

    /// Simulates a store purchase of `product_id` and emits a [`PurchasesUpdate`] to listeners.
    pub fn purchase(
        &self,
        product_id: &str,
        _offer: Option<&Offer>,
        _replacing: Option<&Purchase>,
        _account_token: Option<&str>,
    ) -> NativeResult<BillingResult> {
        let (purchase, listeners) = {
            let mut state = self.state.borrow_mut();
            let product = state
                .products
                .iter()
                .find(|p| p.id == product_id)
                .cloned()
                .ok_or_else(|| NativeError(format!("product not found: {product_id}")))?;
            state.next_id += 1;
            let tx_id = format!("runtime-{}-{}", product.id, state.next_id);
            let purchase = Purchase {
                product_id: product.id,
                transaction_id: tx_id.clone(),
                original_id: tx_id,
                token: format!("runtime-token-{}", state.next_id),
                platform: Platform::Ios,
                state: PurchaseState::Purchased,
                finished: false,
                auto_renewing: product.kind == ProductKind::Subscription,
                purchase_time: 1,
                expires_time: None,
            };
            state.purchases.push(purchase.clone());
            let listeners: Vec<PurchaseListener> =
                state.listeners.iter().map(|(_, cb)| cb.clone()).collect();
            (purchase, listeners)
        };
        for listener in listeners {
            listener(PurchasesUpdate {
                status: Status::Ok,
                code: 0,
                message: String::new(),
                purchases: vec![purchase.clone()],
            });
        }
        Ok(BillingResult {
            status: Status::Ok,
            code: 0,
            message: String::new(),
        })
    }

    /// Returns all currently held runtime purchases.
    pub fn query_purchases(&self) -> Vec<Purchase> {
        self.state.borrow().purchases.clone()
    }

    /// Marks `purchase` as finished in memory.
    pub fn finish(&self, purchase: &Purchase) {
        let mut state = self.state.borrow_mut();
        for existing in &mut state.purchases {
            if existing.transaction_id == purchase.transaction_id {
                existing.finished = true;
            }
        }
        state.finished_ids.push(purchase.transaction_id.clone());
    }

    /// Consumes `purchase` in memory (marks finished and removes from active in-app purchases).
    pub fn consume(&self, purchase: &Purchase) {
        let mut state = self.state.borrow_mut();
        state
            .purchases
            .retain(|existing| existing.transaction_id != purchase.transaction_id);
        state.consumed_tokens.push(purchase.token.clone());
    }

    /// Tokens passed to [`Self::consume`].
    pub fn consumed_tokens(&self) -> Vec<String> {
        self.state.borrow().consumed_tokens.clone()
    }

    /// Subscribes to simulated purchase updates.
    pub fn on_purchases(&self, handler: impl Fn(PurchasesUpdate) + 'static) -> Subscription {
        let mut state = self.state.borrow_mut();
        state.next_id += 1;
        let id = state.next_id;
        state.listeners.push((id, Rc::new(handler)));
        let shared = self.state.clone();
        Subscription::new(RuntimeListenerGuard { state: shared, id })
    }
}

struct RuntimeListenerGuard {
    state: Rc<RefCell<RuntimeState>>,
    id: u64,
}

impl Drop for RuntimeListenerGuard {
    fn drop(&mut self) {
        self.state
            .borrow_mut()
            .listeners
            .retain(|(id, _)| *id != self.id);
    }
}
