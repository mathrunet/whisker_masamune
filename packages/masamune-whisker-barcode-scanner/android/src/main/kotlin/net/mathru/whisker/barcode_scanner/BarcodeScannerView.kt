// Scanner element of masamune-whisker-barcode-scanner (Android), registered by BarcodeScannerModule.
//
// A CameraX PreviewView shows the camera; an ImageAnalysis use case hands the luminance plane of the latest frame to
// ZXing's QRCodeReader on a single background thread. The use cases are bound to the host activity's lifecycle while
// the element is attached and `active` is true, and unbound when either stops being the case (a Host reparent, which
// detaches and attaches in the same frame, keeps the camera running).
//
// WhiskerUI calls createView() from its constructor, before the properties of this class are initialized, so
// createView() only builds the PreviewView.

package net.mathru.whisker.barcode_scanner

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Color
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import androidx.camera.core.Camera
import androidx.camera.core.CameraSelector
import androidx.camera.core.CameraState
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.Observer
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.LuminanceSource
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.ReaderException
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.qrcode.QRCodeReader
import rs.whisker.runtime.WhiskerAppContext
import rs.whisker.runtime.WhiskerContext
import rs.whisker.runtime.WhiskerCustomEvent
import rs.whisker.runtime.WhiskerUI
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

class BarcodeScannerView(context: WhiskerContext) : WhiskerUI<PreviewView>(context) {
    companion object {
        const val DEFAULT_DEBOUNCE_MS = 1500L
    }

    private val mainHandler = Handler(Looper.getMainLooper())

    private var requestPermission = true
    private var front = false
    private var debounceMs = DEFAULT_DEBOUNCE_MS
    private var torch = false
    private var active = false

    /** Whether the camera should run; [generation] invalidates callbacks of an earlier start. */
    private var running = false
    private var generation = 0

    private var provider: ProcessCameraProvider? = null
    private var camera: Camera? = null
    private var preview: Preview? = null
    private var analysis: ImageAnalysis? = null
    private var analysisExecutor: ExecutorService? = null
    private var stateObserver: Observer<CameraState>? = null

    private var lastValue: String? = null
    private var lastSeen = 0L

    override fun createView(context: Context): PreviewView = PreviewView(context).apply {
        // TextureView-backed, so the preview clips and stacks like any other view in the Whisker tree.
        implementationMode = PreviewView.ImplementationMode.COMPATIBLE
        scaleType = PreviewView.ScaleType.FILL_CENTER
        setBackgroundColor(Color.BLACK)
    }

    // ---- props ------------------------------------------------------------------------------

    fun setRequestPermission(request: Boolean) {
        requestPermission = request
    }

    fun setFacing(value: String) {
        val wantsFront = value == "front"
        if (wantsFront == front) return
        front = wantsFront
        if (running) {
            stop()
            update()
        }
    }

    fun setDebounceMs(value: Long) {
        debounceMs = value.coerceAtLeast(0)
    }

    fun setTorch(on: Boolean) {
        torch = on
        applyTorch()
    }

    fun setActive(value: Boolean) {
        active = value
        update()
    }

