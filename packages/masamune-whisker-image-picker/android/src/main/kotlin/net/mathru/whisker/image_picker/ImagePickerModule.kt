// Image picking (photo library or camera). Scales the long side down to the given size and saves it as a PNG.

package net.mathru.whisker.image_picker

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.FileProvider
import androidx.exifinterface.media.ExifInterface
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger

/** Dedicated class to avoid duplicate entries in the manifest. */
class ImagePickerFileProvider : FileProvider()

@WhiskerModule
class ImagePickerModule : Module() {
    private val counter = AtomicInteger(0)
    private val worker = Executors.newSingleThreadExecutor()

    override fun definition() = ModuleDefinition {
        Name("ImagePicker")

        // pickImage({ source: "gallery" | "camera", maxSize }) -> { path } | null (cancelled)
        AsyncFunction("pickImage") { args, promise ->
            val json = Bridge.args(args)
            val activity = appContext.currentActivity as? ComponentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            val maxSize = json.optInt("maxSize", 512)
            val source = json.optString("source", "gallery")
            Bridge.onMain {
                if (source == "camera") {
                    val dir = File(activity.cacheDir, "masamune_whisker_image_picker").apply { mkdirs() }
                    val target = File(dir, "camera_${System.currentTimeMillis()}.jpg")
                    val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.masamune_whisker_image_picker.fileprovider", target)
                    var launcher: ActivityResultLauncher<Uri>? = null
                    launcher = activity.activityResultRegistry.register(key(), ActivityResultContracts.TakePicture()) { ok ->
                        launcher?.unregister()
                        if (ok) process(activity, uri, maxSize, promise) else Bridge.resolve(promise, null)
                    }
                    launcher.launch(uri)
                } else {
                    var launcher: ActivityResultLauncher<PickVisualMediaRequest>? = null
                    launcher = activity.activityResultRegistry.register(key(), ActivityResultContracts.PickVisualMedia()) { uri ->
                        launcher?.unregister()
                        if (uri != null) process(activity, uri, maxSize, promise) else Bridge.resolve(promise, null)
                    }
                    launcher.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                }
            }
        }
    }

    private fun key() = "masamune_whisker_image_picker_${counter.incrementAndGet()}"

    private fun process(activity: ComponentActivity, uri: Uri, maxSize: Int, promise: WhiskerPromise) {
        worker.execute {
            try {
                val resolver = activity.contentResolver
                val decoded = resolver.openInputStream(uri).use { BitmapFactory.decodeStream(it) }
                    ?: throw IllegalStateException("File not found.")
                val rotation = resolver.openInputStream(uri).use { stream ->
                    if (stream == null) 0 else when (ExifInterface(stream).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL)) {
                        ExifInterface.ORIENTATION_ROTATE_90 -> 90
                        ExifInterface.ORIENTATION_ROTATE_180 -> 180
                        ExifInterface.ORIENTATION_ROTATE_270 -> 270
                        else -> 0
                    }
                }
                val rotated = if (rotation == 0) decoded else {
                    val matrix = android.graphics.Matrix().apply { postRotate(rotation.toFloat()) }
                    Bitmap.createBitmap(decoded, 0, 0, decoded.width, decoded.height, matrix, true)
                }
                val longest = maxOf(rotated.width, rotated.height)
                val scaled = if (longest <= maxSize) rotated else {
                    val ratio = maxSize.toFloat() / longest
                    Bitmap.createScaledBitmap(rotated, (rotated.width * ratio).toInt().coerceAtLeast(1), (rotated.height * ratio).toInt().coerceAtLeast(1), true)
                }
                val dir = File(activity.cacheDir, "masamune_whisker_image_picker").apply { mkdirs() }
                val file = File(dir, "image_${System.currentTimeMillis()}.png")
                FileOutputStream(file).use { scaled.compress(Bitmap.CompressFormat.PNG, 100, it) }
                Bridge.resolve(promise, mapOf("path" to file.absolutePath, "width" to scaled.width, "height" to scaled.height))
            } catch (error: Throwable) {
                Bridge.reject(promise, error)
            }
        }
    }
}
