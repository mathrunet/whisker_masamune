// Photo library access (Android, MediaStore).
//
// - Assets are queried from MediaStore.Images and MediaStore.Video by DATE_TAKEN (DATE_ADDED when
//   missing), merged, sorted and paged. Ids are `image:<id>` / `video:<id>`.
// - Locations are read from the originals (MediaStore.setRequireOriginal + EXIF for images,
//   MediaMetadataRetriever for videos), which needs ACCESS_MEDIA_LOCATION on Android 10+.
// - Thumbnails use ContentResolver.loadThumbnail (Android 10+) or a sampled decode, and are
//   written as JPEG to <cacheDir>/masamune_whisker_photo_library.
//
// Android cannot tell "never asked" from "denied", so the module remembers that it asked.

package net.mathru.whisker.photo_library

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.ContentUris
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.provider.Settings
import android.util.Size
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.content.ContextCompat
import androidx.exifinterface.media.ExifInterface
import org.json.JSONArray
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise
import java.io.File
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.max
import kotlin.math.min

@WhiskerModule
class PhotoLibraryModule : Module() {
    private val io = Executors.newSingleThreadExecutor()
    private val requestCounter = AtomicInteger(0)

    override fun definition() = ModuleDefinition {
        Name("PhotoLibrary")

        Function("status") { _ ->
            Bridge.ok(WhiskerApplication.appContext?.let { status(it) } ?: "denied")
        }

        AsyncFunction("requestPermission") { _, promise -> request(promise, force = false) }

        AsyncFunction("presentLimitedPicker") { _, promise -> request(promise, force = true) }

        Function("openAppSettings") { _ ->
            try {
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                try {
                    activity.startActivity(
                        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}")),
                    )
                } catch (_: ActivityNotFoundException) {
                    activity.startActivity(Intent(Settings.ACTION_SETTINGS))
                }
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // openAsset({ id }) -> whether a viewer was opened (false for unknown ids or when no app can view it)
        Function("openAsset") { args ->
            try {
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                val (video, uri) = parseId(Bridge.args(args).optString("id")) ?: return@Function Bridge.ok(false)
                val intent = Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, if (video) "video/*" else "image/*")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                activity.startActivity(intent)
                Bridge.ok(true)
            } catch (_: ActivityNotFoundException) {
                Bridge.ok(false)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // listAssets({ fromMs, toMs, kinds, newestFirst, offset, limit, includeLocation }) -> [asset]
        AsyncFunction("listAssets") { args, promise ->
            val json = Bridge.args(args)
            val context = WhiskerApplication.appContext ?: return@AsyncFunction Bridge.reject(promise, "no context")
            io.execute {
                try {
                    Bridge.resolve(promise, list(context, json))
                } catch (error: Throwable) {
                    Bridge.reject(promise, error)
                }
            }
        }

        // thumbnail({ id, size, crop }) -> { path, width, height }
        AsyncFunction("thumbnail") { args, promise ->
            val json = Bridge.args(args)
            val context = WhiskerApplication.appContext ?: return@AsyncFunction Bridge.reject(promise, "no context")
            io.execute {
                try {
                    Bridge.resolve(promise, thumbnail(context, json.optString("id"), max(1, json.optInt("size", 256)), json.optBoolean("crop", true)))
                } catch (error: Throwable) {
                    Bridge.reject(promise, error)
                }
            }
        }

        // assetLocation({ id }) -> { latitude, longitude } | null
        AsyncFunction("assetLocation") { args, promise ->
            val id = Bridge.args(args).optString("id")
            val context = WhiskerApplication.appContext ?: return@AsyncFunction Bridge.reject(promise, "no context")
            io.execute {
                val location = parseId(id)?.let { (video, uri) -> location(context, uri, video) }
                Bridge.resolve(promise, location?.let { mapOf("latitude" to it.first, "longitude" to it.second) })
            }
        }
    }

    // MARK: - Permission

    private fun granted(context: Context, permission: String) =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED

    private fun readPermissions(): Array<String> = when {
        Build.VERSION.SDK_INT >= 34 -> arrayOf(
            Manifest.permission.READ_MEDIA_IMAGES,
            Manifest.permission.READ_MEDIA_VIDEO,
            Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED,
        )
        Build.VERSION.SDK_INT >= 33 -> arrayOf(Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.READ_MEDIA_VIDEO)
        else -> arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE)
    }

    private fun status(context: Context): String {
        val full = if (Build.VERSION.SDK_INT >= 33) {
            granted(context, Manifest.permission.READ_MEDIA_IMAGES) || granted(context, Manifest.permission.READ_MEDIA_VIDEO)
        } else {
            granted(context, Manifest.permission.READ_EXTERNAL_STORAGE)
        }
        return when {
            full -> "granted"
            Build.VERSION.SDK_INT >= 34 && granted(context, Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED) -> "limited"
            !context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(ASKED, false) -> "notDetermined"
            else -> "denied"
        }
    }

    /** Requests read access (and the media location permission). `force` asks again when limited. */
    private fun request(promise: WhiskerPromise, force: Boolean) {
        val context = WhiskerApplication.appContext
        val activity = appContext.currentActivity as? ComponentActivity
        if (context == null || activity == null) {
            Bridge.reject(promise, "no activity")
            return
        }
        val current = status(context)
        if (current == "granted" || (!force && current == "limited")) {
            Bridge.resolve(promise, current)
            return
        }
        Bridge.onMain {
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(ASKED, true).apply()
            launch(activity, readPermissions()) {
                if (Build.VERSION.SDK_INT >= 29 && status(context) != "denied" &&
                    !granted(context, Manifest.permission.ACCESS_MEDIA_LOCATION)
                ) {
                    launch(activity, arrayOf(Manifest.permission.ACCESS_MEDIA_LOCATION)) {
                        Bridge.resolve(promise, status(context))
                    }
                } else {
                    Bridge.resolve(promise, status(context))
                }
            }
        }
    }

    private fun launch(activity: ComponentActivity, permissions: Array<String>, done: () -> Unit) {
        val key = "masamune_whisker_photo_library_permission_${requestCounter.incrementAndGet()}"
        var launcher: ActivityResultLauncher<Array<String>>? = null
        launcher = activity.activityResultRegistry.register(key, ActivityResultContracts.RequestMultiplePermissions()) {
            launcher?.unregister()
            done()
        }
        launcher.launch(permissions)
    }

    // MARK: - Listing

    private class Row(val id: String, val uri: Uri, val video: Boolean, val created: Long, val width: Int, val height: Int, val duration: Long)

    private fun list(context: Context, json: JSONObject): List<Map<String, Any?>> {
        val status = status(context)
        if (status == "denied" || status == "notDetermined") return emptyList()
        val kinds = json.optJSONArray("kinds") ?: JSONArray().put("image").put("video")
        val wanted = (0 until kinds.length()).map { kinds.optString(it) }.toSet()
        val from = if (json.isNull("fromMs") || !json.has("fromMs")) null else json.optLong("fromMs")
        val to = if (json.isNull("toMs") || !json.has("toMs")) null else json.optLong("toMs")
        val newestFirst = json.optBoolean("newestFirst", true)
        val offset = max(0, json.optInt("offset", 0))
        val limit = max(0, json.optInt("limit", 100))
        if (limit == 0) return emptyList()
        val rows = mutableListOf<Row>()
        if ("image" in wanted) rows += query(context, false, from, to)
        if ("video" in wanted) rows += query(context, true, from, to)
        val sorted = if (newestFirst) rows.sortedByDescending { it.created } else rows.sortedBy { it.created }
        val includeLocation = json.optBoolean("includeLocation")
        return sorted.drop(offset).take(limit).map { row ->
            val location = if (includeLocation) location(context, row.uri, row.video) else null
            mapOf(
                "id" to row.id,
                "kind" to if (row.video) "video" else "image",
                "createdAtMs" to row.created,
                "width" to row.width,
                "height" to row.height,
                "durationMs" to row.duration,
                "latitude" to location?.first,
                "longitude" to location?.second,
            )
        }
    }

    private fun query(context: Context, video: Boolean, from: Long?, to: Long?): List<Row> {
        val collection = if (video) MediaStore.Video.Media.EXTERNAL_CONTENT_URI else MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        // Plain columns only: MediaStore rejects SQL expressions in projections on Android 11+.
        // DATE_TAKEN is in milliseconds and may be missing; DATE_ADDED is in seconds.
        val taken = MediaStore.MediaColumns.DATE_TAKEN
        val added = MediaStore.MediaColumns.DATE_ADDED
        val projection = arrayOf(
            MediaStore.MediaColumns._ID,
            taken,
            added,
            MediaStore.MediaColumns.WIDTH,
            MediaStore.MediaColumns.HEIGHT,
            if (video) MediaStore.Video.VideoColumns.DURATION else MediaStore.MediaColumns._ID,
        )
        // Coarse bounds in SQL (either date in range); the exact filter runs below.
        val conditions = mutableListOf<String>()
        val arguments = mutableListOf<String>()
        from?.let {
            conditions += "($taken >= ? OR $added >= ?)"
            arguments += listOf(it.toString(), (it / 1000).toString())
        }
        to?.let {
            conditions += "($taken < ? OR $added < ?)"
            arguments += listOf(it.toString(), ((it + 999) / 1000).toString())
        }
        val rows = mutableListOf<Row>()
        context.contentResolver.query(
            collection,
            projection,
            conditions.joinToString(" AND ").ifEmpty { null },
            arguments.toTypedArray(),
            null,
        )?.use { cursor ->
            while (cursor.moveToNext()) {
                val id = cursor.getLong(0)
                val created = if (!cursor.isNull(1) && cursor.getLong(1) > 0) cursor.getLong(1) else cursor.getLong(2) * 1000
                if (from != null && created < from) continue
                if (to != null && created >= to) continue
                rows += Row(
                    id = "${if (video) "video" else "image"}:$id",
                    uri = ContentUris.withAppendedId(collection, id),
                    video = video,
                    created = created,
                    width = cursor.getInt(3),
                    height = cursor.getInt(4),
                    duration = if (video) cursor.getLong(5) else 0L,
                )
            }
        }
        return rows
    }

    private fun parseId(id: String): Pair<Boolean, Uri>? {
        val (kind, raw) = id.split(':', limit = 2).takeIf { it.size == 2 }?.let { it[0] to it[1] } ?: return null
        val number = raw.toLongOrNull() ?: return null
        return when (kind) {
            "image" -> false to ContentUris.withAppendedId(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, number)
            "video" -> true to ContentUris.withAppendedId(MediaStore.Video.Media.EXTERNAL_CONTENT_URI, number)
            else -> null
        }
    }

    // MARK: - Location

    private fun location(context: Context, uri: Uri, video: Boolean): Pair<Double, Double>? = try {
        val original = if (Build.VERSION.SDK_INT >= 29 && granted(context, Manifest.permission.ACCESS_MEDIA_LOCATION)) {
            MediaStore.setRequireOriginal(uri)
        } else {
            uri
        }
        if (video) {
            MediaMetadataRetriever().run {
                try {
                    setDataSource(context, original)
                    parseIso6709(extractMetadata(MediaMetadataRetriever.METADATA_KEY_LOCATION))
                } finally {
                    release()
                }
            }
        } else {
            context.contentResolver.openInputStream(original)?.use { stream ->
                ExifInterface(stream).latLong?.let { it[0] to it[1] }
            }
        }?.takeIf { (latitude, longitude) -> latitude != 0.0 || longitude != 0.0 }
    } catch (_: Exception) {
        null
    }

    /** `+35.6812+139.7671/` → (35.6812, 139.7671). */
    private fun parseIso6709(value: String?): Pair<Double, Double>? {
        val match = Regex("""([+-]\d+(?:\.\d+)?)([+-]\d+(?:\.\d+)?)""").find(value ?: return null) ?: return null
        val latitude = match.groupValues[1].toDoubleOrNull() ?: return null
        val longitude = match.groupValues[2].toDoubleOrNull() ?: return null
        return latitude to longitude
    }

    // MARK: - Thumbnails

    private fun thumbnail(context: Context, id: String, size: Int, crop: Boolean): Map<String, Any?> {
        val (video, uri) = parseId(id) ?: throw IllegalArgumentException("asset not found")
        val directory = File(context.cacheDir, DIRECTORY).apply { mkdirs() }
        val file = File(directory, "${id.replace(':', '_')}_${size}_${if (crop) "c" else "f"}.jpg")
        if (file.isFile && file.length() > 0) {
            val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            BitmapFactory.decodeFile(file.absolutePath, bounds)
            return mapOf("path" to file.absolutePath, "width" to bounds.outWidth, "height" to bounds.outHeight)
        }
        val source = load(context, uri, video, size) ?: throw IllegalStateException("image not available")
        val output = if (crop) centerCrop(source, size) else fit(source, size)
        val temporary = File(directory, "${file.name}.tmp")
        temporary.outputStream().use { output.compress(Bitmap.CompressFormat.JPEG, 85, it) }
        if (!temporary.renameTo(file)) throw IllegalStateException("cannot write the image")
        return mapOf("path" to file.absolutePath, "width" to output.width, "height" to output.height)
    }

    private fun load(context: Context, uri: Uri, video: Boolean, size: Int): Bitmap? {
        if (Build.VERSION.SDK_INT >= 29) {
            return context.contentResolver.loadThumbnail(uri, Size(size, size), null)
        }
        if (video) {
            return MediaMetadataRetriever().run {
                try {
                    setDataSource(context, uri)
                    getFrameAtTime(0)
                } finally {
                    release()
                }
            }
        }
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        context.contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        var sample = 1
        while (min(bounds.outWidth, bounds.outHeight) / (sample * 2) >= size) sample *= 2
        return context.contentResolver.openInputStream(uri)?.use {
            BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample })
        }
    }

    private fun centerCrop(source: Bitmap, side: Int): Bitmap {
        val square = min(source.width, source.height)
        val cropped = Bitmap.createBitmap(source, (source.width - square) / 2, (source.height - square) / 2, square, square)
        return if (square == side) cropped else Bitmap.createScaledBitmap(cropped, side, side, true)
    }

    private fun fit(source: Bitmap, maxSide: Int): Bitmap {
        val longest = max(source.width, source.height)
        if (longest <= maxSide) return source
        val scale = maxSide.toDouble() / longest
        return Bitmap.createScaledBitmap(
            source,
            max(1, (source.width * scale).toInt()),
            max(1, (source.height * scale).toInt()),
            true,
        )
    }

    private companion object {
        const val PREFS = "MasamuneWhiskerPhotoLibrary"
        const val ASKED = "asked"
        const val DIRECTORY = "masamune_whisker_photo_library"
    }
}
