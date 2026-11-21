//! Compile check of the README usage (not run).

use masamune_whisker_google_maps::{
    CameraIdle, CameraPosition, CameraUpdate, GoogleMap, GoogleMapRef, LatLng, LatLngBounds,
    Marker, MarkerIcon, Polyline,
};
use whisker::prelude::*;

#[allow(dead_code)]
#[component]
fn trail_map(points: Signal<Vec<LatLng>>) -> Element {
    let map = GoogleMapRef::new();
    let markers = computed(move || {
        points
            .get()
            .first()
            .map(|start| Marker {
                id: "start".into(),
                position: *start,
                icon: MarkerIcon::Circle {
                    size: 28.0,
                    fill: 0xFF63_66F1,
                    stroke: 0xFFFF_FFFF,
                    stroke_width: 2.0,
                    glyph: Some("♪".into()),
                    glyph_color: 0xFFFF_FFFF,
                },
                anchor: (0.5, 0.5),
                consume_taps: true,
                ..Default::default()
            })
            .into_iter()
            .collect::<Vec<_>>()
    });
    let polylines = computed(move || {
        vec![Polyline {
            id: "route".into(),
            points: points.get(),
            color: 0xFF63_66F1,
            width: 5.0,
            tappable: true,
            ..Default::default()
        }]
    });
    render! {
        GoogleMap(
            initial_camera: CameraPosition::new(LatLng::new(35.681236, 139.767125), 14.0),
            markers: markers,
            polylines: polylines,
            map_style: "[]".to_string(),
            map_ref: map,
            on_ready: move |_| {
                if let Some(bounds) = LatLngBounds::from_points(points.try_get_untracked().unwrap_or_default()) {
                    map.move_camera(CameraUpdate::Bounds { bounds, padding: 48.0 }, false);
                }
            },
            on_marker_tap: move |_id: String| {},
            on_camera_idle: move |_idle: CameraIdle| {},
            style: css!(width: percent(100), height: percent(100)),
        )
    }
}

#[test]
fn usage_compiles() {}
