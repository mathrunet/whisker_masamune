// Makes bundled fonts available by name from whisker's Text.
//
// whisker's Android Host resolves fonts with `Typeface.create(familyName, NORMAL)`
// (rs.whisker.runtime.resolveWhiskerTypeface). This only looks up the system font table
// `Typeface.sSystemFontMap`, so fonts registered in this table at startup are used by
// `font_family("Name")` for both rendering and measurement.

package net.mathru.whisker.fonts

import android.content.Context
import android.graphics.Typeface
import android.graphics.fonts.Font
import android.graphics.fonts.FontFamily
import android.os.Build
import android.util.Log
import org.json.JSONObject
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

@WhiskerModule
class FontsModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("Fonts")

        // register({ families: [{ family, files: [{ path, weights }] }] }) -> array of registered family names
        Function("register") { args ->
            try {
                Bridge.ok(Fonts.register(Bridge.args(args)))
            } catch (error: Throwable) {
                Log.w(TAG, "font registration failed", error)
                Bridge.err(error)
            }
        }
    }

    private companion object {
        const val TAG = "MasamuneWhiskerFonts"
    }
}

internal object Fonts {
    private const val TAG = "MasamuneWhiskerFonts"
    private const val ASSET_PREFIX = "file:///android_asset/"

    private val registered = mutableSetOf<String>()

    @Synchronized
    fun register(json: JSONObject): List<String> {
        val context = WhiskerApplication.appContext
            ?: throw IllegalStateException("WhiskerApplication.appContext is not initialized")
        val systemMap = systemFontMap()
        val families = json.optJSONArray("families") ?: return emptyList()
        val done = mutableListOf<String>()
        for (i in 0 until families.length()) {
            val family = families.getJSONObject(i)
            val name = family.getString("family")
            if (name in registered) {
                done.add(name)
                continue
            }
            val filesJson = family.getJSONArray("files")
            val files = (0 until filesJson.length()).map { index ->
                val file = filesJson.getJSONObject(index)
                val weights = file.getJSONArray("weights")
                file.getString("path") to (0 until weights.length()).map { weights.getInt(it) }
            }
            val typeface = try {
                buildTypeface(context, files)
            } catch (error: Throwable) {
                Log.w(TAG, "failed to load $name", error)
                null
            } ?: continue
            systemMap[name] = typeface
            registered.add(name)
            done.add(name)
        }
        return done
    }

    private fun buildTypeface(context: Context, files: List<Pair<String, List<Int>>>): Typeface? {
        if (files.isEmpty()) return null
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            var builder: FontFamily.Builder? = null
            for ((path, weights) in files) {
                val buffer = read(context, path)
                for (weight in weights) {
                    val font = Font.Builder(buffer.duplicate().order(ByteOrder.nativeOrder()))
                        .setWeight(weight)
                        .setSlant(android.graphics.fonts.FontStyle.FONT_SLANT_UPRIGHT)
                        .build()
                    builder = builder?.also { it.addFont(font) } ?: FontFamily.Builder(font)
                }
            }
            val family = builder?.build() ?: return null
            return Typeface.CustomFallbackBuilder(family).build()
        }
        // API 28 and below use only one file (preferring the file that includes the regular weight 400).
        val path = files.firstOrNull { 400 in it.second }?.first ?: files.first().first
        return if (path.startsWith(ASSET_PREFIX)) {
            Typeface.createFromAsset(context.assets, path.removePrefix(ASSET_PREFIX))
        } else {
            Typeface.createFromFile(path.removePrefix("file://"))
        }
    }

    /** Reads a whisker-asset URI (`file:///android_asset/…`) or a file path. */
    private fun read(context: Context, path: String): ByteBuffer {
        val bytes = if (path.startsWith(ASSET_PREFIX)) {
            context.assets.open(path.removePrefix(ASSET_PREFIX)).use { it.readBytes() }
        } else {
            File(path.removePrefix("file://")).readBytes()
        }
        return ByteBuffer.allocateDirect(bytes.size).order(ByteOrder.nativeOrder()).also {
            it.put(bytes)
            it.flip()
        }
    }

    @Suppress("UNCHECKED_CAST", "DiscouragedPrivateApi", "SoonBlockedPrivateApi")
    private fun systemFontMap(): MutableMap<String, Typeface> {
        val field = Typeface::class.java.getDeclaredField("sSystemFontMap")
        field.isAccessible = true
        val current = field.get(null) as Map<String, Typeface>
        if (current is MutableMap<String, Typeface>) {
            try {
                // Check whether the Map is mutable (replace it if it is read-only).
                current.putAll(emptyMap())
                return current
            } catch (_: UnsupportedOperationException) {
            }
        }
        val copy = HashMap(current)
        field.set(null, copy)
        return copy
    }
}
