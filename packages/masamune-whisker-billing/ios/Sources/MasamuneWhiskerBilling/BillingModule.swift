// In-app subscriptions with StoreKit 2. Shares the wire contract of `BillingModule.kt` (Google Play Billing):
// products, offers and purchases use the same keys, and purchase outcomes, `Transaction.updates` and
// purchases completed outside the app are sent to Rust through the "purchases" event.
//
// A purchase's `token` is the transaction's JWS (`jwsRepresentation`), which the server verifies.

import StoreKit
import UIKit
import WhiskerModule

@WhiskerModule
public final class BillingModule: Module {
    @MainActor private var store: BillingStore?

    public override func definition() -> ModuleDefinition {
        ModuleDefinition {
            Name("Billing")
            Events("purchases")

            // Start listening to Transaction.updates as soon as Rust subscribes to purchase notifications.
            OnStartObserving("purchases") { [weak self] in
                self?.run { store in store.startUpdates() }
            }

            // connect() -> true
            AsyncFunction("connect") { [weak self] _, promise in
                self?.run { store in
                    store.startUpdates()
                    Bridge.resolve(promise, true)
                }
            }

            // queryProducts({ ids: [...] }) -> [{ id, title, description, price, priceMicros, currency, period, offers }]
            AsyncFunction("queryProducts") { [weak self] args, promise in
                let ids = Bridge.args(args)["ids"] as? [String] ?? []
                self?.run { store in
                    do {
                        Bridge.resolve(promise, try await store.queryProducts(ids))
                    } catch {
                        Bridge.reject(promise, error)
                    }
                }
            }

            // purchase({ productId, offerToken?, oldPurchaseToken? }) -> { status, code, message }.
            // The offer token and the replaced purchase are Play-only: StoreKit applies introductory offers and
            // subscription-group upgrades itself. The outcome arrives through the "purchases" event.
            AsyncFunction("purchase") { [weak self] args, promise in
                let productId = Bridge.args(args)["productId"] as? String ?? ""
                self?.run { store in
                    do {
                        Bridge.resolve(promise, try await store.purchase(productId))
                    } catch {
                        Bridge.reject(promise, error)
                    }
                }
            }

            // queryPurchases() -> current entitlements
            AsyncFunction("queryPurchases") { [weak self] _, promise in
                self?.run { store in
                    Bridge.resolve(promise, await store.queryPurchases())
                }
            }

            // finish({ token, transactionId }) — finishes the transaction.
            AsyncFunction("finish") { [weak self] args, promise in
                let id = (Bridge.args(args)["transactionId"] as? String).flatMap { UInt64($0) }
                guard let id else {
                    Bridge.reject(promise, "finish: invalid transactionId")
                    return
                }
                self?.run { store in
                    await store.finish(id)
                    Bridge.resolve(promise, nil)
                }
            }
        }
    }

    /// Runs `body` on the main actor, where the store keeps its state.
    private func run(_ body: @escaping @MainActor (BillingStore) async -> Void) {
        Task { @MainActor [weak self] in
            guard let self else { return }
            let store = self.store ?? BillingStore { [weak self] payload in
                self?.sendEvent("purchases", Bridge.event(payload))
            }
            self.store = store
            await body(store)
        }
    }
}

/// StoreKit state (product cache, transactions seen but not finished, the updates listener). Main actor only.
@MainActor
final class BillingStore {
    private let emit: ([String: Any?]) -> Void
    private var products: [String: Product] = [:]
    private var transactions: [UInt64: Transaction] = [:]
    private var updates: Task<Void, Never>?

    init(emit: @escaping ([String: Any?]) -> Void) {
        self.emit = emit
    }

    // MARK: - Operations

    func startUpdates() {
        guard updates == nil else { return }
        updates = Task { [weak self] in
            for await result in Transaction.updates {
                guard let self else { return }
                await self.deliver(result)
            }
        }
    }

    func queryProducts(_ ids: [String]) async throws -> [[String: Any?]] {
        guard !ids.isEmpty else { return [] }
        let found = try await Product.products(for: ids)
        var encoded: [[String: Any?]] = []
        for product in found {
            products[product.id] = product
            encoded.append(await Self.encode(product))
        }
        return encoded
    }

