// Reverse geocoding (iOS, CLGeocoder). Each request uses its own geocoder, so concurrent lookups
// do not cancel each other.

import CoreLocation
import Foundation
import WhiskerModule

@WhiskerModule
public final class GeocodingModule: Module {
    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Geocoding")

            Function("isAvailable") { _ in
                Bridge.ok(true)
            }

            // reverseGeocode({ latitude, longitude, locale }) -> [placemark]
            AsyncFunction("reverseGeocode") { args, promise in
                let json = Bridge.args(args)
                let location = CLLocation(
                    latitude: (json["latitude"] as? NSNumber)?.doubleValue ?? 0,
                    longitude: (json["longitude"] as? NSNumber)?.doubleValue ?? 0
                )
                let identifier = json["locale"] as? String ?? ""
                let locale = identifier.isEmpty ? nil : Locale(identifier: identifier)
                Bridge.onMain {
                    CLGeocoder().reverseGeocodeLocation(location, preferredLocale: locale) { placemarks, error in
                        if let error = error as? CLError, error.code == .geocodeFoundNoResult {
                            return Bridge.resolve(promise, [Any]())
                        }
                        if let error { return Bridge.reject(promise, error) }
                        Bridge.resolve(promise, (placemarks ?? []).map(Self.payload))
                    }
                }
            }
        }
    }

    private static func payload(_ placemark: CLPlacemark) -> [String: Any?] {
        [
            "name": placemark.name,
            "street": placemark.thoroughfare,
            "subThoroughfare": placemark.subThoroughfare,
            "subLocality": placemark.subLocality,
            "locality": placemark.locality,
            "subAdministrativeArea": placemark.subAdministrativeArea,
            "administrativeArea": placemark.administrativeArea,
            "postalCode": placemark.postalCode,
            "country": placemark.country,
            "isoCountryCode": placemark.isoCountryCode,
        ]
    }
}
