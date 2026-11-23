//! Network interface addresses of this device and mDNS (DNS-SD) service discovery on the local
//! network.
//!
//! - [`interfaces`]: the IP addresses of the active interfaces (Wi-Fi, cellular, VPN such as
//!   Tailscale's `tun0`).
//! - [`discover_services`]: browses a DNS-SD service type (for example `_http._tcp`) for a fixed
//!   time and resolves what it finds. Android only for now; iOS returns an error.
//!
//! Android needs the `android.permission.INTERNET` permission (declared by most apps already).

use std::time::Duration;

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Network")
}

/// An active network interface and its addresses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    /// Interface name (`wlan0`, `rmnet0`, `tun0`, `en0`, `utun3`, ...).
    #[serde(default)]
    pub name: String,
    /// IPv4 addresses in dotted form.
    #[serde(default)]
    pub ipv4: Vec<String>,
    /// IPv6 addresses without the zone suffix (`%wlan0`). Link-local addresses are left out.
    #[serde(default)]
    pub ipv6: Vec<String>,
    /// The interface is point-to-point, which is how VPNs (Tailscale, WireGuard) show up.
    #[serde(default)]
    pub point_to_point: bool,
}

/// The interfaces that are up and have an address, loopback excluded.
pub fn interfaces() -> NativeResult<Vec<NetworkInterface>> {
    module().call("interfaces", &json!({}))
}

/// A DNS-SD service found by [`discover_services`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredService {
    /// Service instance name (`adb-R5CX1234-AbCdEf`).
    #[serde(default)]
    pub name: String,
    /// Service type as reported by the system (`_adb-tls-connect._tcp.`).
    #[serde(default)]
    pub service_type: String,
    /// Resolved addresses of the host.
    #[serde(default)]
    pub addresses: Vec<String>,
    /// Resolved port.
    #[serde(default)]
    pub port: u16,
}

/// Browses `service_type` (for example `_http._tcp`) for `timeout` and returns the services that
/// could be resolved. Services this device advertises itself are included when the system reports
/// them.
///
/// - Android: `NsdManager` (`discoverServices` and `resolveService`, one resolve at a time).
/// - iOS: not supported yet (browsing needs `NSBonjourServices` and the local network permission);
///   returns an error.
pub async fn discover_services(
    service_type: &str,
    timeout: Duration,
) -> NativeResult<Vec<DiscoveredService>> {
    module()
        .call_async(
            "discoverServices",
            &json!({
                "serviceType": service_type,
                "timeoutMs": timeout.as_millis() as u64,
            }),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_interfaces_and_services() {
        let interfaces: Vec<NetworkInterface> = serde_json::from_value(json!([
            { "name": "tun0", "ipv4": ["100.64.0.12"], "ipv6": [], "pointToPoint": true },
            { "name": "wlan0", "ipv4": ["192.168.1.5"] }
        ]))
        .unwrap();
        assert_eq!(interfaces[0].ipv4, vec!["100.64.0.12"]);
        assert!(interfaces[0].point_to_point);
        assert!(interfaces[1].ipv6.is_empty());

        let services: Vec<DiscoveredService> = serde_json::from_value(json!([
            { "name": "adb-x", "serviceType": "_adb-tls-connect._tcp.", "addresses": ["192.168.1.5"], "port": 41845 }
        ]))
        .unwrap();
        assert_eq!(services[0].port, 41845);
    }
}
