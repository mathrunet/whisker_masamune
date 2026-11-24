// Google Map element of masamune-whisker-google-maps (iOS), registered by GoogleMapModule.
//
// A GMSMapView fills the element. Props are JSON strings built in Rust (see `src/lib.rs`):
// markers and polylines are diffed by id, the initial camera is applied when the map is created,
// and camera moves requested before then are replayed once it exists.

import Foundation
import GoogleMaps
import UIKit
import WhiskerModule

@objc(MasamuneWhiskerGoogleMapView)
public final class GoogleMapView: WhiskerUI<UIView> {
    enum Gesture { case zoom, scroll, tilt, rotate }

    private lazy var containerView: UIView = {
        let view = UIView()
        view.clipsToBounds = true
        return view
    }()

    private var mapView: GMSMapView?
    private var initialCamera: GMSCameraPosition?
    private var styleJSON = ""
    private var mapBackgroundColor: UIColor?
    private var padding = UIEdgeInsets.zero
    private var gestures: [Gesture: Bool] = [.zoom: true, .scroll: true, .tilt: true, .rotate: true]
    private var myLocation = false
    private var markerSpecs: [[String: Any]] = []
    private var polylineSpecs: [[String: Any]] = []
    private var markers: [String: GMSMarker] = [:]
    private var markerKeys: [String: String] = [:]
    private var polylines: [String: GMSPolyline] = [:]
    private var pendingCamera: [String: Any]?
    private var readySent = false

    @objc public override func createView() -> UIView {
        let options = GMSMapViewOptions()
        options.camera = initialCamera ?? GMSCameraPosition(latitude: 0, longitude: 0, zoom: 2)
        options.frame = containerView.bounds
        let map = GMSMapView(options: options)
        map.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        map.delegate = self
        containerView.addSubview(map)
        mapView = map
        // Props can arrive before createView(); apply what is known.
        applyBackground()
        applySettings()
        applyStyle()
        syncMarkers()
        syncPolylines()
        if let pendingCamera {
            self.pendingCamera = nil
            apply(pendingCamera)
        }
        return containerView
    }

    // MARK: - Props

    func setInitialCamera(_ json: String) {
        guard let object = Self.object(json), let camera = Self.camera(object) else { return }
        // Only the first camera matters; later moves go through `move_camera`.
        if initialCamera == nil {
            initialCamera = camera
            if let mapView, !readySent { mapView.camera = camera }
        }
    }

    /// `0xAARRGGBB` as a decimal string; empty restores the SDK default. Painted until the tiles
    /// are drawn, so a dark style does not flash the light default.
    func setBackgroundColor(_ value: String) {
        mapBackgroundColor = UInt32(value).flatMap { Self.color(NSNumber(value: $0)) }
        applyBackground()
    }

    func setMapStyle(_ json: String) {
        styleJSON = json
        applyStyle()
    }

    func setPadding(_ json: String) {
        let object = Self.object(json) ?? [:]
        padding = UIEdgeInsets(
            top: Self.number(object["top"]),
            left: Self.number(object["left"]),
            bottom: Self.number(object["bottom"]),
            right: Self.number(object["right"])
        )
        mapView?.padding = padding
    }

    func setGesture(_ gesture: Gesture, _ enabled: Bool) {
        gestures[gesture] = enabled
        applySettings()
    }

    func setMyLocation(_ enabled: Bool) {
        myLocation = enabled
        mapView?.isMyLocationEnabled = enabled
    }

    func setMarkers(_ json: String) {
        markerSpecs = Self.array(json)
        syncMarkers()
    }

    func setPolylines(_ json: String) {
        polylineSpecs = Self.array(json)
        syncPolylines()
    }

    // MARK: - Commands

    func moveCamera(_ json: String) {
        guard let command = Self.object(json) else { return }
        guard mapView != nil else {
            pendingCamera = command
            return
        }
        apply(command)
    }

    private func apply(_ command: [String: Any]) {
        guard let mapView, let update = command["update"] as? [String: Any] else { return }
        let cameraUpdate: GMSCameraUpdate
        switch update["type"] as? String {
        case "position":
            guard let position = update["position"] as? [String: Any], let camera = Self.camera(position) else { return }
            cameraUpdate = GMSCameraUpdate.setCamera(camera)
        case "latLngZoom":
            guard let target = Self.coordinate(update["target"]) else { return }
            cameraUpdate = GMSCameraUpdate.setTarget(target, zoom: Float(Self.number(update["zoom"])))
        case "latLng":
            guard let target = Self.coordinate(update["target"]) else { return }
            cameraUpdate = GMSCameraUpdate.setTarget(target)
        case "bounds":
            guard let bounds = update["bounds"] as? [String: Any] else { return }
            let coordinateBounds = GMSCoordinateBounds(
                coordinate: CLLocationCoordinate2D(latitude: Self.number(bounds["south"]), longitude: Self.number(bounds["west"])),
                coordinate: CLLocationCoordinate2D(latitude: Self.number(bounds["north"]), longitude: Self.number(bounds["east"]))
            )
            cameraUpdate = GMSCameraUpdate.fit(coordinateBounds, withPadding: Self.number(update["padding"]))
        default:
            return
        }
        if command["animate"] as? Bool == true {
            mapView.animate(with: cameraUpdate)
        } else {
            mapView.moveCamera(cameraUpdate)
        }
    }

