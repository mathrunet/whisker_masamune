#![cfg(not(debug_assertions))]
use masamune_whisker_billing::{
    debug_force_purchase, debug_force_unpurchase, debug_is_purchased, debug_purchases,
};

#[test]
fn release_cannot_create_entitlements() {
    assert!(!debug_force_purchase("subscription").is_purchased());
    assert!(!debug_is_purchased("subscription"));
    assert!(debug_purchases().is_empty());
    assert!(!debug_force_unpurchase("subscription"));
}
