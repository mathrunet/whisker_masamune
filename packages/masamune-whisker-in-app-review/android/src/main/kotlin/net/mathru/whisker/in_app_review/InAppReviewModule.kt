// In-app review (Google Play In-App Review). Opens the store page if unavailable.

package net.mathru.whisker.in_app_review

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import com.google.android.play.core.review.ReviewManagerFactory
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerModule

@WhiskerModule
class InAppReviewModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("InAppReview")

        // requestReview({ storeUrl }) -> true if requested in-app, false if the store was opened
        AsyncFunction("requestReview") { args, promise ->
            val activity = appContext.currentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            val storeUrl = Bridge.args(args).optString("storeUrl")
            val manager = ReviewManagerFactory.create(activity)
            manager.requestReviewFlow().addOnCompleteListener { request ->
                if (request.isSuccessful) {
                    manager.launchReviewFlow(activity, request.result).addOnCompleteListener {
                        Bridge.resolve(promise, true)
                    }
                } else {
                    if (storeUrl.isNotBlank()) {
                        try {
                            activity.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(storeUrl)))
                        } catch (_: ActivityNotFoundException) {
                        }
                    }
                    Bridge.resolve(promise, false)
                }
            }
        }
    }
}
