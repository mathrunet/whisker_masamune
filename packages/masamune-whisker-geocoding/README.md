# masamune-whisker-geocoding

Reverse geocoding (coordinate → address) with the platform geocoder for Whisker apps, the
counterpart of `placemarkFromCoordinates` of Flutter's `geocoding`.

- **iOS**: `CLGeocoder.reverseGeocodeLocation(_:preferredLocale:)`.
- **Android**: `android.location.Geocoder` (the asynchronous API on Android 13+, a background
  thread before).

No permission, `Info.plist` key or manifest entry is needed.

## Usage

```rust,ignore
use masamune_whisker_geocoding as geocoding;

let placemarks = geocoding::reverse_geocode(35.681236, 139.767125, Some("ja")).await?;
if let Some(place) = placemarks.first() {
    // place.administrative_area: Some("東京都"), place.locality: Some("千代田区"), …
}
```

## Behavior and limits

- Both platform geocoders use network services with rate limits and may return different
  granularity (for example iOS fills `name` with a landmark, Android with the address line).
  Cache results by a rounded coordinate and locale.
- `is_available` is `false` on Android devices without a geocoding backend; `reverse_geocode`
  fails there.
- Forward geocoding (address → coordinate) is not provided.
