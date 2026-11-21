#![doc = include_str!("../README.md")]

pub mod cng;

use serde::{Deserialize, Serialize};
use whisker::platform_module::WhiskerValue;
use whisker::prelude::*;
use whisker::{ElementRef, Signal, Style};

/// A geographic coordinate in degrees.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LatLng {
    pub latitude: f64,
    pub longitude: f64,
}

impl LatLng {
    pub const fn new(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }
}

/// A rectangle of coordinates (south-west and north-east corners).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LatLngBounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

impl LatLngBounds {
    /// The smallest bounds holding every point, or `None` without points. Bounds crossing the
    /// antimeridian are not handled (the result spans the other way around the globe).
    pub fn from_points(points: impl IntoIterator<Item = LatLng>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        Some(points.fold(
            Self {
                south: first.latitude,
                west: first.longitude,
                north: first.latitude,
                east: first.longitude,
            },
            |bounds, point| Self {
                south: bounds.south.min(point.latitude),
                west: bounds.west.min(point.longitude),
                north: bounds.north.max(point.latitude),
                east: bounds.east.max(point.longitude),
            },
        ))
    }

    /// Whether `point` lies inside (edges included; bounds whose `west` is east of `east` wrap
    /// around the antimeridian).
    pub fn contains(&self, point: LatLng) -> bool {
        let latitude = (self.south..=self.north).contains(&point.latitude);
        let longitude = if self.west <= self.east {
            (self.west..=self.east).contains(&point.longitude)
        } else {
            point.longitude >= self.west || point.longitude <= self.east
        };
        latitude && longitude
    }

    pub fn center(&self) -> LatLng {
        LatLng::new(
            (self.south + self.north) / 2.0,
            (self.west + self.east) / 2.0,
        )
    }
}

/// Where the camera looks.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraPosition {
    pub target: LatLng,
    /// Zoom level (0 shows the whole world, about 15 streets, 20 buildings).
    pub zoom: f64,
    /// Direction in degrees clockwise from north.
    #[serde(default)]
    pub bearing: f64,
    /// Viewing angle in degrees from the nadir.
    #[serde(default)]
    pub tilt: f64,
}

impl CameraPosition {
    pub const fn new(target: LatLng, zoom: f64) -> Self {
        Self {
            target,
            zoom,
            bearing: 0.0,
            tilt: 0.0,
        }
    }
}

impl Default for CameraPosition {
    fn default() -> Self {
        Self::new(LatLng::default(), 2.0)
    }
}

/// How a marker is drawn.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MarkerIcon {
    /// The standard pin in a hue from 0 to 360 (0 red, 120 green, 240 blue).
    Pin { hue: f32 },
    /// A PNG or JPEG file drawn `size` points (dp) wide, keeping its aspect ratio.
    Image { path: String, size: f32 },
    /// A filled circle of `size` points with an optional border and a centered text glyph (for
    /// example `"♪"`). Colors are `0xAARRGGBB`.
    #[serde(rename_all = "camelCase")]
    Circle {
        size: f32,
        fill: u32,
        stroke: u32,
        stroke_width: f32,
        glyph: Option<String>,
        glyph_color: u32,
    },
}

impl Default for MarkerIcon {
    fn default() -> Self {
        Self::Pin { hue: 0.0 }
    }
}

/// A marker on the map. Markers are matched by `id` between updates.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    pub id: String,
    pub position: LatLng,
    /// Info window title shown on tap (when the marker is not `consume_taps`).
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub icon: MarkerIcon,
    /// Point of the icon placed at `position`, as fractions of its width and height
    /// (`(0.5, 1.0)` is the bottom center, `(0.5, 0.5)` the center).
    pub anchor: (f32, f32),
    pub z_index: f32,
    /// Report taps through `on_marker_tap` without the default behavior (centering and the info
    /// window).
    pub consume_taps: bool,
}