    func purchase(_ productId: String) async throws -> [String: Any?] {
        let product: Product
        if let cached = products[productId] {
            product = cached
        } else if let fetched = try await Product.products(for: [productId]).first {
            products[productId] = fetched
            product = fetched
        } else {
            throw BillingError("product not found: \(productId)")
        }
        let result: Product.PurchaseResult
        do {
            if #available(iOS 17.0, *), let scene = Self.keyWindow()?.windowScene {
                result = try await product.purchase(confirmIn: scene)
            } else {
                result = try await product.purchase()
            }
        } catch {
            // The sheet could not be shown or the store rejected the request.
            return Self.result(error)
        }
        switch result {
        case .success(let verification):
            switch verification {
            case .verified(let transaction):
                transactions[transaction.id] = transaction
                let purchase = await encode(transaction, jws: verification.jwsRepresentation, finished: false)
                emit(Self.update("ok", purchases: [purchase]))
            case .unverified(_, let error):
                emit(Self.update("error", code: (error as NSError).code, message: "transaction failed verification: \(Bridge.message(error))"))
            }
        case .userCancelled:
            emit(Self.update("user_canceled"))
        case .pending:
            // Ask to Buy or Strong Customer Authentication; the transaction arrives later through Transaction.updates.
            emit(Self.update("ok", purchases: [[
                "productId": productId,
                "transactionId": "",
                "originalId": "",
                "token": "",
                "platform": "ios",
                "state": "pending",
                "finished": false,
                "autoRenewing": false,
                "purchaseTime": Int64(Date().timeIntervalSince1970 * 1000),
                "expiresTime": nil,
            ]]))
        @unknown default:
            emit(Self.update("error", message: "unknown purchase result"))
        }
        return ["status": "ok", "code": 0, "message": ""]
    }

    func queryPurchases() async -> [[String: Any?]] {
        let unfinished = await unfinishedIds()
        var purchases: [[String: Any?]] = []
        for await result in Transaction.currentEntitlements {
            guard case .verified(let transaction) = result else { continue }
            purchases.append(
                await encode(transaction, jws: result.jwsRepresentation, finished: !unfinished.contains(transaction.id))
            )
        }
        return purchases
    }

    func finish(_ id: UInt64) async {
        if let transaction = transactions.removeValue(forKey: id) {
            await transaction.finish()
            return
        }
        for await result in Transaction.unfinished {
            if case .verified(let transaction) = result, transaction.id == id {
                await transaction.finish()
                return
            }
        }
        // Not unfinished any more: already finished.
    }

    // MARK: - Updates

    private func deliver(_ result: VerificationResult<Transaction>) async {
        guard case .verified(let transaction) = result else {
            NSLog("MasamuneWhiskerBilling: ignored an unverified transaction update")
            return
        }
        let unfinished = await unfinishedIds()
        let finished = !unfinished.contains(transaction.id)
        if !finished { transactions[transaction.id] = transaction }
        emit(Self.update("ok", purchases: [await encode(transaction, jws: result.jwsRepresentation, finished: finished)]))
    }

    private func unfinishedIds() async -> Set<UInt64> {
        var ids = Set<UInt64>()
        for await result in Transaction.unfinished {
            if case .verified(let transaction) = result {
                ids.insert(transaction.id)
                transactions[transaction.id] = transaction
            }
        }
        return ids
    }

    // MARK: - Encoding

    private func encode(_ transaction: Transaction, jws: String, finished: Bool) async -> [String: Any?] {
        [
            "productId": transaction.productID,
            "transactionId": String(transaction.id),
            "originalId": String(transaction.originalID),
            "token": jws,
            "platform": "ios",
            "state": transaction.revocationDate == nil ? "purchased" : "revoked",
            "finished": finished,
            "autoRenewing": await Self.willAutoRenew(transaction),
            "purchaseTime": Self.millis(transaction.purchaseDate),
            "expiresTime": transaction.expirationDate.map(Self.millis),
        ]
    }

    /// Whether the subscription renews, from the renewal info of its subscription group.
    private static func willAutoRenew(_ transaction: Transaction) async -> Bool {
        guard transaction.productType == .autoRenewable, transaction.revocationDate == nil else { return false }
        if let group = transaction.subscriptionGroupID,
           let statuses = try? await Product.SubscriptionInfo.status(for: group) {
            for status in statuses {
                if case .verified(let renewal) = status.renewalInfo, renewal.originalTransactionID == transaction.originalID {
                    return renewal.willAutoRenew
                }
            }
        }
        return (transaction.expirationDate ?? .distantPast) > Date()
    }

    private static func encode(_ product: Product) async -> [String: Any?] {
        let currency = product.priceFormatStyle.currencyCode
        let period = product.subscription.map { iso($0.subscriptionPeriod) }
        let regular: [String: Any?] = [
            "price": product.displayPrice,
            "priceMicros": micros(product.price),
            "currency": currency,
            "period": period ?? "",
            "cycles": 0,
            "recurrence": product.subscription == nil ? "non_recurring" : "infinite",
        ]
        var offers: [[String: Any?]] = []
        if let subscription = product.subscription {
            offers.append(offer(kind: "base", freeTrial: false, phases: [regular]))
            // Like Play, list the introductory offer only when this user can still get it.
            if let intro = subscription.introductoryOffer, await subscription.isEligibleForIntroOffer {
                let phase: [String: Any?] = [
                    "price": intro.displayPrice,
                    "priceMicros": micros(intro.price),
                    "currency": currency,
                    "period": iso(intro.period),
                    "cycles": intro.periodCount,
                    "recurrence": intro.paymentMode == .payUpFront ? "non_recurring" : "finite",
                ]
                offers.append(offer(kind: "introductory", freeTrial: intro.paymentMode == .freeTrial, phases: [phase, regular]))
            }
        }
        return [
            "id": product.id,
            "title": product.displayName,
            "description": product.description,
            "price": product.displayPrice,
            "priceMicros": micros(product.price),
            "currency": currency,
            "period": period,
            "offers": offers,
        ]
    }

    private static func offer(kind: String, freeTrial: Bool, phases: [[String: Any?]]) -> [String: Any?] {
        ["kind": kind, "id": nil, "basePlanId": nil, "token": "", "freeTrial": freeTrial, "phases": phases]
    }

    /// ISO 8601 duration (`P1M`, `P1Y` …), the format Play reports billing periods in.
    private static func iso(_ period: Product.SubscriptionPeriod) -> String {
        switch period.unit {
        case .day: return "P\(period.value)D"
        case .week: return "P\(period.value)W"
        case .month: return "P\(period.value)M"
        case .year: return "P\(period.value)Y"
        @unknown default: return "P\(period.value)D"
        }
    }

    private static func micros(_ price: Decimal) -> Int64 {
        NSDecimalNumber(decimal: price * 1_000_000).int64Value
    }

    private static func millis(_ date: Date) -> Int64 {
        Int64(date.timeIntervalSince1970 * 1000)
    }

    // MARK: - Results

    private static func update(
        _ status: String, code: Int = 0, message: String = "", purchases: [[String: Any?]] = []
    ) -> [String: Any?] {
        ["status": status, "code": code, "message": message, "purchases": purchases]
    }

    /// Maps a StoreKit error to the platform-neutral status.
    private static func result(_ error: Error) -> [String: Any?] {
        let status: String
        if let error = error as? StoreKitError {
            switch error {
            case .userCancelled: status = "user_canceled"
            case .networkError: status = "network_error"
            case .notAvailableInStorefront: status = "item_unavailable"
            case .notEntitled: status = "not_owned"
            case .unsupported: status = "not_supported"
            // Includes cases newer SDKs add (for example a missing presentation context).
            default: status = "error"
            }
        } else if let error = error as? Product.PurchaseError {
            switch error {
            case .purchaseNotAllowed: status = "billing_unavailable"
            case .productUnavailable, .ineligibleForOffer: status = "item_unavailable"
            case .invalidQuantity, .invalidOfferIdentifier, .invalidOfferPrice, .invalidOfferSignature,
                 .missingOfferParameters:
                status = "developer_error"
            default: status = "error"
            }
        } else {
            status = "error"
        }
        return ["status": status, "code": (error as NSError).code, "message": Bridge.message(error)]
    }

    static func keyWindow() -> UIWindow? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let ordered = scenes.filter { $0.activationState == .foregroundActive } + scenes
        for scene in ordered {
            if let window = scene.windows.first(where: \.isKeyWindow) ?? scene.windows.first {
                return window
            }
        }
        return nil
    }
}

private struct BillingError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}
