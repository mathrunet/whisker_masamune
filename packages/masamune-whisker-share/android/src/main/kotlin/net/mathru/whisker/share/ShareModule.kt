// OS の共有シートでファイルとテキストを送る。
//
// 渡されたファイルはこのモジュール専用のキャッシュへコピーしてから、専用の FileProvider で共有する
// （どのディレクトリのファイルでも共有でき、他のライブラリの FileProvider と衝突しない）。

package net.mathru.whisker.share

import android.content.ClipData
import android.content.Intent
import androidx.core.content.FileProvider
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule
import java.io.File

/** マニフェストでの重複を避けるための専用クラス。 */
class ShareFileProvider : FileProvider()

@WhiskerModule
class ShareModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("Share")

        // shareFile({ path, mimeType, subject, text }) — OS の共有シートを開く。
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
                    // 画像と文章を同時に共有するときも、共有シートから画像を読み取れるようにする。
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

        // shareText({ text, subject }) — テキストだけを共有する。
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
