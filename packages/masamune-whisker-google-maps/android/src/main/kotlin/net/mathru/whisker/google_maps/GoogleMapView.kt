// Google Map element of masamune-whisker-google-maps (Android), registered by GoogleMapModule.
//
// The element view is a FrameLayout hosting a MapView. The MapView is created when the element is
// attached and destroyed when it is detached (keeping the camera for the next attach), and follows
// the host activity's lifecycle (start / resume / pause / stop) in between.
//
// WhiskerUI calls createView() from its constructor, before the properties of this class are
// initialized, so createView() only builds the container.
//
// Props are JSON strings built in Rust (see `src/lib.rs`): markers and polylines are diffed by id,
// the initial camera is used when the map is created, and camera moves requested before the map
// exists are replayed once it does. Sizes arrive in dp and are converted to pixels here.

package net.mathru.whisker.google_maps

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import android.util.Log
import android.view.MotionEvent
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.LifecycleOwner
import com.google.android.gms.maps.CameraUpdate
import com.google.android.gms.maps.CameraUpdateFactory
import com.google.android.gms.maps.GoogleMap
import com.google.android.gms.maps.GoogleMapOptions
import com.google.android.gms.maps.MapView
import com.google.android.gms.maps.model.BitmapDescriptor
import com.google.android.gms.maps.model.BitmapDescriptorFactory
import com.google.android.gms.maps.model.CameraPosition
import com.google.android.gms.maps.model.LatLng
import com.google.android.gms.maps.model.LatLngBounds
import com.google.android.gms.maps.model.MapStyleOptions
import com.google.android.gms.maps.model.Marker
import com.google.android.gms.maps.model.MarkerOptions
import com.google.android.gms.maps.model.Polyline
import com.google.android.gms.maps.model.PolylineOptions
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.WhiskerContext
import rs.whisker.runtime.WhiskerCustomEvent
import rs.whisker.runtime.WhiskerUI
import kotlin.math.max

class GoogleMapView(context: WhiskerContext) : WhiskerUI<FrameLayout>(context) {
    enum class Gesture { ZOOM, SCROLL, TILT, ROTATE }

    private var mapView: MapView? = null
    private var map: GoogleMap? = null
    private var initialCamera: CameraPosition? = null
    private var lastCamera: CameraPosition? = null
    private var styleJson = ""
    private var backgroundColor: Int? = null
    private var paddingDp = FloatArray(4) // left, top, right, bottom
    private val gestures = mutableMapOf(Gesture.ZOOM to true, Gesture.SCROLL to true, Gesture.TILT to true, Gesture.ROTATE to true)
    private var myLocation = false
    private var markerSpecs: List<JSONObject> = emptyList()
    private var polylineSpecs: List<JSONObject> = emptyList()
    private val markers = mutableMapOf<String, Marker>()
    private val markerKeys = mutableMapOf<String, String>()
    private val polylines = mutableMapOf<String, Polyline>()
    private var pendingCamera: JSONObject? = null
    private var readySent = false

    private var lifecycle: Lifecycle? = null
    private var started = false
    private var resumed = false
    private val observer = LifecycleEventObserver { _, event ->
        val view = mapView ?: return@LifecycleEventObserver
        when (event) {
            Lifecycle.Event.ON_START -> start(view)
            Lifecycle.Event.ON_RESUME -> resume(view)
            Lifecycle.Event.ON_PAUSE -> pause(view)
            Lifecycle.Event.ON_STOP -> stop(view)
            else -> Unit
        }
    }

    // The container keeps enclosing scroll views from stealing the map's gestures.
    override fun createView(context: Context): FrameLayout = object : FrameLayout(context) {
        override fun onInterceptTouchEvent(event: MotionEvent): Boolean {
            if (event.actionMasked == MotionEvent.ACTION_DOWN) parent?.requestDisallowInterceptTouchEvent(true)
            return super.onInterceptTouchEvent(event)
        }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        createMap()
    }

    override fun onDetachedFromWindow() {
        destroyMap()
        super.onDetachedFromWindow()
    }

