//! Forced purchases for development builds (Flutter's `Purchase.debugForcePurchase`).
//!
//! A forced purchase never reaches the store. It is delivered to every [`crate::on_purchases`]
//! handler as a finished, purchased transaction and is listed by [`crate::query_purchases`] until
//! [`debug_force_unpurchase`] removes it. Forced purchases live in memory only, so they disappear
//! when the app restarts. Call these only from development tooling such as
//! `masamune-whisker-ai-debugger`; the server cannot verify the fake `token`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Platform, Purchase, PurchaseState, PurchasesUpdate, Status};

/// Token carried by every forced purchase, so servers and apps can reject it.
pub const DEBUG_PURCHASE_TOKEN: &str = "masamune-whisker-billing-debug";

type Listener = Rc<dyn Fn(PurchasesUpdate)>;

thread_local! {
    static FORCED: RefCell<Vec<Purchase>> = const { RefCell::new(Vec::new()) };
    static LISTENERS: RefCell<Vec<(u64, Listener)>> = const { RefCell::new(Vec::new()) };
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

/// Marks `product_id` as purchased and notifies [`crate::on_purchases`] handlers. Replaces an
/// earlier forced purchase of the same product.
pub fn debug_force_purchase(product_id: &str) -> Purchase {
    let id = NEXT_ID.with(|next| next.replace(next.get() + 1));
    let transaction_id = format!("debug-{product_id}-{id}");
    let purchase = Purchase {
        product_id: product_id.to_owned(),
        original_id: transaction_id.clone(),
        transaction_id,
        token: DEBUG_PURCHASE_TOKEN.to_owned(),
        platform: current_platform(),
        state: PurchaseState::Purchased,
        finished: true,
        auto_renewing: true,
        purchase_time: now_ms(),
        expires_time: None,
    };
    FORCED.with(|forced| {
        let mut forced = forced.borrow_mut();
        forced.retain(|p| p.product_id != product_id);
        forced.push(purchase.clone());
    });
    notify(purchase.clone());
    purchase
}

/// Removes the forced purchase of `product_id`. Handlers receive it again with
/// [`PurchaseState::Revoked`]. Returns false when it was not forced.
pub fn debug_force_unpurchase(product_id: &str) -> bool {
    let removed = FORCED.with(|forced| {
        let mut forced = forced.borrow_mut();
        let index = forced.iter().position(|p| p.product_id == product_id)?;
        Some(forced.remove(index))
    });
    match removed {
        Some(mut purchase) => {
            purchase.state = PurchaseState::Revoked;
            notify(purchase);
            true
        }
        None => false,
    }
}

/// The forced purchases currently active.
pub fn debug_purchases() -> Vec<Purchase> {
    FORCED.with(|forced| forced.borrow().clone())
}

/// Whether `product_id` has a forced purchase.
pub fn debug_is_purchased(product_id: &str) -> bool {
    FORCED.with(|forced| forced.borrow().iter().any(|p| p.product_id == product_id))
}

/// Native purchases with the forced ones added (a forced purchase replaces the store's purchase of
/// the same product).
pub(crate) fn merge(mut purchases: Vec<Purchase>) -> Vec<Purchase> {
    let forced = debug_purchases();
    purchases.retain(|p| !forced.iter().any(|f| f.product_id == p.product_id));
    purchases.extend(forced);
    purchases
}

pub(crate) struct ListenerGuard(u64);

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        let id = self.0;
        // Ignore drops during thread teardown.
        let _ = LISTENERS.try_with(|listeners| listeners.borrow_mut().retain(|(i, _)| *i != id));
    }
}

pub(crate) fn listen(handler: Listener) -> ListenerGuard {
    let id = NEXT_ID.with(|next| next.replace(next.get() + 1));
    LISTENERS.with(|listeners| listeners.borrow_mut().push((id, handler)));
    ListenerGuard(id)
}

fn notify(purchase: Purchase) {
    // Snapshot first so a handler can subscribe or unsubscribe while being called.
    let listeners: Vec<Listener> =
        LISTENERS.with(|listeners| listeners.borrow().iter().map(|(_, l)| l.clone()).collect());
    for listener in listeners {
        listener(PurchasesUpdate {
            status: Status::Ok,
            code: 0,
            message: String::new(),
            purchases: vec![purchase.clone()],
        });
    }
}

fn current_platform() -> Platform {
    if cfg!(target_os = "android") {
        Platform::Android
    } else if cfg!(target_os = "ios") {
        Platform::Ios
    } else {
        Platform::Unknown
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forces_and_removes_purchases() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let guard = listen({
            let seen = seen.clone();
            Rc::new(move |update: PurchasesUpdate| seen.borrow_mut().push(update))
        });
        let first = debug_force_purchase("monthly");
        assert!(first.is_purchased() && first.finished);
        assert_eq!(first.token, DEBUG_PURCHASE_TOKEN);
        debug_force_purchase("monthly");
        assert_eq!(debug_purchases().len(), 1);
        assert!(debug_is_purchased("monthly"));

        let store = Purchase {
            product_id: "monthly".into(),
            token: "store".into(),
            ..Default::default()
        };
        let other = Purchase {
            product_id: "yearly".into(),
            ..Default::default()
        };
        let merged = merge(vec![store, other]);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|p| p.token == DEBUG_PURCHASE_TOKEN));

        assert!(debug_force_unpurchase("monthly"));
        assert!(!debug_force_unpurchase("monthly"));
        assert!(!debug_is_purchased("monthly"));
        let states: Vec<PurchaseState> =
            seen.borrow().iter().map(|u| u.purchases[0].state).collect();
        assert_eq!(
            states,
            [
                PurchaseState::Purchased,
                PurchaseState::Purchased,
                PurchaseState::Revoked
            ]
        );

        drop(guard);
        debug_force_purchase("yearly");
        assert_eq!(seen.borrow().len(), 3);
    }
}