impl Default for Marker {
    fn default() -> Self {
        Self {
            id: String::new(),
            position: LatLng::default(),
            title: None,
            snippet: None,
            icon: MarkerIcon::default(),
            anchor: (0.5, 1.0),
            z_index: 0.0,
            consume_taps: false,
        }
    }
}

/// A line through coordinates. Polylines are matched by `id` between updates.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Polyline {
    pub id: String,
    pub points: Vec<LatLng>,
    /// `0xAARRGGBB`.
    pub color: u32,
    /// Width in points (dp).
    pub width: f32,
    pub z_index: f32,
    /// Report taps through `on_polyline_tap`.
    pub tappable: bool,
    /// Follow great circles instead of straight screen lines.
    pub geodesic: bool,
}

impl Default for Polyline {
    fn default() -> Self {
        Self {
            id: String::new(),
            points: Vec::new(),
            color: 0xFF00_00FF,
            width: 4.0,
            z_index: 0.0,
            tappable: false,
            geodesic: false,
        }
    }
}

/// Insets in points (dp) that the map keeps free for overlaid UI (controls, the Google logo and
/// camera fitting respect them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct MapPadding {
    pub top: f32,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
}

/// A camera move for [`GoogleMapRef::move_camera`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CameraUpdate {
    /// Move to a full camera position.
    Position { position: CameraPosition },
    /// Center on `target` at `zoom`, keeping bearing and tilt.
    LatLngZoom { target: LatLng, zoom: f64 },
    /// Center on `target`, keeping the zoom.
    LatLng { target: LatLng },
    /// Fit `bounds` with `padding` points around it.
    Bounds { bounds: LatLngBounds, padding: f32 },
}

#[derive(Serialize)]
struct CameraCommand {
    update: CameraUpdate,
    animate: bool,
}

/// The camera after a move ended (`on_camera_idle`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraIdle {
    pub position: CameraPosition,
    /// The visible area (the bounding box of the visible region).
    pub visible: LatLngBounds,
}

