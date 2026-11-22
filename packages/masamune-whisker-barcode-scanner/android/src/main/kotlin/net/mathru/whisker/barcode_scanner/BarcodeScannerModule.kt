// Module definition of masamune-whisker-barcode-scanner (Android): the camera permission functions and the
// `masamune-whisker-barcode-scanner:Scanner` element (BarcodeScannerView.kt).

package net.mathru.whisker.barcode_scanner

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import java.util.concurrent.atomic.AtomicInteger

/**
 * Camera permission state as `authorized`, `denied` or `notDetermined`.
 *
 * Android does not report whether the user was asked, so the first request is remembered in SharedPreferences: before
 * it, and while the system would still show the prompt (`shouldShowRequestPermissionRationale`), the state is
 * `notDetermined`; after a refusal the prompt no longer appears and the state is `denied`.
 */
internal object CameraPermissions {
    private const val PREFS = "MasamuneWhiskerBarcodeScanner"
    private const val REQUESTED = "cameraRequested"
    private val requestCounter = AtomicInteger(0)

    private fun granted(context: Context): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED

    fun status(context: Context, activity: Activity?): String {
        if (granted(context)) return "authorized"
        val requested = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(REQUESTED, false)
        if (!requested) return "notDetermined"
        if (activity != null && ActivityCompat.shouldShowRequestPermissionRationale(activity, Manifest.permission.CAMERA)) {
            return "notDetermined"
        }
        return "denied"
    }

    /** Shows the system prompt when the state is `notDetermined`, then answers the resulting state on the main thread. */
    fun request(activity: Activity?, done: (String) -> Unit) {
        Bridge.onMain {
            val context = activity ?: WhiskerApplication.appContext
            if (context == null) {
                done("denied")
                return@onMain
            }
            val current = status(context, activity)
            val component = activity as? ComponentActivity
            if (current != "notDetermined" || component == null) {
                done(current)
                return@onMain
            }
            context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(REQUESTED, true).apply()
            val key = "masamune_whisker_barcode_scanner_permission_${requestCounter.incrementAndGet()}"
            var launcher: ActivityResultLauncher<String>? = null
            launcher = component.activityResultRegistry.register(key, ActivityResultContracts.RequestPermission()) {
                launcher?.unregister()
                done(status(context, component))
            }
            launcher.launch(Manifest.permission.CAMERA)
        }
    }
}

@WhiskerModule
class BarcodeScannerModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("BarcodeScanner")

        // permissionStatus() -> "authorized" | "denied" | "notDetermined"
        Function("permissionStatus") { _ ->
            val context = WhiskerApplication.appContext ?: return@Function Bridge.ok("denied")
            Bridge.ok(CameraPermissions.status(context, appContext.currentActivity))
        }

        // requestPermission() -> the state after the prompt (no prompt once decided)
        AsyncFunction("requestPermission") { _, promise ->
            CameraPermissions.request(appContext.currentActivity) { status -> Bridge.resolve(promise, status) }
        }

        // openSettings() — opens this app's page in the system settings.
        Function("openSettings") { _ ->
            try {
                val activity = appContext.currentActivity ?: throw IllegalStateException("no activity")
                activity.startActivity(
                    Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${activity.packageName}")),
                )
                Bridge.ok(true)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        View("masamune-whisker-barcode-scanner:Scanner", BarcodeScannerView::class.java) {
            // Configuration (declared before `torch` and `active` in the Rust schema).
            Prop("request-permission", clear = { view: BarcodeScannerView -> view.setRequestPermission(true) }) { view: BarcodeScannerView, value ->
                view.setRequestPermission(value.asBool() ?: value.asString()?.toBooleanStrictOrNull() ?: true)
            }
            Prop("facing", clear = { view: BarcodeScannerView -> view.setFacing("back") }) { view: BarcodeScannerView, value ->
                view.setFacing(value.asString() ?: "back")
            }
            Prop("debounce-ms", clear = { view: BarcodeScannerView -> view.setDebounceMs(BarcodeScannerView.DEFAULT_DEBOUNCE_MS) }) { view: BarcodeScannerView, value ->
                view.setDebounceMs(value.asString()?.toLongOrNull() ?: BarcodeScannerView.DEFAULT_DEBOUNCE_MS)
            }
            Prop("torch", clear = { view: BarcodeScannerView -> view.setTorch(false) }) { view: BarcodeScannerView, value ->
                view.setTorch(value.asBool() ?: value.asString()?.toBooleanStrictOrNull() ?: false)
            }
            Prop("active", clear = { view: BarcodeScannerView -> view.setActive(false) }) { view: BarcodeScannerView, value ->
                view.setActive(value.asBool() ?: value.asString()?.toBooleanStrictOrNull() ?: false)
            }

            Events("scan", "permission", "error")
        }
    }
}