    // MARK: - Map state

    private func applySettings() {
        guard let mapView else { return }
        mapView.settings.zoomGestures = gestures[.zoom] ?? true
        mapView.settings.scrollGestures = gestures[.scroll] ?? true
        mapView.settings.tiltGestures = gestures[.tilt] ?? true
        mapView.settings.rotateGestures = gestures[.rotate] ?? true
        mapView.settings.consumesGesturesInView = true
        mapView.isMyLocationEnabled = myLocation
        mapView.padding = padding
    }

    private func applyBackground() {
        containerView.backgroundColor = mapBackgroundColor
        mapView?.backgroundColor = mapBackgroundColor
    }

    private func applyStyle() {
        guard let mapView else { return }
        if styleJSON.isEmpty {
            mapView.mapStyle = nil
            return
        }
        do {
            mapView.mapStyle = try GMSMapStyle(jsonString: styleJSON)
        } catch {
            NSLog("[masamune-whisker-google-maps] invalid map style: \(error.localizedDescription)")
        }
    }

    private func syncMarkers() {
        guard let mapView else { return }
        var seen = Set<String>()
        for spec in markerSpecs {
            guard let id = spec["id"] as? String, !id.isEmpty, let position = Self.coordinate(spec["position"]) else { continue }
            seen.insert(id)
            let marker = markers[id] ?? GMSMarker()
            marker.position = position
            marker.title = spec["title"] as? String
            marker.snippet = spec["snippet"] as? String
            marker.zIndex = Int32(Self.number(spec["zIndex"]))
            marker.userData = id
            if let anchor = spec["anchor"] as? [NSNumber], anchor.count == 2 {
                marker.groundAnchor = CGPoint(x: anchor[0].doubleValue, y: anchor[1].doubleValue)
            }
            // Re-render the icon only when its description changed.
            let icon = spec["icon"] as? [String: Any] ?? [:]
            let key = Self.key(icon)
            if markerKeys[id] != key {
                markerKeys[id] = key
                marker.icon = Self.icon(icon)
            }
            if marker.map == nil { marker.map = mapView }
            markers[id] = marker
        }
        for (id, marker) in markers where !seen.contains(id) {
            marker.map = nil
            markers[id] = nil
            markerKeys[id] = nil
        }
    }

    private func syncPolylines() {
        guard let mapView else { return }
        var seen = Set<String>()
        for spec in polylineSpecs {
            guard let id = spec["id"] as? String, !id.isEmpty else { continue }
            seen.insert(id)
            let path = GMSMutablePath()
            for point in spec["points"] as? [Any] ?? [] {
                if let coordinate = Self.coordinate(point) { path.add(coordinate) }
            }
            let polyline = polylines[id] ?? GMSPolyline()
            polyline.path = path
            polyline.strokeColor = Self.color(spec["color"]) ?? .blue
            polyline.strokeWidth = Self.number(spec["width"])
            polyline.zIndex = Int32(Self.number(spec["zIndex"]))
            polyline.isTappable = spec["tappable"] as? Bool ?? false
            polyline.geodesic = spec["geodesic"] as? Bool ?? false
            polyline.userData = id
            if polyline.map == nil { polyline.map = mapView }
            polylines[id] = polyline
        }
        for (id, polyline) in polylines where !seen.contains(id) {
            polyline.map = nil
            polylines[id] = nil
        }
    }

    // MARK: - Icons

    private static func icon(_ spec: [String: Any]) -> UIImage? {
        switch spec["type"] as? String {
        case "image":
            guard let path = spec["path"] as? String, let image = UIImage(contentsOfFile: path) else { return nil }
            let width = max(1, number(spec["size"]))
            let size = CGSize(width: width, height: width * image.size.height / max(image.size.width, 1))
            return UIGraphicsImageRenderer(size: size).image { _ in image.draw(in: CGRect(origin: .zero, size: size)) }
        case "circle":
            let side = max(1, number(spec["size"]))
            let strokeWidth = max(0, number(spec["strokeWidth"]))
            let fill = color(spec["fill"]) ?? .white
            let stroke = color(spec["stroke"]) ?? .clear
            let glyph = spec["glyph"] as? String
            let glyphColor = color(spec["glyphColor"]) ?? .white
            return UIGraphicsImageRenderer(size: CGSize(width: side, height: side)).image { context in
                let rect = CGRect(x: strokeWidth / 2, y: strokeWidth / 2, width: side - strokeWidth, height: side - strokeWidth)
                context.cgContext.setFillColor(fill.cgColor)
                context.cgContext.fillEllipse(in: rect)
                if strokeWidth > 0 {
                    context.cgContext.setStrokeColor(stroke.cgColor)
                    context.cgContext.setLineWidth(strokeWidth)
                    context.cgContext.strokeEllipse(in: rect)
                }
                if let glyph, !glyph.isEmpty {
                    let attributes: [NSAttributedString.Key: Any] = [
                        .font: UIFont.systemFont(ofSize: side * 0.55, weight: .semibold),
                        .foregroundColor: glyphColor,
                    ]
                    let text = NSAttributedString(string: glyph, attributes: attributes)
                    let size = text.size()
                    text.draw(at: CGPoint(x: (side - size.width) / 2, y: (side - size.height) / 2))
                }
            }
        default:
            let hue = number(spec["hue"]).truncatingRemainder(dividingBy: 360) / 360
            return GMSMarker.markerImage(with: UIColor(hue: hue, saturation: 1, brightness: 1, alpha: 1))
        }
    }

