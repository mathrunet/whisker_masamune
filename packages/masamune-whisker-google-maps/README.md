# masamune-whisker-google-maps

A Google Map element for Whisker apps (Google Maps SDK for iOS / Android in a native view) with
markers, polylines, a custom style and camera control — the parts of `google_maps_flutter` /
`masamune_location_google` that Segue uses.

- **iOS**: `GMSMapView` (Google Maps SDK for iOS 10.x through SwiftPM; iOS 16 or later).
- **Android**: `MapView` (`com.google.android.gms:play-services-maps`), created when the element
  is attached and following the activity lifecycle.

Hosts other than iOS and Android render nothing.

## Setup

1. Create API keys for "Maps SDK for iOS" and "Maps SDK for Android" in the Google Cloud console
   and restrict them to the app (iOS bundle id; Android package name and signing certificate
   SHA-1).
2. Pass them with the CNG plugin in `whisker.rs` (keep them out of the repository, for example in
   environment variables):

```rust,ignore
use masamune_whisker_google_maps::cng::MasamuneGoogleMaps;

app.project_plugin::<MasamuneGoogleMaps>(|c| {
    c.ios_api_key(std::env::var("GOOGLE_MAPS_IOS_API_KEY").unwrap_or_default())
        .android_api_key(std::env::var("GOOGLE_MAPS_ANDROID_API_KEY").unwrap_or_default());
});
```

| Platform | What the plugin writes | Read by |
|---|---|---|
| iOS | `Info.plist` `MasamuneGoogleMapsApiKey` | the module, which calls `GMSServices.provideAPIKey` at launch |
| Android | `<meta-data android:name="com.google.android.geo.API_KEY" android:value="…">` in `<application>` | the Maps SDK |

Do not declare `com.google.android.geo.API_KEY` a second time (for example with `AndroidMetaData`).
The iOS package needs iOS 16 (Google Maps SDK 11 requires iOS 17, so the dependency stays below
11.0). `my_location` needs the location permission (`masamune-whisker-location`).

## Usage

```rust,ignore
use masamune_whisker_google_maps::{
    CameraPosition, CameraUpdate, GoogleMap, GoogleMapRef, LatLng, LatLngBounds, Marker,
    MarkerIcon, Polyline,
};
use whisker::prelude::*;

#[component]
fn trail_map(points: Signal<Vec<LatLng>>) -> Element {
    let map = GoogleMapRef::new();
    let markers = computed(move || {
        points.get().first().map(|start| Marker {
            id: "start".into(),
            position: *start,
            icon: MarkerIcon::Circle {
                size: 28.0,
                fill: 0xFF6366F1,
                stroke: 0xFFFFFFFF,
                stroke_width: 2.0,
                glyph: Some("♪".into()),
                glyph_color: 0xFFFFFFFF,
            },
            anchor: (0.5, 0.5),
            consume_taps: true,
            ..Default::default()
        }).into_iter().collect::<Vec<_>>()
    });
    let polylines = computed(move || vec![Polyline {
        id: "route".into(),
        points: points.get(),
        color: 0xFF6366F1,
        width: 5.0,
        tappable: true,
        ..Default::default()
    }]);
    render! {
        GoogleMap(
            initial_camera: CameraPosition::new(LatLng::new(35.681236, 139.767125), 14.0),
            markers: markers,
            polylines: polylines,
            map_style: include_str!("map_style.json").to_string(),
            map_ref: map,
            on_ready: move |_| {
                if let Some(bounds) = LatLngBounds::from_points(points.try_get_untracked().unwrap_or_default()) {
                    map.move_camera(CameraUpdate::Bounds { bounds, padding: 48.0 }, false);
                }
            },
            on_marker_tap: move |id: String| {},
            on_camera_idle: move |idle| { /* idle.visible: the visible bounds */ },
            style: css!(width: percent(100), height: percent(100)),
        )
    }
}
```

## Behavior and limits

- Markers and polylines are matched by `id`; changed properties are applied in place and marker
  icons are re-rendered only when their description changes.
- `MarkerIcon::Circle` draws a circle with an optional text glyph natively (use it for icon-like
  markers); `MarkerIcon::Image` draws a PNG / JPEG file; `MarkerIcon::Pin` is the standard pin.
- `initial_camera` is applied once; move the camera with `GoogleMapRef::move_camera`. Moves
  requested before the map exists are applied when it is created.
- Info windows show `title` / `snippet` on tap unless the marker has `consume_taps`. The Android
  map toolbar and the my-location button are disabled.
- Not provided: polygons, circles, ground overlays, heatmaps, clustering, lite mode and
  screen-coordinate projection queries (use `on_camera_idle`'s visible bounds).
