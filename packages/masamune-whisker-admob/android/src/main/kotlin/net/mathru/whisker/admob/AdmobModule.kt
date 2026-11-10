// AdMob (banners and interstitial ads). The SDK is initialized when the first ad is loaded (same as the Flutter version).

package net.mathru.whisker.admob

import android.content.Context
import android.util.Log
import android.widget.FrameLayout
import com.google.android.gms.ads.AdListener
import com.google.android.gms.ads.AdRequest
import com.google.android.gms.ads.AdSize
import com.google.android.gms.ads.AdView
import com.google.android.gms.ads.FullScreenContentCallback
import com.google.android.gms.ads.LoadAdError
import com.google.android.gms.ads.MobileAds
import com.google.android.gms.ads.interstitial.InterstitialAd
import com.google.android.gms.ads.interstitial.InterstitialAdLoadCallback
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerContext
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerUI
import rs.whisker.runtime.WhiskerValue

internal object AdmobSdk {
    private var initialized = false
    private var initializing = false
    private val waiters = mutableListOf<Pair<() -> Unit, (Throwable) -> Unit>>()

    fun ensureInitialized(context: Context, onReady: () -> Unit, onFailed: (Throwable) -> Unit) {
        Bridge.onMain {
            if (initialized) {
                try {
                    onReady()
                } catch (error: Exception) {
                    onFailed(error)
                }
            } else {
                waiters.add(onReady to onFailed)
                if (!initializing) {
                    initializing = true
                    try {
                        MobileAds.initialize(context.applicationContext) {
                            Bridge.onMain {
                                initialized = true
                                initializing = false
                                Log.i("MasamuneWhiskerAdmob", "SDK initialized")
                                val pending = waiters.toList()
                                waiters.clear()
                                pending.forEach { (ready, failed) ->
                                    try {
                                        ready()
                                    } catch (error: Exception) {
                                        failed(error)
                                    }
                                }
                            }
                        }
                    } catch (error: Exception) {
                        initializing = false
                        Log.w("MasamuneWhiskerAdmob", "SDK initialization failed", error)
                        val pending = waiters.toList()
                        waiters.clear()
                        pending.forEach { it.second(error) }
                    }
                }
            }
        }
    }

    /** Size of an anchored adaptive banner sized to the screen width. */
    fun bannerSize(context: Context, widthDp: Int): AdSize =
        AdSize.getCurrentOrientationAnchoredAdaptiveBannerAdSize(context, widthDp)
}

@WhiskerModule
class AdmobModule : Module() {
    override fun definition() = ModuleDefinition {
        Name("Admob")

        // bannerHeight({ widthDp }) -> height (dp)
        Function("bannerHeight") { args ->
            try {
                val context = WhiskerApplication.appContext ?: throw IllegalStateException("no context")
                val width = Bridge.args(args).optInt("widthDp", 360)
                Bridge.ok(AdmobSdk.bannerSize(context, width).height)
            } catch (error: Throwable) {
                Bridge.err(error)
            }
        }

        // showInterstitial({ adUnitId }) — loads and shows the ad, resolving once it is closed.
        AsyncFunction("showInterstitial") { args, promise ->
            val activity = appContext.currentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            val adUnitId = Bridge.args(args).optString("adUnitId")
            Bridge.onMain {
                AdmobSdk.ensureInitialized(activity.applicationContext, onReady = {
                    InterstitialAd.load(
                        activity,
                        adUnitId,
                        AdRequest.Builder().build(),
                        object : InterstitialAdLoadCallback() {
                            override fun onAdLoaded(ad: InterstitialAd) {
                                ad.fullScreenContentCallback = object : FullScreenContentCallback() {
                                    override fun onAdDismissedFullScreenContent() {
                                        Bridge.resolve(promise, true)
                                    }

                                    override fun onAdFailedToShowFullScreenContent(error: com.google.android.gms.ads.AdError) {
                                        Bridge.reject(promise, error.message)
                                    }
                                }
                                ad.show(activity)
                            }

                            override fun onAdFailedToLoad(error: LoadAdError) {
                                // Treat no fill as an expected outcome.
                                Bridge.resolve(promise, false)
                            }
                        },
                    )
                }, onFailed = { Bridge.reject(promise, it) })
            }
        }

        // requestTrackingAuthorization() — App Tracking Transparency only exists on iOS, so Android reports "authorized".
        AsyncFunction("requestTrackingAuthorization") { _, promise ->
            Bridge.resolve(promise, "authorized")
        }

        View("masamune-whisker-admob:Banner", AdBannerView::class.java) {
            Prop("ad-unit-id") { view: AdBannerView, value -> view.setAdUnit(value.asString() ?: "") }
            Prop("width-dp") { view: AdBannerView, value -> view.setWidthDp(value.asString()?.toIntOrNull() ?: value.asInt()?.toInt() ?: 0) }
            Events("loaded", "failed")
        }
    }
}

