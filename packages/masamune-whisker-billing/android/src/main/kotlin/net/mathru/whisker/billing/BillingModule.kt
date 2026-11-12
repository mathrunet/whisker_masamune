// Google Play Billing (subscriptions). Purchase results are sent to Rust via the "purchases" event.
//
// The wire contract is shared with the StoreKit 2 implementation on iOS (BillingModule.swift): products,
// offers and purchases are encoded with platform-neutral keys, and response codes are mapped to status strings.

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
    // Calls waiting for the connection in progress; startConnection again while connecting fails.
    private val connecting = Any()
    private var waiting: MutableList<Pair<WhiskerPromise, () -> Unit>>? = null

    private val listener = PurchasesUpdatedListener { result, purchases ->
        sendEvent(
            "purchases",
            Bridge.event(
                encodeResult(result) + mapOf("purchases" to (purchases ?: emptyList()).map(::encode)),
            ),
        )
    }

    override fun definition() = ModuleDefinition {
        Name("Billing")
        Events("purchases")

        // connect() -> true
        AsyncFunction("connect") { _, promise -> connect(promise) { Bridge.resolve(promise, true) } }

        // queryProducts({ ids: [...] }) -> [{ id, title, description, price, priceMicros, currency, period, offers }]
        AsyncFunction("queryProducts") { args, promise ->
            val ids = Bridge.args(args).optJSONArray("ids")
            val list = (0 until (ids?.length() ?: 0)).map { ids!!.getString(it) }
            connect(promise) {
                loadDetails(list, promise) { products -> Bridge.resolve(promise, products.map(::encodeDetails)) }
            }
        }

        // purchase({ productId, offerToken?, oldPurchaseToken? }) -> { status, code, message } of the launch.
        // The outcome arrives through the "purchases" event.
        AsyncFunction("purchase") { args, promise ->
            val json = Bridge.args(args)
            val activity = appContext.currentActivity
            if (activity == null) {
                Bridge.reject(promise, "no activity")
                return@AsyncFunction
            }
            val productId = json.getString("productId")
            connect(promise) {
                val launch = { product: ProductDetails ->
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
                        Bridge.resolve(promise, encodeResult(result))
                    }
                }
                val cached = details[productId]
                if (cached != null) {
                    launch(cached)
                } else {
                    loadDetails(listOf(productId), promise) { products ->
                        val product = products.firstOrNull { it.productId == productId }
                        if (product == null) Bridge.reject(promise, "product not found: $productId") else launch(product)
                    }
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
                        Bridge.reject(promise, "queryPurchases failed: ${result.responseCode} ${result.debugMessage}")
                    } else {
                        Bridge.resolve(promise, purchases.map(::encode))
                    }
                }
            }
        }

        // finish({ token, transactionId }) — acknowledges the purchase token.
        AsyncFunction("finish") { args, promise ->
            val token = Bridge.args(args).optString("token")
            if (token.isBlank()) {
                Bridge.reject(promise, "finish: token is empty")
                return@AsyncFunction
            }
            connect(promise) {
                client!!.acknowledgePurchase(
                    AcknowledgePurchaseParams.newBuilder().setPurchaseToken(token).build(),
                ) { result ->
                    if (result.responseCode == BillingClient.BillingResponseCode.OK) {
                        Bridge.resolve(promise, null)
                    } else {
                        Bridge.reject(promise, "acknowledge failed: ${result.responseCode} ${result.debugMessage}")
                    }
                }
            }
        }
    }

    private fun loadDetails(ids: List<String>, promise: WhiskerPromise, then: (List<ProductDetails>) -> Unit) {
        if (ids.isEmpty()) {
            then(emptyList())
            return
        }
        val params = QueryProductDetailsParams.newBuilder().setProductList(
            ids.map {
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
            val products = response.productDetailsList
            products.forEach { product -> details[product.productId] = product }
            then(products)
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
        synchronized(connecting) {
            waiting?.let {
                it.add(promise to then)
                return
            }
            waiting = mutableListOf(promise to then)
        }
        val created = current ?: BillingClient.newBuilder(context)
            .setListener(listener)
            .enablePendingPurchases(PendingPurchasesParams.newBuilder().enableOneTimeProducts().build())
            .build()
        client = created
        created.startConnection(object : BillingClientStateListener {
            override fun onBillingSetupFinished(result: BillingResult) {
                val callers = synchronized(connecting) { waiting.orEmpty().also { waiting = null } }
                if (result.responseCode == BillingClient.BillingResponseCode.OK) {
                    callers.forEach { (_, next) -> next() }
                } else {
                    val message = "billing setup failed: ${result.responseCode} ${result.debugMessage}"
                    callers.forEach { (waiter, _) -> Bridge.reject(waiter, message) }
                }
            }

            override fun onBillingServiceDisconnected() {
                Log.i("MasamuneWhiskerBilling", "billing service disconnected")
            }
        })
    }

    private fun encodeResult(result: BillingResult): Map<String, Any?> = mapOf(
        "status" to status(result.responseCode),
        "code" to result.responseCode,
        "message" to result.debugMessage,
    )

    private fun status(code: Int): String = when (code) {
        BillingClient.BillingResponseCode.OK -> "ok"
        BillingClient.BillingResponseCode.USER_CANCELED -> "user_canceled"
        BillingClient.BillingResponseCode.ITEM_ALREADY_OWNED -> "already_owned"
        BillingClient.BillingResponseCode.ITEM_NOT_OWNED -> "not_owned"
        BillingClient.BillingResponseCode.ITEM_UNAVAILABLE -> "item_unavailable"
        BillingClient.BillingResponseCode.BILLING_UNAVAILABLE -> "billing_unavailable"
        BillingClient.BillingResponseCode.SERVICE_UNAVAILABLE -> "service_unavailable"
        BillingClient.BillingResponseCode.SERVICE_DISCONNECTED -> "service_disconnected"
        BillingClient.BillingResponseCode.NETWORK_ERROR -> "network_error"
        BillingClient.BillingResponseCode.DEVELOPER_ERROR -> "developer_error"
        BillingClient.BillingResponseCode.FEATURE_NOT_SUPPORTED -> "not_supported"
        else -> "error"
    }

    private fun encode(purchase: Purchase): Map<String, Any?> {
        val orderId = purchase.orderId.orEmpty()
        return mapOf(
            "productId" to purchase.products.firstOrNull().orEmpty(),
            "transactionId" to orderId,
            // Renewals get order ids like "GPA.1234..0"; the part before ".." is shared by the whole subscription.
            "originalId" to (orderId.substringBefore("..").ifBlank { purchase.purchaseToken }),
            "token" to purchase.purchaseToken,
            "platform" to "android",
            "state" to when (purchase.purchaseState) {
                Purchase.PurchaseState.PURCHASED -> "purchased"
                Purchase.PurchaseState.PENDING -> "pending"
                else -> "unspecified"
            },
            "finished" to purchase.isAcknowledged,
            "autoRenewing" to purchase.isAutoRenewing,
            "purchaseTime" to purchase.purchaseTime,
            "expiresTime" to null,
        )
    }

    private fun encodePhase(phase: ProductDetails.PricingPhase): Map<String, Any?> = mapOf(
        "price" to phase.formattedPrice,
        "priceMicros" to phase.priceAmountMicros,
        "currency" to phase.priceCurrencyCode,
        "period" to phase.billingPeriod,
        "cycles" to phase.billingCycleCount,
        "recurrence" to when (phase.recurrenceMode) {
            ProductDetails.RecurrenceMode.INFINITE_RECURRING -> "infinite"
            ProductDetails.RecurrenceMode.FINITE_RECURRING -> "finite"
            ProductDetails.RecurrenceMode.NON_RECURRING -> "non_recurring"
            else -> "unknown"
        },
    )

    private fun encodeDetails(product: ProductDetails): Map<String, Any?> {
        val offers = product.subscriptionOfferDetails.orEmpty()
        // The regular price is the last phase of the base plan (the offer without an offer id).
        val regular = (offers.firstOrNull { it.offerId == null } ?: offers.firstOrNull())
            ?.pricingPhases?.pricingPhaseList?.lastOrNull()
        return mapOf(
            "id" to product.productId,
            "title" to product.name.ifBlank { product.title },
            "description" to product.description,
            "price" to (regular?.formattedPrice ?: product.oneTimePurchaseOfferDetails?.formattedPrice.orEmpty()),
            "priceMicros" to (regular?.priceAmountMicros ?: product.oneTimePurchaseOfferDetails?.priceAmountMicros ?: 0L),
            "currency" to (regular?.priceCurrencyCode ?: product.oneTimePurchaseOfferDetails?.priceCurrencyCode.orEmpty()),
            "period" to regular?.billingPeriod,
            "offers" to offers.map { offer ->
                val phases = offer.pricingPhases.pricingPhaseList
                mapOf(
                    "kind" to (if (offer.offerId == null) "base" else "developer"),
                    "id" to offer.offerId,
                    "basePlanId" to offer.basePlanId,
                    "token" to offer.offerToken,
                    "freeTrial" to phases.any { it.priceAmountMicros == 0L },
                    "phases" to phases.map(::encodePhase),
                )
            },
        )
    }
}
