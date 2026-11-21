#![doc = include_str!("../README.md")]

use masamune_whisker_bridge::NativeModule;
pub use masamune_whisker_bridge::{NativeError, NativeResult};
use serde::Deserialize;
use serde_json::json;

fn module() -> NativeModule {
    NativeModule::new(env!("CARGO_PKG_NAME"), "Geocoding")
}

/// An address for a coordinate (the counterpart of `Placemark` of Flutter's `geocoding`).
///
/// Fields the geocoder does not know are `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Placemark {
    /// Name of the place (a landmark or the street address).
    pub name: Option<String>,
    /// Street (iOS `thoroughfare`, Android `thoroughfare`).
    pub street: Option<String>,
    /// House number (`subThoroughfare`).
    pub sub_thoroughfare: Option<String>,
    /// District within a city (`subLocality`).
    pub sub_locality: Option<String>,
    /// City (`locality`).
    pub locality: Option<String>,
    /// County (`subAdministrativeArea`).
    pub sub_administrative_area: Option<String>,
    /// State or prefecture (`administrativeArea`).
    pub administrative_area: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
    /// ISO 3166-1 alpha-2 country code (`JP`).
    pub iso_country_code: Option<String>,
}

/// Whether the platform has a geocoder (Android devices without a geocoding backend have none;
/// always `true` on iOS).
pub fn is_available() -> bool {
    module()
        .call::<_, bool>("isAvailable", &json!({}))
        .unwrap_or(false)
}

/// Looks up addresses for a coordinate with the platform geocoder (iOS `CLGeocoder`, Android
/// `android.location.Geocoder`), best match first.
///
/// `locale` is a BCP 47 / ICU identifier (`"ja"`, `"en_US"`, `"zh-Hans"`) for the language of the
/// names; `None` uses the device language. The platform geocoders use network services with rate
/// limits (on iOS, roughly one request per minute per app is sustained), so cache the results.
/// Returns an empty list when nothing is found.
pub async fn reverse_geocode(
    latitude: f64,
    longitude: f64,
    locale: Option<&str>,
) -> NativeResult<Vec<Placemark>> {
    if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
        return Err(NativeError("coordinate out of range".into()));
    }
    module()
        .call_async(
            "reverseGeocode",
            &json!({ "latitude": latitude, "longitude": longitude, "locale": locale.unwrap_or("") }),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_placemarks() {
        let placemarks: Vec<Placemark> = serde_json::from_str(
            r#"[{"name":"Tokyo Station","locality":"Chiyoda","administrativeArea":"Tokyo","isoCountryCode":"JP","street":null}]"#,
        )
        .unwrap();
        assert_eq!(placemarks[0].locality.as_deref(), Some("Chiyoda"));
        assert_eq!(placemarks[0].iso_country_code.as_deref(), Some("JP"));
        assert_eq!(placemarks[0].street, None);
        assert_eq!(placemarks[0].postal_code, None);
    }

    #[test]
    fn rejects_out_of_range_coordinates() {
        let result = block_on(reverse_geocode(91.0, 0.0, None));
        assert_eq!(result, Err(NativeError("coordinate out of range".into())));
        let result = block_on(reverse_geocode(f64::NAN, 0.0, None));
        assert!(result.is_err());
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("validation should resolve immediately"),
        }
    }
}