    // MARK: - Props

    fun setInitialCamera(json: String) {
        if (initialCamera != null) return
        initialCamera = parse(json)?.let { camera(it) } ?: return
        if (map != null && !readySent) initialCamera?.let { map?.moveCamera(CameraUpdateFactory.newCameraPosition(it)) }
    }

    // `0xAARRGGBB` as a decimal string; empty restores the SDK default. The map's own background
    // is fixed when the MapView is created, so a change reaches it on the next attach.
    fun setBackgroundColor(value: String) {
        backgroundColor = value.toLongOrNull()?.toInt()
        applyBackground()
    }

    fun setMapStyle(json: String) {
        styleJson = json
        applyStyle()
    }

    fun setPadding(json: String) {
        val value = parse(json) ?: JSONObject()
        paddingDp = floatArrayOf(
            value.optDouble("left", 0.0).toFloat(),
            value.optDouble("top", 0.0).toFloat(),
            value.optDouble("right", 0.0).toFloat(),
            value.optDouble("bottom", 0.0).toFloat(),
        )
        applySettings()
    }

    fun setGesture(gesture: Gesture, enabled: Boolean) {
        gestures[gesture] = enabled
        applySettings()
    }

    fun setMyLocation(enabled: Boolean) {
        myLocation = enabled
        applySettings()
    }

    fun setMarkers(json: String) {
        markerSpecs = list(json)
        syncMarkers()
    }

    fun setPolylines(json: String) {
        polylineSpecs = list(json)
        syncPolylines()
    }

    // MARK: - Commands

    fun moveCamera(json: String) {
        val command = parse(json) ?: return
        if (map == null) {
            pendingCamera = command
            return
        }
        apply(command)
    }

    private fun apply(command: JSONObject) {
        val map = map ?: return
        val update = command.optJSONObject("update") ?: return
        val cameraUpdate: CameraUpdate = when (update.optString("type")) {
            "position" -> camera(update.optJSONObject("position") ?: return)?.let { CameraUpdateFactory.newCameraPosition(it) }
            "latLngZoom" -> latLng(update.optJSONObject("target"))?.let {
                CameraUpdateFactory.newLatLngZoom(it, update.optDouble("zoom", map.cameraPosition.zoom.toDouble()).toFloat())
            }
            "latLng" -> latLng(update.optJSONObject("target"))?.let { CameraUpdateFactory.newLatLng(it) }
            "bounds" -> update.optJSONObject("bounds")?.let { bounds ->
                val south = bounds.optDouble("south")
                val west = bounds.optDouble("west")
                val north = bounds.optDouble("north")
                val east = bounds.optDouble("east")
                if (listOf(south, west, north, east).any { it.isNaN() } || south > north) return
                CameraUpdateFactory.newLatLngBounds(
                    LatLngBounds(LatLng(south, west), LatLng(north, east)),
                    px(update.optDouble("padding", 0.0).toFloat()),
                )
            }
            else -> null
        } ?: return
        try {
            if (command.optBoolean("animate")) map.animateCamera(cameraUpdate) else map.moveCamera(cameraUpdate)
        } catch (error: IllegalStateException) {
            // newLatLngBounds before the map has a size: retry after layout.
            mapView?.post { this.map?.let { apply(command) } }
        }
    }

    // MARK: - Map lifecycle

