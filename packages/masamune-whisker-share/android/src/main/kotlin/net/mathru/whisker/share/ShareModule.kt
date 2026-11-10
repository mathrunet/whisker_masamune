// Shares files and text via the OS share sheet.
//
// Given files are copied into this module's dedicated cache and shared through a dedicated FileProvider
// (files from any directory can be shared without conflicting with other libraries' FileProviders).

package net.mathru.whisker.share

import android.content.ClipData
import android.content.Intent
import androidx.core.content.FileProvider
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import java.io.File

/** Dedicated class to avoid duplicate entries in the manifest. */
class ShareFileProvider : FileProvider()

@WhiskerModule
class ShareModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("Share")

        // shareFile({ path, mimeType, subject, text }) — opens the OS share sheet.
        Function("shareFile") { args ->
            try {
                val json = Bridge.args(args)
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                val source = File(json.getString("path").removePrefix("file://"))
                val dir = File(activity.cacheDir, "masamune_whisker_share").apply { mkdirs() }
                val file = if (source.parentFile?.canonicalPath == dir.canonicalPath) source else source.copyTo(File(dir, source.name), overwrite = true)
                val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.masamune_whisker_share.fileprovider", file)
                val intent = Intent(Intent.ACTION_SEND).apply {
                    type = json.optString("mimeType", "application/octet-stream")
                    putExtra(Intent.EXTRA_STREAM, uri)
                    // Allow the share sheet to read the image even when sharing an image together with text.
                    clipData = ClipData.newUri(activity.contentResolver, source.name, uri)
                    json.optString("subject").takeIf { it.isNotBlank() }?.let { putExtra(Intent.EXTRA_TITLE, it) }
                    json.optString("subject").takeIf { it.isNotBlank() }?.let { putExtra(Intent.EXTRA_SUBJECT, it) }
                    json.optString("text").takeIf { it.isNotBlank() }?.let { putExtra(Intent.EXTRA_TEXT, it) }
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }
                activity.startActivity(Intent.createChooser(intent, json.optString("subject").ifBlank { null }))
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // shareText({ text, subject }) — shares text only.
        Function("shareText") { args ->
            try {
                val json = Bridge.args(args)
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                val intent = Intent(Intent.ACTION_SEND).apply {
                    type = "text/plain"
                    putExtra(Intent.EXTRA_TEXT, json.optString("text"))
                    json.optString("subject").takeIf { it.isNotBlank() }?.let { putExtra(Intent.EXTRA_SUBJECT, it) }
                }
                activity.startActivity(Intent.createChooser(intent, json.optString("subject").ifBlank { null }))
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }
    }
}