class AdBannerView(context: WhiskerContext) : WhiskerUI<FrameLayout>(context) {
    private val lifecycle = BannerLifecycle()
    private var adView: AdView? = null
    private val retryHandler = android.os.Handler(android.os.Looper.getMainLooper())
    private var retryTask: Runnable? = null

    override fun createView(context: Context): FrameLayout = FrameLayout(context)

    fun setAdUnit(value: String) {
        if (lifecycle.configure(adUnitId = value)) restart()
    }

    fun setWidthDp(value: Int) {
        if (lifecycle.configure(widthDp = value)) restart()
    }

    private fun cancelRetry() {
        retryTask?.let { retryHandler.removeCallbacks(it) }
        retryTask = null
    }

    private fun releaseAd() {
        adView?.adListener = object : AdListener() {}
        adView?.destroy()
        adView = null
        view().removeAllViews()
    }

    private fun restart() {
        cancelRetry()
        releaseAd()
        load()
    }

    private fun load() {
        val token = lifecycle.beginLoad() ?: return
        AdmobSdk.ensureInitialized(context.applicationContext, onReady = {
            if (!lifecycle.isCurrent(token)) return@ensureInitialized
            try {
                val ad = AdView(context)
                adView = ad
                ad.adUnitId = lifecycle.adUnitId
                ad.setAdSize(AdmobSdk.bannerSize(context, lifecycle.widthDp))
                ad.adListener = object : AdListener() {
                    override fun onAdLoaded() {
                        if (adView !== ad || !lifecycle.loaded(token)) return
                        cancelRetry()
                        emitWhiskerEvent("loaded")
                    }

                    override fun onAdFailedToLoad(error: LoadAdError) {
                        if (adView !== ad || !lifecycle.isCurrent(token)) return
                        failed(token, error.message, error.code)
                    }
                }
                view().addView(ad, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
                ad.loadAd(AdRequest.Builder().build())
            } catch (error: Exception) {
                failed(token, error.message ?: error.javaClass.simpleName)
            }
        }, onFailed = { error ->
            if (lifecycle.isCurrent(token)) failed(token, error.message ?: error.javaClass.simpleName)
        })
    }

    private fun failed(token: Long, message: String, code: Int? = null) {
        if (!lifecycle.isCurrent(token)) return
        val retryDelay = lifecycle.failed(token)
        releaseAd()
        Log.i("MasamuneWhiskerAdmob", "banner failed: code=$code, retryMs=$retryDelay, message=$message")
        emitWhiskerEvent("failed", WhiskerValue.Map(mapOf("message" to WhiskerValue.Str(message))))
        if (retryDelay != null && lifecycle.isCurrent(token)) {
            cancelRetry()
            retryTask = Runnable {
                retryTask = null
                if (lifecycle.isCurrent(token)) load()
            }.also { retryHandler.postDelayed(it, retryDelay) }
        }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        lifecycle.attach()
        restart()
    }

    override fun onDetachedFromWindow() {
        lifecycle.detach()
        cancelRetry()
        releaseAd()
        super.onDetachedFromWindow()
    }
}