    private fun createMap() {
        if (mapView != null) return
        val camera = lastCamera ?: initialCamera ?: CameraPosition(LatLng(0.0, 0.0), 2f, 0f, 0f)
        val options = GoogleMapOptions().camera(camera).mapToolbarEnabled(false)
        // Painted until the tiles are drawn, so a dark style does not flash the light default.
        backgroundColor?.let { options.backgroundColor(it) }
        val view = MapView(context, options)
        view.onCreate(null)
        view().addView(view, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        mapView = view
        applyBackground()
        val owner = findActivity(context) as? LifecycleOwner
        if (owner != null) {
            lifecycle = owner.lifecycle
            // Replays ON_START / ON_RESUME up to the current state.
            owner.lifecycle.addObserver(observer)
        } else {
            start(view)
            resume(view)
        }
        view.getMapAsync { googleMap ->
            if (mapView !== view) return@getMapAsync
            onMapReady(googleMap)
        }
    }

    private fun onMapReady(googleMap: GoogleMap) {
        map = googleMap
        googleMap.setOnMapLoadedCallback {
            if (!readySent) {
                readySent = true
                emit("ready")
            }
        }
        googleMap.setOnMarkerClickListener { marker ->
            val id = marker.tag as? String ?: return@setOnMarkerClickListener false
            emit("marker_tap", mapOf("id" to id))
            markerSpecs.firstOrNull { it.optString("id") == id }?.optBoolean("consumeTaps") ?: false
        }
        googleMap.setOnPolylineClickListener { polyline ->
            (polyline.tag as? String)?.let { emit("polyline_tap", mapOf("id" to it)) }
        }
        googleMap.setOnMapClickListener { point ->
            emit("map_tap", mapOf("latitude" to point.latitude, "longitude" to point.longitude))
        }
        googleMap.setOnCameraMoveStartedListener { reason ->
            emit("camera_move_started", mapOf("gesture" to (reason == GoogleMap.OnCameraMoveStartedListener.REASON_GESTURE)))
        }
        googleMap.setOnCameraIdleListener {
            val position = googleMap.cameraPosition
            val bounds = googleMap.projection.visibleRegion.latLngBounds
            emit(
                "camera_idle",
                mapOf(
                    "latitude" to position.target.latitude,
                    "longitude" to position.target.longitude,
                    "zoom" to position.zoom.toDouble(),
                    "bearing" to position.bearing.toDouble(),
                    "tilt" to position.tilt.toDouble(),
                    "south" to bounds.southwest.latitude,
                    "west" to bounds.southwest.longitude,
                    "north" to bounds.northeast.latitude,
                    "east" to bounds.northeast.longitude,
                ),
            )
        }
        applySettings()
        applyStyle()
        syncMarkers()
        syncPolylines()
        pendingCamera?.let {
            pendingCamera = null
            apply(it)
        }
    }

    private fun destroyMap() {
        val view = mapView ?: return
        lastCamera = map?.cameraPosition ?: lastCamera
        lifecycle?.removeObserver(observer)
        lifecycle = null
        pause(view)
        stop(view)
        view.onDestroy()
        view().removeView(view)
        mapView = null
        map = null
        // Overlays belong to the destroyed map; they are recreated from the specs on the next attach.
        markers.clear()
        markerKeys.clear()
        polylines.clear()
    }

    private fun start(view: MapView) {
        if (started) return
        started = true
        view.onStart()
    }

    private fun resume(view: MapView) {
        start(view)
        if (resumed) return
        resumed = true
        view.onResume()
    }

    private fun pause(view: MapView) {
        if (!resumed) return
        resumed = false
        view.onPause()
    }

    private fun stop(view: MapView) {
        pause(view)
        if (!started) return
        started = false
        view.onStop()
    }

    // MARK: - Map state

    // `my_location` needs the location permission; without it the SecurityException is logged.
    @SuppressLint("MissingPermission")
    private fun applySettings() {
        val map = map ?: return
        map.uiSettings.isZoomGesturesEnabled = gestures[Gesture.ZOOM] ?: true
        map.uiSettings.isScrollGesturesEnabled = gestures[Gesture.SCROLL] ?: true
        map.uiSettings.isTiltGesturesEnabled = gestures[Gesture.TILT] ?: true
        map.uiSettings.isRotateGesturesEnabled = gestures[Gesture.ROTATE] ?: true
        map.uiSettings.isMyLocationButtonEnabled = false
        map.uiSettings.isMapToolbarEnabled = false
        map.setPadding(px(paddingDp[0]), px(paddingDp[1]), px(paddingDp[2]), px(paddingDp[3]))
        try {
            map.isMyLocationEnabled = myLocation
        } catch (_: SecurityException) {
            Log.w(TAG, "my location needs the location permission")
        }
    }

    private fun applyBackground() {
        val color = backgroundColor ?: Color.TRANSPARENT
        view().setBackgroundColor(color)
        mapView?.setBackgroundColor(color)
    }

    private fun applyStyle() {
        val map = map ?: return
        try {
            map.setMapStyle(if (styleJson.isEmpty()) null else MapStyleOptions(styleJson))
        } catch (error: RuntimeException) {
            Log.w(TAG, "invalid map style: ${error.javaClass.simpleName}")
        }
    }

    private fun syncMarkers() {
        val map = map ?: return
        val seen = mutableSetOf<String>()
        for (spec in markerSpecs) {
            val id = spec.optString("id").takeIf { it.isNotEmpty() } ?: continue
            val position = latLng(spec.optJSONObject("position")) ?: continue
            seen += id
            val icon = spec.optJSONObject("icon") ?: JSONObject()
            val key = icon.toString()
            val anchor = spec.optJSONArray("anchor")
            val anchorU = anchor?.optDouble(0, 0.5)?.toFloat() ?: 0.5f
            val anchorV = anchor?.optDouble(1, 1.0)?.toFloat() ?: 1f
            val existing = markers[id]
            if (existing == null) {
                val marker = map.addMarker(
                    MarkerOptions()
                        .position(position)
                        .title(spec.optNullableString("title"))
                        .snippet(spec.optNullableString("snippet"))
                        .anchor(anchorU, anchorV)
                        .zIndex(spec.optDouble("zIndex", 0.0).toFloat())
                        .icon(descriptor(icon)),
                ) ?: continue
                marker.tag = id
                markers[id] = marker
                markerKeys[id] = key
            } else {
                existing.position = position
                existing.title = spec.optNullableString("title")
                existing.snippet = spec.optNullableString("snippet")
                existing.setAnchor(anchorU, anchorV)
                existing.zIndex = spec.optDouble("zIndex", 0.0).toFloat()
                if (markerKeys[id] != key) {
                    markerKeys[id] = key
                    existing.setIcon(descriptor(icon))
                }
            }
        }
        markers.keys.filter { it !in seen }.forEach { id ->
            markers.remove(id)?.remove()
            markerKeys.remove(id)
        }
    }

    private fun syncPolylines() {
        val map = map ?: return
        val seen = mutableSetOf<String>()
        for (spec in polylineSpecs) {
            val id = spec.optString("id").takeIf { it.isNotEmpty() } ?: continue
            seen += id
            val points = spec.optJSONArray("points")?.let { array ->
                (0 until array.length()).mapNotNull { latLng(array.optJSONObject(it)) }
            }.orEmpty()
            val color = argb(spec, "color", 0xFF0000FF.toInt())
            val width = px(spec.optDouble("width", 4.0).toFloat()).toFloat()
            val zIndex = spec.optDouble("zIndex", 0.0).toFloat()
            val tappable = spec.optBoolean("tappable")
            val geodesic = spec.optBoolean("geodesic")
            val existing = polylines[id]
            if (existing == null) {
                val polyline = map.addPolyline(
                    PolylineOptions().addAll(points).color(color).width(width).zIndex(zIndex)
                        .clickable(tappable).geodesic(geodesic),
                )
                polyline.tag = id
                polylines[id] = polyline
            } else {
                existing.points = points
                existing.color = color
                existing.width = width
                existing.zIndex = zIndex
                existing.isClickable = tappable
                existing.isGeodesic = geodesic
            }
        }
        polylines.keys.filter { it !in seen }.forEach { id -> polylines.remove(id)?.remove() }
    }

    // MARK: - Icons

    private fun descriptor(icon: JSONObject): BitmapDescriptor = when (icon.optString("type")) {
        "image" -> imageIcon(icon) ?: BitmapDescriptorFactory.defaultMarker()
        "circle" -> BitmapDescriptorFactory.fromBitmap(circleIcon(icon))
        else -> BitmapDescriptorFactory.defaultMarker(((icon.optDouble("hue", 0.0) % 360 + 360) % 360).toFloat())
    }

    private fun imageIcon(icon: JSONObject): BitmapDescriptor? {
        val source = BitmapFactory.decodeFile(icon.optString("path")) ?: return null
        val width = max(1, px(icon.optDouble("size", 32.0).toFloat()))
        val height = max(1, (width.toLong() * source.height / max(1, source.width)).toInt())
        return BitmapDescriptorFactory.fromBitmap(Bitmap.createScaledBitmap(source, width, height, true))
    }

    private fun circleIcon(icon: JSONObject): Bitmap {
        val side = max(1, px(icon.optDouble("size", 24.0).toFloat()))
        val strokeWidth = max(0, px(icon.optDouble("strokeWidth", 0.0).toFloat())).toFloat()
        val bitmap = Bitmap.createBitmap(side, side, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val radius = (side - strokeWidth) / 2f
        val center = side / 2f
        canvas.drawCircle(center, center, radius, Paint(Paint.ANTI_ALIAS_FLAG).apply { color = argb(icon, "fill", -1) })
        if (strokeWidth > 0) {
            canvas.drawCircle(
                center,
                center,
                radius,
                Paint(Paint.ANTI_ALIAS_FLAG).apply {
                    style = Paint.Style.STROKE
                    this.strokeWidth = strokeWidth
                    color = argb(icon, "stroke", 0)
                },
            )
        }
        val glyph = icon.optNullableString("glyph")
        if (!glyph.isNullOrEmpty()) {
            val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                color = argb(icon, "glyphColor", -1)
                textSize = side * 0.55f
                textAlign = Paint.Align.CENTER
                typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
            }
            val baseline = center - (paint.descent() + paint.ascent()) / 2f
            canvas.drawText(glyph, center, baseline, paint)
        }
        return bitmap
    }

    // MARK: - Helpers

    private fun emit(name: String, params: Map<String, Any?> = emptyMap()) {
        WhiskerCustomEvent.dispatch(ui = this, name = name, params = params)
    }

    private fun px(dp: Float): Int = (dp * resources.displayMetrics.density).toInt()

    private fun parse(json: String): JSONObject? = try {
        if (json.isEmpty()) null else JSONObject(json)
    } catch (_: Exception) {
        null
    }

    private fun list(json: String): List<JSONObject> = try {
        if (json.isEmpty()) emptyList() else JSONArray(json).let { array -> (0 until array.length()).mapNotNull { array.optJSONObject(it) } }
    } catch (_: Exception) {
        emptyList()
    }

    private fun latLng(value: JSONObject?): LatLng? {
        value ?: return null
        val latitude = value.optDouble("latitude")
        val longitude = value.optDouble("longitude")
        if (latitude.isNaN() || longitude.isNaN() || latitude !in -90.0..90.0) return null
        return LatLng(latitude, longitude)
    }

    private fun camera(value: JSONObject): CameraPosition? {
        val target = latLng(value.optJSONObject("target")) ?: return null
        return CameraPosition(
            target,
            value.optDouble("zoom", 2.0).toFloat(),
            value.optDouble("tilt", 0.0).toFloat().coerceIn(0f, 90f),
            value.optDouble("bearing", 0.0).toFloat(),
        )
    }

    /** `0xAARRGGBB` (a JSON number up to 2^32 - 1) as an Android color int. */
    private fun argb(value: JSONObject, key: String, fallback: Int): Int =
        if (value.has(key)) value.optLong(key, fallback.toLong()).toInt() else fallback

    private fun JSONObject.optNullableString(key: String): String? = if (isNull(key)) null else optString(key)

    private fun findActivity(context: Context): Activity? {
        var current: Context? = context
        while (current is ContextWrapper) {
            if (current is Activity) return current
            current = current.baseContext
        }
        return null
    }

    private companion object {
        const val TAG = "MasamuneGoogleMaps"
    }
}
