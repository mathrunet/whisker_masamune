// Reverse geocoding (Android, android.location.Geocoder). Android 13+ uses the asynchronous
// listener API; earlier versions call the blocking API on a background thread.

package net.mathru.whisker.geocoding

import android.location.Address
import android.location.Geocoder
import android.os.Build
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import java.util.Locale
import java.util.concurrent.Executors

@WhiskerModule
class GeocodingModule : Module() {
    private val io = Executors.newSingleThreadExecutor()

    override fun definition() = ModuleDefinition {
        Name("Geocoding")

        Function("isAvailable") { _ ->
            Bridge.ok(Geocoder.isPresent())
        }

        // reverseGeocode({ latitude, longitude, locale }) -> [placemark]
        AsyncFunction("reverseGeocode") { args, promise ->
            val json = Bridge.args(args)
            val context = WhiskerApplication.appContext ?: return@AsyncFunction Bridge.reject(promise, "no context")
            if (!Geocoder.isPresent()) return@AsyncFunction Bridge.reject(promise, "no geocoder on this device")
            val tag = json.optString("locale").replace('_', '-')
            val locale = if (tag.isEmpty()) Locale.getDefault() else Locale.forLanguageTag(tag)
            val geocoder = Geocoder(context, locale)
            val latitude = json.optDouble("latitude")
            val longitude = json.optDouble("longitude")
            if (Build.VERSION.SDK_INT >= 33) {
                geocoder.getFromLocation(
                    latitude,
                    longitude,
                    MAX_RESULTS,
                    object : Geocoder.GeocodeListener {
                        override fun onGeocode(addresses: MutableList<Address>) {
                            Bridge.resolve(promise, addresses.map(::payload))
                        }

                        override fun onError(errorMessage: String?) {
                            Bridge.reject(promise, errorMessage ?: "geocoding failed")
                        }
                    },
                )
            } else {
                io.execute {
                    try {
                        @Suppress("DEPRECATION")
                        val addresses = geocoder.getFromLocation(latitude, longitude, MAX_RESULTS).orEmpty()
                        Bridge.resolve(promise, addresses.map(::payload))
                    } catch (error: Throwable) {
                        Bridge.reject(promise, error)
                    }
                }
            }
        }
    }

    private fun payload(address: Address): Map<String, Any?> = mapOf(
        "name" to (address.featureName ?: address.getAddressLine(0)),
        "street" to address.thoroughfare,
        "subThoroughfare" to address.subThoroughfare,
        "subLocality" to address.subLocality,
        "locality" to address.locality,
        "subAdministrativeArea" to address.subAdminArea,
        "administrativeArea" to address.adminArea,
        "postalCode" to address.postalCode,
        "country" to address.countryName,
        "isoCountryCode" to address.countryCode,
    )

    private companion object {
        const val MAX_RESULTS = 5
    }
}