// Every native event carries its body under `detail`. Fields default so a partial body still
// reaches the handler.

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct IdEvent {
    #[serde(default)]
    pub detail: IdDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct IdDetail {
    #[serde(default)]
    pub id: String,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TapEvent {
    #[serde(default)]
    pub detail: TapDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TapDetail {
    #[serde(default)]
    pub latitude: f64,
    #[serde(default)]
    pub longitude: f64,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MoveStartedEvent {
    #[serde(default)]
    pub detail: MoveStartedDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MoveStartedDetail {
    #[serde(default)]
    pub gesture: bool,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct IdleEvent {
    #[serde(default)]
    pub detail: IdleDetail,
}

#[doc(hidden)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct IdleDetail {
    #[serde(default)]
    pub latitude: f64,
    #[serde(default)]
    pub longitude: f64,
    #[serde(default)]
    pub zoom: f64,
    #[serde(default)]
    pub bearing: f64,
    #[serde(default)]
    pub tilt: f64,
    #[serde(default)]
    pub south: f64,
    #[serde(default)]
    pub west: f64,
    #[serde(default)]
    pub north: f64,
    #[serde(default)]
    pub east: f64,
}

impl IdleDetail {
    fn camera(&self) -> CameraIdle {
        CameraIdle {
            position: CameraPosition {
                target: LatLng::new(self.latitude, self.longitude),
                zoom: self.zoom,
                bearing: self.bearing,
                tilt: self.tilt,
            },
            visible: LatLngBounds {
                south: self.south,
                west: self.west,
                north: self.north,
                east: self.east,
            },
        }
    }
}

/// Imperative handle of a mounted [`GoogleMap`], bound through its `map_ref` prop. Every method
/// is a no-op while the map is not mounted; camera moves before the map is ready are applied once
/// it is. The handle is `Copy`.
#[derive(Copy, Clone)]
pub struct GoogleMapRef {
    r: ElementRef,
}

impl GoogleMapRef {
    pub fn new() -> Self {
        Self {
            r: ElementRef::new(),
        }
    }

    #[doc(hidden)]
    pub fn r(&self) -> ElementRef {
        self.r
    }

    /// Moves the camera, animated or at once.
    pub fn move_camera(&self, update: CameraUpdate, animate: bool) {
        let _ = self.r.command(
            "move_camera",
            WhiskerValue::String(json(&CameraCommand { update, animate })),
        );
    }
}

impl Default for GoogleMapRef {
    fn default() -> Self {
        Self::new()
    }
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// Drops markers and polylines whose coordinates are not finite (the native SDKs reject them).
fn finite(point: &LatLng) -> bool {
    point.latitude.is_finite() && point.longitude.is_finite()
}

fn markers_json(markers: &[Marker]) -> String {
    let markers: Vec<&Marker> = markers
        .iter()
        .filter(|marker| !marker.id.is_empty() && finite(&marker.position))
        .collect();
    json(&markers)
}

fn polylines_json(polylines: &[Polyline]) -> String {
    let polylines: Vec<Polyline> = polylines
        .iter()
        .filter(|polyline| !polyline.id.is_empty())
        .map(|polyline| Polyline {
            points: polyline.points.iter().copied().filter(finite).collect(),
            ..polyline.clone()
        })
        .collect();
    json(&polylines)
}

// The camera and settings are declared before the overlays so they apply first.
#[doc(hidden)]
#[whisker::module_element(
    name = "masamune-whisker-google-maps:GoogleMap",
    measurement = None,
    commands = [("move_camera", String)],
)]
pub fn native_google_map(
    initial_camera: Signal<String>,
    map_style: Signal<String>,
    padding: Signal<String>,
    zoom_gestures: Signal<bool>,
    scroll_gestures: Signal<bool>,
    tilt_gestures: Signal<bool>,
    rotate_gestures: Signal<bool>,
    my_location: Signal<bool>,
    markers: Signal<String>,
    polylines: Signal<String>,
    style: Style,
    on_ready: (),
    on_marker_tap: IdEvent,
    on_polyline_tap: IdEvent,
    on_map_tap: TapEvent,
    on_camera_move_started: MoveStartedEvent,
    on_camera_idle: IdleEvent,
) {
}

/// Element schema referenced by the generated Host bootstrap code (Web, desktop).
#[doc(hidden)]
pub fn __whisker_element_module_definition() -> whisker::ElementModuleDefinition {
    whisker::ElementModuleDefinition::new(
        env!("CARGO_PKG_NAME"),
        [native_google_map_schema::element_provider()],
    )
}

/// A Google Map (Google Maps SDK for iOS / Android in a native view).
///
/// The map has no intrinsic size: give it one through `style`. `initial_camera` is applied when
/// the map is created; move it later with [`GoogleMapRef::move_camera`]. Markers and polylines
/// are diffed by `id`, so update the lists freely.
///
/// `map_style` is a Google Maps style JSON (the array of `featureType` / `elementType` /
/// `stylers` rules); an empty string uses the default style. `my_location` shows the blue dot
/// (it needs the location permission, for example from `masamune-whisker-location`).
///
/// Hosts other than iOS and Android render nothing.
#[component]
pub fn google_map(
    #[prop(default = CameraPosition::default().into())] initial_camera: Signal<CameraPosition>,
    #[prop(default = Vec::new().into())] markers: Signal<Vec<Marker>>,
    #[prop(default = Vec::new().into())] polylines: Signal<Vec<Polyline>>,
    #[prop(default = String::new().into())] map_style: Signal<String>,
    #[prop(default = MapPadding::default().into())] padding: Signal<MapPadding>,
    #[prop(default = true.into())] zoom_gestures: Signal<bool>,
    #[prop(default = true.into())] scroll_gestures: Signal<bool>,
    #[prop(default = true.into())] tilt_gestures: Signal<bool>,
    #[prop(default = true.into())] rotate_gestures: Signal<bool>,
    #[prop(default = false.into())] my_location: Signal<bool>,
    /// The map is ready (tiles may still be loading).
    on_ready: Option<Callback<()>>,
    /// A marker was tapped (its id).
    on_marker_tap: Option<Callback<String>>,
    /// A tappable polyline was tapped (its id).
    on_polyline_tap: Option<Callback<String>>,
    /// The map was tapped outside markers and polylines.
    on_map_tap: Option<Callback<LatLng>>,
    /// The camera started moving (`true` when the user moved it with a gesture).
    on_camera_move_started: Option<Callback<bool>>,
    /// The camera stopped moving.
    on_camera_idle: Option<Callback<CameraIdle>>,
    /// Structured Whisker style.
    style: Option<Style>,
    /// Imperative handle.
    map_ref: Option<GoogleMapRef>,
) -> Element {
    let mut builder = NativeGoogleMap::builder()
        .initial_camera(computed(move || json(&initial_camera.get())))
        .map_style(map_style)
        .padding(computed(move || json(&padding.get())))
        .zoom_gestures(zoom_gestures)
        .scroll_gestures(scroll_gestures)
        .tilt_gestures(tilt_gestures)
        .rotate_gestures(rotate_gestures)
        .my_location(my_location)
        .markers(computed(move || markers_json(&markers.get())))
        .polylines(computed(move || polylines_json(&polylines.get())))
        .style(style.clone().unwrap_or_default())
        .on_ready(move || {
            if let Some(cb) = &on_ready {
                cb.run(());
            }
        })
        .on_marker_tap(move |event: IdEvent| {
            if let Some(cb) = &on_marker_tap {
                cb.run(event.detail.id);
            }
        })
        .on_polyline_tap(move |event: IdEvent| {
            if let Some(cb) = &on_polyline_tap {
                cb.run(event.detail.id);
            }
        })
        .on_map_tap(move |event: TapEvent| {
            if let Some(cb) = &on_map_tap {
                cb.run(LatLng::new(event.detail.latitude, event.detail.longitude));
            }
        })
        .on_camera_move_started(move |event: MoveStartedEvent| {
            if let Some(cb) = &on_camera_move_started {
                cb.run(event.detail.gesture);
            }
        })
        .on_camera_idle(move |event: IdleEvent| {
            if let Some(cb) = &on_camera_idle {
                cb.run(event.detail.camera());
            }
        });
    if let Some(handle) = map_ref {
        builder = builder.element_ref(handle.r());
    }
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_from_points() {
        let bounds = LatLngBounds::from_points([
            LatLng::new(35.0, 139.0),
            LatLng::new(36.0, 138.5),
            LatLng::new(35.5, 140.0),
        ])
        .unwrap();
        assert_eq!(
            bounds,
            LatLngBounds {
                south: 35.0,
                west: 138.5,
                north: 36.0,
                east: 140.0
            }
        );
        assert!(bounds.contains(LatLng::new(35.5, 139.5)));
        assert!(!bounds.contains(LatLng::new(34.9, 139.5)));
        assert_eq!(bounds.center(), LatLng::new(35.5, 139.25));
        assert_eq!(LatLngBounds::from_points([]), None);
        let wrapping = LatLngBounds {
            south: -10.0,
            west: 170.0,
            north: 10.0,
            east: -170.0,
        };
        assert!(wrapping.contains(LatLng::new(0.0, 179.0)));
        assert!(wrapping.contains(LatLng::new(0.0, -175.0)));
        assert!(!wrapping.contains(LatLng::new(0.0, 0.0)));
    }

    #[test]
    fn encodes_overlays() {
        let markers = vec![
            Marker {
                id: "a".into(),
                position: LatLng::new(1.0, 2.0),
                icon: MarkerIcon::Circle {
                    size: 28.0,
                    fill: 0xFF11_2233,
                    stroke: 0xFFFF_FFFF,
                    stroke_width: 2.0,
                    glyph: Some("♪".into()),
                    glyph_color: 0xFFFF_FFFF,
                },
                consume_taps: true,
                ..Default::default()
            },
            Marker {
                id: String::new(),
                ..Default::default()
            },
            Marker {
                id: "nan".into(),
                position: LatLng::new(f64::NAN, 0.0),
                ..Default::default()
            },
        ];
        let value: serde_json::Value = serde_json::from_str(&markers_json(&markers)).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);
        assert_eq!(value[0]["icon"]["type"], "circle");
        assert_eq!(value[0]["icon"]["strokeWidth"], 2.0);
        assert_eq!(value[0]["icon"]["fill"], 0xFF11_2233_u32);
        assert_eq!(value[0]["anchor"], serde_json::json!([0.5, 1.0]));
        assert_eq!(value[0]["consumeTaps"], true);
        assert_eq!(value[0]["position"]["latitude"], 1.0);

        let polylines = vec![Polyline {
            id: "route".into(),
            points: vec![LatLng::new(1.0, 2.0), LatLng::new(f64::INFINITY, 0.0)],
            ..Default::default()
        }];
        let value: serde_json::Value = serde_json::from_str(&polylines_json(&polylines)).unwrap();
        assert_eq!(value[0]["points"].as_array().unwrap().len(), 1);
        assert_eq!(value[0]["zIndex"], 0.0);
    }

    #[test]
    fn encodes_camera_commands() {
        let command = CameraCommand {
            update: CameraUpdate::Bounds {
                bounds: LatLngBounds::default(),
                padding: 48.0,
            },
            animate: true,
        };
        let value: serde_json::Value = serde_json::from_str(&json(&command)).unwrap();
        assert_eq!(value["update"]["type"], "bounds");
        assert_eq!(value["update"]["padding"], 48.0);
        assert_eq!(value["animate"], true);
        let zoom = json(&CameraUpdate::LatLngZoom {
            target: LatLng::new(1.0, 2.0),
            zoom: 15.0,
        });
        assert!(zoom.contains(r#""type":"latLngZoom""#), "{zoom}");
    }

    #[test]
    fn schema_and_events() {
        let schema = native_google_map_schema::schema();
        assert_eq!(schema.name, "masamune-whisker-google-maps:GoogleMap");
        assert_eq!(
            schema
                .commands
                .iter()
                .map(|command| command.name.as_str())
                .collect::<Vec<_>>(),
            ["move_camera"]
        );
        assert_eq!(
            schema
                .properties
                .iter()
                .map(|property| property.name.as_str())
                .collect::<Vec<_>>(),
            [
                "initial-camera",
                "map-style",
                "padding",
                "zoom-gestures",
                "scroll-gestures",
                "tilt-gestures",
                "rotate-gestures",
                "my-location",
                "markers",
                "polylines",
            ]
        );
        let idle: IdleEvent = WhiskerValue::map([(
            "detail",
            WhiskerValue::map([
                ("latitude", WhiskerValue::Float(35.0)),
                ("zoom", WhiskerValue::Float(12.5)),
                ("north", WhiskerValue::Float(36.0)),
            ]),
        )])
        .deserialize_into()
        .unwrap();
        let camera = idle.detail.camera();
        assert_eq!(camera.position.target.latitude, 35.0);
        assert_eq!(camera.position.zoom, 12.5);
        assert_eq!(camera.visible.north, 36.0);
        let tap: IdEvent = WhiskerValue::map([(
            "detail",
            WhiskerValue::map([("id", WhiskerValue::String("m1".into()))]),
        )])
        .deserialize_into()
        .unwrap();
        assert_eq!(tap.detail.id, "m1");
    }
}
