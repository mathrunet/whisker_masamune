// Google Play Billing (subscriptions). Purchase results are sent to Rust via the "purchases" event.

package net.mathru.whisker.billing

import android.util.Log
import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.PendingPurchasesParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.PurchasesUpdatedListener
import com.android.billingclient.api.QueryProductDetailsParams
import com.android.billingclient.api.QueryPurchasesParams
import rs.whisker.runtime.Module
import rs.whisker.runtime.ModuleDefinition
import rs.whisker.runtime.WhiskerApplication
import rs.whisker.runtime.WhiskerModule
import rs.whisker.runtime.WhiskerPromise

@WhiskerModule
class BillingModule : Module() {
    private var client: BillingClient? = null
    private val details = mutableMapOf<String, ProductDetails>()

    private val listener = PurchasesUpdatedListener { result, purchases ->
        sendEvent(
            "purchases",
            Bridge.event(
                mapOf(
                    "responseCode" to result.responseCode,
                    "debugMessage" to result.debugMessage,
                    "purchases" to (purchases ?: emptyList()).map(::encode),
                ),
            ),
        )
    }

    override fun definition() = ModuleDefinition {
        Name("Billing")
        Events("purchases")

        // connect() -> true
        AsyncFunction("connect") { _, promise -> connect(promise) { Bridge.resolve(promise, true) } }

        // queryProducts({ ids: [...] }) -> [{ productId, title, offers: [...] }]
        AsyncFunction("queryProducts") { args, promise ->
            val ids = Bridge.args(args).optJSONArray("ids")
            val list = (0 until (ids?.length() ?: 0)).map { ids!!.getString(it) }
            connect(promise) {
                val params = QueryProductDetailsParams.newBuilder().setProductList(
                    list.map {
                        QueryProductDetailsParams.Product.newBuilder()
                            .setProductId(it)
                            .setProductType(BillingClient.ProductType.SUBS)
                            .build()
                    },
                ).build()
                client!!.queryProductDetailsAsync(params) { result, response ->
                    if (result.responseCode != BillingClient.BillingResponseCode.OK) {
                        Bridge.reject(promise, "queryProducts failed: ${result.responseCode} ${result.debugMessage}")
                        return@queryProductDetailsAsync
                    }
                    val products: List<ProductDetails> = response
                    products.forEach { product -> details[product.productId] = product }
                    Bridge.resolve(promise, products.map(::encodeDetails))
                }
            }
        }

        // purchase({ productId, offerToken?, oldPurchaseToken? }) -> launch response code
        AsyncFunction("purchase") { args, promise ->
            val json = Bridge.args(args)
            val activity = appContext.currentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            connect(promise) {
                val product = details[json.getString("productId")]
                if (product == null) {
                    Bridge.reject(promise, "product details not loaded")
                    return@connect
                }
                val offers = product.subscriptionOfferDetails.orEmpty()
                val offerToken = json.optString("offerToken").ifBlank { null }
                    ?: offers.firstOrNull()?.offerToken
                val productParams = BillingFlowParams.ProductDetailsParams.newBuilder()
                    .setProductDetails(product)
                    .apply { if (offerToken != null) setOfferToken(offerToken) }
                    .build()
                val builder = BillingFlowParams.newBuilder().setProductDetailsParamsList(listOf(productParams))
                val old = json.optString("oldPurchaseToken")
                if (old.isNotBlank()) {
                    builder.setSubscriptionUpdateParams(
                        BillingFlowParams.SubscriptionUpdateParams.newBuilder()
                            .setOldPurchaseToken(old)
                            .setSubscriptionReplacementMode(
                                BillingFlowParams.SubscriptionUpdateParams.ReplacementMode.WITH_TIME_PRORATION,
                            )
                            .build(),
                    )
                }
                Bridge.onMain {
                    val result = client!!.launchBillingFlow(activity, builder.build())
                    Bridge.resolve(promise, mapOf("responseCode" to result.responseCode, "debugMessage" to result.debugMessage))
                }
            }
        }

        // queryPurchases() -> current subscriptions
        AsyncFunction("queryPurchases") { _, promise ->
            connect(promise) {
                client!!.queryPurchasesAsync(
                    QueryPurchasesParams.newBuilder().setProductType(BillingClient.ProductType.SUBS).build(),
                ) { result, purchases ->
                    if (result.responseCode != BillingClient.BillingResponseCode.OK) {
                        Bridge.reject(promise, "queryPurchases failed: ${result.responseCode}")
                    } else {
                        Bridge.resolve(promise, purchases.map(::encode))
                    }
                }
            }
        }

        // acknowledge({ purchaseToken })
        AsyncFunction("acknowledge") { args, promise ->
            val token = Bridge.args(args).getString("purchaseToken")
            connect(promise) {
                client!!.acknowledgePurchase(
                    AcknowledgePurchaseParams.newBuilder().setPurchaseToken(token).build(),
                ) { result -> Bridge.resolve(promise, result.responseCode) }
            }
        }
    }

    private fun connect(promise: WhiskerPromise, then: () -> Unit) {
        val current = client
        if (current != null && current.isReady) {
            then()
            return
        }
        val context = WhiskerApplication.appContext
        if (context == null) {
            Bridge.reject(promise, "no context")
            return
        }
        val created = current ?: BillingClient.newBuilder(context)
            .setListener(listener)
            .enablePendingPurchases(PendingPurchasesParams.newBuilder().enableOneTimeProducts().build())
            .build()
        client = created
        created.startConnection(object : BillingClientStateListener {
            override fun onBillingSetupFinished(result: BillingResult) {
                if (result.responseCode == BillingClient.BillingResponseCode.OK) {
                    then()
                } else {
                    Bridge.reject(promise, "billing setup failed: ${result.responseCode} ${result.debugMessage}")
                }
            }

            override fun onBillingServiceDisconnected() {
                Log.i("MasamuneWhiskerBilling", "billing service disconnected")
            }
        })
    }

    private fun encode(purchase: Purchase): Map<String, Any?> = mapOf(
        "productIds" to purchase.products,
        "purchaseToken" to purchase.purchaseToken,
        "orderId" to purchase.orderId,
        "purchaseState" to purchase.purchaseState,
        "acknowledged" to purchase.isAcknowledged,
        "purchaseTime" to purchase.purchaseTime,
        "autoRenewing" to purchase.isAutoRenewing,
    )

    private fun encodeDetails(product: ProductDetails): Map<String, Any?> = mapOf(
        "productId" to product.productId,
        "title" to product.title,
        "name" to product.name,
        "offers" to product.subscriptionOfferDetails.orEmpty().map { offer ->
            mapOf(
                "offerToken" to offer.offerToken,
                "basePlanId" to offer.basePlanId,
                "offerId" to offer.offerId,
                "phases" to offer.pricingPhases.pricingPhaseList.map { phase ->
                    mapOf(
                        "formattedPrice" to phase.formattedPrice,
                        "priceAmountMicros" to phase.priceAmountMicros,
                        "priceCurrencyCode" to phase.priceCurrencyCode,
                        "billingPeriod" to phase.billingPeriod,
                        "billingCycleCount" to phase.billingCycleCount,
                        "recurrenceMode" to phase.recurrenceMode,
                    )
                },
            )
        },
    )
}
