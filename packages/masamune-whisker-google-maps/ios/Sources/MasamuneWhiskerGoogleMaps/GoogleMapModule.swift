// Module definition of masamune-whisker-google-maps (iOS). The codegen plugin registers the
// `masamune-whisker-google-maps:GoogleMap` element; the view lives in GoogleMapView.swift.
//
// `definition()` runs from `application(_:didFinishLaunchingWithOptions:)`, before any map view
// exists, which is where Google requires `GMSServices.provideAPIKey`. The key comes from the
// `MasamuneGoogleMapsApiKey` Info.plist entry written by the crate's CNG plugin.

import Foundation
import GoogleMaps
import WhiskerModule

@WhiskerModule
public final class GoogleMapModule: Module {
    private static var keyProvided = false

    public override func definition() -> ModuleDefinition {
        if !Self.keyProvided,
           let key = Bundle.main.object(forInfoDictionaryKey: "MasamuneGoogleMapsApiKey") as? String,
           !key.isEmpty {
            Self.keyProvided = GMSServices.provideAPIKey(key)
        }
        if !Self.keyProvided {
            NSLog("[masamune-whisker-google-maps] MasamuneGoogleMapsApiKey is missing from Info.plist; maps stay blank")
        }
        return ModuleDefinition {
            Name("GoogleMap")
            View("masamune-whisker-google-maps:GoogleMap", GoogleMapView.self) {
                Prop("initial-camera", clear: { (view: GoogleMapView) in view.setInitialCamera("") }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setInitialCamera(value.asString ?? "")
                }
                Prop("background-color", clear: { (view: GoogleMapView) in view.setBackgroundColor("") }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setBackgroundColor(value.asString ?? "")
                }
                Prop("map-style", clear: { (view: GoogleMapView) in view.setMapStyle("") }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setMapStyle(value.asString ?? "")
                }
                Prop("padding", clear: { (view: GoogleMapView) in view.setPadding("") }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setPadding(value.asString ?? "")
                }
                Prop("zoom-gestures", clear: { (view: GoogleMapView) in view.setGesture(.zoom, true) }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setGesture(.zoom, value.asBool ?? true)
                }
                Prop("scroll-gestures", clear: { (view: GoogleMapView) in view.setGesture(.scroll, true) }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setGesture(.scroll, value.asBool ?? true)
                }
                Prop("tilt-gestures", clear: { (view: GoogleMapView) in view.setGesture(.tilt, true) }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setGesture(.tilt, value.asBool ?? true)
                }
                Prop("rotate-gestures", clear: { (view: GoogleMapView) in view.setGesture(.rotate, true) }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setGesture(.rotate, value.asBool ?? true)
                }
                Prop("my-location", clear: { (view: GoogleMapView) in view.setMyLocation(false) }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setMyLocation(value.asBool ?? false)
                }
                Prop("markers", clear: { (view: GoogleMapView) in view.setMarkers("") }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setMarkers(value.asString ?? "")
                }
                Prop("polylines", clear: { (view: GoogleMapView) in view.setPolylines("") }) { (view: GoogleMapView, value: WhiskerValue) in
                    view.setPolylines(value.asString ?? "")
                }

                Events("ready", "marker_tap", "polyline_tap", "map_tap", "camera_move_started", "camera_idle")

                Command("move_camera") { (view: GoogleMapView, parameters: WhiskerValue) in
                    view.moveCamera(parameters.asString ?? "")
                }
            }
        }
    }
}
