// Module definition of masamune-whisker-google-maps (Android). KSP registers the
// `masamune-whisker-google-maps:GoogleMap` element; the view lives in GoogleMapView.kt. The API key
// is the `com.google.android.geo.API_KEY` manifest meta-data written by the crate's CNG plugin.

package net.mathru.whisker.google_maps

import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class GoogleMapModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("GoogleMap")
        View("masamune-whisker-google-maps:GoogleMap", GoogleMapView::class.java) {
            Prop("initial-camera", clear = { view: GoogleMapView -> view.setInitialCamera("") }) { view: GoogleMapView, value ->
                view.setInitialCamera(value.asString() ?: "")
            }
            Prop("background-color", clear = { view: GoogleMapView -> view.setBackgroundColor("") }) { view: GoogleMapView, value ->
                view.setBackgroundColor(value.asString() ?: "")
            }
            Prop("map-style", clear = { view: GoogleMapView -> view.setMapStyle("") }) { view: GoogleMapView, value ->
                view.setMapStyle(value.asString() ?: "")
            }
            Prop("padding", clear = { view: GoogleMapView -> view.setPadding("") }) { view: GoogleMapView, value ->
                view.setPadding(value.asString() ?: "")
            }
            Prop("zoom-gestures", clear = { view: GoogleMapView -> view.setGesture(GoogleMapView.Gesture.ZOOM, true) }) { view: GoogleMapView, value ->
                view.setGesture(GoogleMapView.Gesture.ZOOM, value.asBool() ?: true)
            }
            Prop("scroll-gestures", clear = { view: GoogleMapView -> view.setGesture(GoogleMapView.Gesture.SCROLL, true) }) { view: GoogleMapView, value ->
                view.setGesture(GoogleMapView.Gesture.SCROLL, value.asBool() ?: true)
            }
            Prop("tilt-gestures", clear = { view: GoogleMapView -> view.setGesture(GoogleMapView.Gesture.TILT, true) }) { view: GoogleMapView, value ->
                view.setGesture(GoogleMapView.Gesture.TILT, value.asBool() ?: true)
            }
            Prop("rotate-gestures", clear = { view: GoogleMapView -> view.setGesture(GoogleMapView.Gesture.ROTATE, true) }) { view: GoogleMapView, value ->
                view.setGesture(GoogleMapView.Gesture.ROTATE, value.asBool() ?: true)
            }
            Prop("my-location", clear = { view: GoogleMapView -> view.setMyLocation(false) }) { view: GoogleMapView, value ->
                view.setMyLocation(value.asBool() ?: false)
            }
            Prop("markers", clear = { view: GoogleMapView -> view.setMarkers("") }) { view: GoogleMapView, value ->
                view.setMarkers(value.asString() ?: "")
            }
            Prop("polylines", clear = { view: GoogleMapView -> view.setPolylines("") }) { view: GoogleMapView, value ->
                view.setPolylines(value.asString() ?: "")
            }

            Events("ready", "marker_tap", "polyline_tap", "map_tap", "camera_move_started", "camera_idle")

            Command("move_camera") { view: GoogleMapView, parameters ->
                view.moveCamera(parameters.asString() ?: "")
            }
        }
    }
}
