// Container that renders the screen, including its children, as a PNG image. The capture({ fileName, heightDp? }) command saves it to the cache
// and emits a captured event ({ path } or { error }).

package net.mathru.whisker.view_capture

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Build
import android.widget.FrameLayout
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerContainerView
import rs.whisker.runtime.WhiskerContext
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerUI
import rs.whisker.runtime.WhiskerValue
import java.io.FileOutputStream

@WhiskerModule
class ViewCaptureModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("ViewCapture")

        View("masamune-whisker-view-capture:Capture", CaptureView::class.java) {
            Command("capture") { view: CaptureView, value: WhiskerValue ->
                view.capture(value)
            }
            Events("captured")
        }
    }
}

class CaptureView(context: WhiskerContext) : WhiskerUI<FrameLayout>(context) {
    override fun createView(context: Context): FrameLayout = FrameLayout(context)

    fun capture(params: WhiskerValue) {
        val map = (params as? WhiskerValue.Map)?.value ?: emptyMap()
        val fileName = map["fileName"]?.asString() ?: "capture.png"
        val requestedHeightDp = map["heightDp"]?.asDouble()
        val composited = map["composited"]?.asBool() ?: false
        post {
            try {
                val file = captureFile(context.cacheDir, fileName)
                // Whisker places the background and children in the native component's parent HostNode.
                // Drawing this CaptureView itself would capture only an empty FrameLayout.
                val target = parent as? WhiskerContainerView
                    ?: throw IllegalStateException("capture host is not attached")
                check(target.isLaidOut && target.width > 0 && target.height > 0) {
                    "capture host is not laid out"
                }
                val density = resources.displayMetrics.density
                val width = target.width
                val fullHeight = target.height
                val cropHeight = requestedHeightDp?.let { (it * density).toInt().coerceIn(1, fullHeight) } ?: fullHeight
                check(file.parentFile!!.isDirectory || file.parentFile!!.mkdirs()) { "capture directory unavailable" }
                val bitmap = Bitmap.createBitmap(width, cropHeight, Bitmap.Config.ARGB_8888)
                fun complete(error: Throwable?) {
                    try {
                        if (error != null) throw error
                        FileOutputStream(file).use {
                            check(bitmap.compress(Bitmap.CompressFormat.PNG, 100, it)) {
                                "capture PNG could not be saved"
                            }
                        }
                        emitWhiskerEvent("captured", WhiskerValue.Map(mapOf("path" to WhiskerValue.Str(file.absolutePath))))
                    } catch (failure: Throwable) {
                        emitWhiskerEvent("captured", WhiskerValue.Map(mapOf("error" to WhiskerValue.Str(failure.message ?: "capture failed"))))
                    } finally {
                        bitmap.recycle()
                    }
                }
                try {
                    if (composited && Build.VERSION.SDK_INT >= 26) {
                        captureComposited(target, bitmap, ::complete)
                    } else {
                        target.draw(Canvas(bitmap))
                        complete(null)
                    }
                } catch (error: Throwable) {
                    bitmap.recycle()
                    throw error
                }
            } catch (error: Throwable) {
                emitWhiskerEvent("captured", WhiskerValue.Map(mapOf("error" to WhiskerValue.Str(error.message ?: "capture failed"))))
            }
        }
    }
}