    // ---- lifecycle --------------------------------------------------------------------------

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        update()
    }

    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        mainHandler.post { if (!isAttachedToWindow) update() }
    }

    private fun update() {
        val shouldRun = active && isAttachedToWindow
        if (shouldRun && !running) {
            start()
        } else if (!shouldRun && running) {
            stop()
        }
    }

    private fun start() {
        running = true
        lastValue = null
        val gen = ++generation
        val activity = activity()
        val status = CameraPermissions.status(context, activity)
        when {
            status == "authorized" -> {
                emit("permission", mapOf("status" to status))
                bind(gen)
            }
            status == "notDetermined" && requestPermission -> CameraPermissions.request(activity) { result ->
                if (gen != generation || !running) return@request
                emit("permission", mapOf("status" to result))
                if (result == "authorized") bind(gen)
            }
            else -> emit("permission", mapOf("status" to status))
        }
    }

    private fun stop() {
        running = false
        generation++
        unbind()
    }

    // ---- camera -----------------------------------------------------------------------------

    private fun bind(gen: Int) {
        val owner = activity() as? LifecycleOwner
        if (owner == null) {
            emitError("configuration", "no lifecycle owner")
            return
        }
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({
            if (gen != generation || !running) return@addListener
            val cameraProvider = try {
                future.get()
            } catch (error: Throwable) {
                emitError("unavailable", error.message ?: "camera provider unavailable")
                return@addListener
            }
            provider = cameraProvider
            val selector = if (front) CameraSelector.DEFAULT_FRONT_CAMERA else CameraSelector.DEFAULT_BACK_CAMERA
            val hasCamera = try {
                cameraProvider.hasCamera(selector)
            } catch (_: Throwable) {
                false
            }
            if (!hasCamera) {
                emitError("unavailable", "no camera is available")
                return@addListener
            }
            unbind()
            val executor = Executors.newSingleThreadExecutor()
            val newPreview = Preview.Builder().build()
            newPreview.setSurfaceProvider(view().surfaceProvider)
            val newAnalysis = ImageAnalysis.Builder()
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
            newAnalysis.setAnalyzer(executor, QrAnalyzer { value -> mainHandler.post { report(gen, value) } })
            try {
                val bound = cameraProvider.bindToLifecycle(owner, selector, newPreview, newAnalysis)
                camera = bound
                preview = newPreview
                analysis = newAnalysis
                analysisExecutor = executor
                val observer = Observer<CameraState> { state ->
                    val error = state.error ?: return@Observer
                    if (gen != generation) return@Observer
                    val code = when (error.code) {
                        CameraState.ERROR_CAMERA_IN_USE, CameraState.ERROR_MAX_CAMERAS_IN_USE -> "interrupted"
                        CameraState.ERROR_CAMERA_DISABLED -> "unavailable"
                        else -> "configuration"
                    }
                    emitError(code, "camera state error ${error.code}")
                }
                bound.cameraInfo.cameraState.observe(owner, observer)
                stateObserver = observer
                applyTorch()
            } catch (error: Throwable) {
                executor.shutdown()
                emitError("configuration", error.message ?: "camera could not be bound")
            }
        }, ContextCompat.getMainExecutor(context))
    }

    private fun unbind() {
        val bound = camera
        if (bound != null) {
            if (bound.cameraInfo.hasFlashUnit()) bound.cameraControl.enableTorch(false)
            stateObserver?.let { bound.cameraInfo.cameraState.removeObserver(it) }
        }
        stateObserver = null
        analysis?.clearAnalyzer()
        val useCases = listOfNotNull(preview, analysis)
        if (useCases.isNotEmpty()) provider?.unbind(*useCases.toTypedArray())
        analysisExecutor?.shutdown()
        analysisExecutor = null
        preview = null
        analysis = null
        camera = null
    }

    private fun applyTorch() {
        val bound = camera ?: return
        if (!bound.cameraInfo.hasFlashUnit()) return
        bound.cameraControl.enableTorch(torch && running)
    }

    // ---- events -----------------------------------------------------------------------------

    /** Emits a decoded code unless it repeats the last one within the window (every sighting restarts it). */
    private fun report(gen: Int, value: String) {
        if (gen != generation || !running) return
        val now = SystemClock.elapsedRealtime()
        val repeated = value == lastValue && now - lastSeen < debounceMs
        lastValue = value
        lastSeen = now
        if (repeated) return
        emit("scan", mapOf("value" to value, "format" to "qr"))
    }

    private fun emitError(code: String, message: String) {
        emit("error", mapOf("code" to code, "message" to message))
    }

    private fun emit(name: String, params: Map<String, Any?>) {
        WhiskerCustomEvent.dispatch(ui = this, name = name, params = params)
    }

    private fun activity(): Activity? {
        var current: Context? = context
        while (current is ContextWrapper) {
            if (current is Activity) return current
            current = current.baseContext
        }
        return WhiskerAppContext.shared.currentActivity
    }
}

/**
 * Decodes QR codes from the luminance (Y) plane of each frame with ZXing. Codes are rotation invariant, so the frame
 * is read as delivered. Every other frame is also tried inverted, for light codes on a dark background.
 */
internal class QrAnalyzer(private val onCode: (String) -> Unit) : ImageAnalysis.Analyzer {
    private val reader = QRCodeReader()
    private val hints = mapOf(DecodeHintType.TRY_HARDER to true)
    private var frame = 0L

    override fun analyze(image: ImageProxy) {
        try {
            val plane = image.planes[0]
            val buffer = plane.buffer
            val data = ByteArray(buffer.remaining())
            buffer.get(data)
            val rowStride = plane.rowStride
            val width = image.width
            val height = image.height
            if (rowStride < width || data.size < (height - 1) * rowStride + width) return
            val source = PlanarYUVLuminanceSource(data, rowStride, height, 0, 0, width, height, false)
            val value = decode(source) ?: if (frame % 2 == 1L) decode(source.invert()) else null
            frame++
            if (!value.isNullOrEmpty()) onCode(value)
        } catch (_: Throwable) {
        } finally {
            image.close()
        }
    }

    private fun decode(source: LuminanceSource): String? = try {
        reader.decode(BinaryBitmap(HybridBinarizer(source)), hints).text
    } catch (_: ReaderException) {
        null
    } finally {
        reader.reset()
    }
}