    // MARK: - JSON helpers

    private static func object(_ json: String) -> [String: Any]? {
        guard !json.isEmpty, let data = json.data(using: .utf8) else { return nil }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    private static func array(_ json: String) -> [[String: Any]] {
        guard !json.isEmpty, let data = json.data(using: .utf8) else { return [] }
        return (try? JSONSerialization.jsonObject(with: data)) as? [[String: Any]] ?? []
    }

    private static func key(_ object: [String: Any]) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys]) else { return "" }
        return String(data: data, encoding: .utf8) ?? ""
    }

    private static func number(_ value: Any?) -> CGFloat {
        CGFloat((value as? NSNumber)?.doubleValue ?? 0)
    }

    private static func coordinate(_ value: Any?) -> CLLocationCoordinate2D? {
        guard let object = value as? [String: Any],
              let latitude = (object["latitude"] as? NSNumber)?.doubleValue,
              let longitude = (object["longitude"] as? NSNumber)?.doubleValue
        else { return nil }
        let coordinate = CLLocationCoordinate2D(latitude: latitude, longitude: longitude)
        return CLLocationCoordinate2DIsValid(coordinate) ? coordinate : nil
    }

    private static func camera(_ object: [String: Any]) -> GMSCameraPosition? {
        guard let target = coordinate(object["target"]) else { return nil }
        return GMSCameraPosition(
            target: target,
            zoom: Float(number(object["zoom"])),
            bearing: Double(number(object["bearing"])),
            viewingAngle: Double(number(object["tilt"]))
        )
    }

    /// `0xAARRGGBB` → UIColor.
    private static func color(_ value: Any?) -> UIColor? {
        guard let raw = (value as? NSNumber)?.uint32Value else { return nil }
        return UIColor(
            red: CGFloat((raw >> 16) & 0xFF) / 255,
            green: CGFloat((raw >> 8) & 0xFF) / 255,
            blue: CGFloat(raw & 0xFF) / 255,
            alpha: CGFloat((raw >> 24) & 0xFF) / 255
        )
    }

    fileprivate func emit(_ name: String, _ params: [String: Any] = [:]) {
        WhiskerCustomEvent.dispatch(from: self, name: name, params: params)
    }

    fileprivate func readyIfNeeded() {
        guard !readySent else { return }
        readySent = true
        emit("ready")
    }
}

// MARK: - GMSMapViewDelegate

extension GoogleMapView: GMSMapViewDelegate {
    public func mapViewDidFinishTileRendering(_ mapView: GMSMapView) {
        readyIfNeeded()
    }

    public func mapView(_ mapView: GMSMapView, didTap marker: GMSMarker) -> Bool {
        guard let id = marker.userData as? String else { return false }
        emit("marker_tap", ["id": id])
        let consume = markerSpecs.first { $0["id"] as? String == id }?["consumeTaps"] as? Bool ?? false
        return consume
    }

    public func mapView(_ mapView: GMSMapView, didTap overlay: GMSOverlay) {
        guard let id = overlay.userData as? String, overlay is GMSPolyline else { return }
        emit("polyline_tap", ["id": id])
    }

    public func mapView(_ mapView: GMSMapView, didTapAt coordinate: CLLocationCoordinate2D) {
        emit("map_tap", ["latitude": coordinate.latitude, "longitude": coordinate.longitude])
    }

    public func mapView(_ mapView: GMSMapView, willMove gesture: Bool) {
        emit("camera_move_started", ["gesture": gesture])
    }

    public func mapView(_ mapView: GMSMapView, idleAt position: GMSCameraPosition) {
        readyIfNeeded()
        let bounds = GMSCoordinateBounds(region: mapView.projection.visibleRegion())
        emit("camera_idle", [
            "latitude": position.target.latitude,
            "longitude": position.target.longitude,
            "zoom": Double(position.zoom),
            "bearing": position.bearing,
            "tilt": position.viewingAngle,
            "south": bounds.southWest.latitude,
            "west": bounds.southWest.longitude,
            "north": bounds.northEast.latitude,
            "east": bounds.northEast.longitude,
        ])
    }
}
