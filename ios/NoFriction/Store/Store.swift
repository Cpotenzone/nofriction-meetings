import Foundation
import Observation
import StoreKit

/// noFriction Pro, verified on the device with StoreKit 2. No server, no bypass.
@MainActor
@Observable
final class Store {
    nonisolated static let monthlyID = "com.nofriction.meetings.pro.monthly"
    nonisolated static let yearlyID = "com.nofriction.meetings.pro.yearly"
    nonisolated static let productIDs: [String] = [yearlyID, monthlyID]

    private(set) var products: [Product] = []
    private(set) var entitledProductIDs: Set<String> = []
    private(set) var loadError: String?
    private(set) var isLoading = false

    var isPro: Bool { !entitledProductIDs.isEmpty }

    @ObservationIgnored private var updates: Task<Void, Never>?

    init() {
        // Start listening right away so renewals, refunds and Ask to Buy
        // approvals that arrive while the app runs are never missed.
        updates = Task.detached(priority: .background) { [weak self] in
            for await result in Transaction.updates {
                if case .verified(let t) = result { await t.finish() }
                await self?.refreshEntitlements()
            }
        }
        Task {
            await refreshEntitlements()
            await loadProducts()
        }
    }

    // Store lives for the app's lifetime; the listener ends with the process.

    func loadProducts() async {
        isLoading = true
        defer { isLoading = false }
        do {
            let loaded = try await Product.products(for: Self.productIDs)
            products = loaded.sorted { Self.productIDs.firstIndex(of: $0.id) ?? 0 < Self.productIDs.firstIndex(of: $1.id) ?? 0 }
            loadError = loaded.isEmpty ? "Subscriptions aren't available right now." : nil
        } catch {
            loadError = "Couldn't load subscriptions. Check your connection and try again."
        }
    }

    enum PurchaseOutcome { case purchased, pending, cancelled }

    func purchase(_ product: Product) async throws -> PurchaseOutcome {
        let result = try await product.purchase()
        switch result {
        case .success(let verification):
            guard case .verified(let transaction) = verification else {
                throw StoreError.unverified
            }
            await transaction.finish()
            await refreshEntitlements()
            return .purchased
        case .pending:
            return .pending
        case .userCancelled:
            return .cancelled
        @unknown default:
            return .cancelled
        }
    }

    /// Restore Purchases: sync with the App Store, then re-read entitlements.
    func restore() async throws {
        try await AppStore.sync()
        await refreshEntitlements()
    }

    func refreshEntitlements() async {
        var ids = Set<String>()
        for await result in Transaction.currentEntitlements {
            guard case .verified(let t) = result else { continue }
            if Self.grantsPro(productID: t.productID, revocationDate: t.revocationDate,
                              expirationDate: t.expirationDate, isUpgraded: t.isUpgraded) {
                ids.insert(t.productID)
            }
        }
        entitledProductIDs = ids
    }

    /// Entitlement rule (spec): a verified, unrevoked, unexpired transaction for a Pro product.
    nonisolated static func grantsPro(productID: String, revocationDate: Date?, expirationDate: Date?,
                                      isUpgraded: Bool, now: Date = .now) -> Bool {
        guard productIDs.contains(productID), revocationDate == nil, !isUpgraded else { return false }
        if let expirationDate, expirationDate <= now { return false }
        return true
    }

    enum StoreError: LocalizedError {
        case unverified
        var errorDescription: String? { "The App Store couldn't verify this purchase." }
    }
}

// MARK: - Display helpers (pure; unit-tested)

enum SubscriptionText {
    static func name(_ unit: Product.SubscriptionPeriod.Unit) -> String {
        switch unit {
        case .day: "day"
        case .week: "week"
        case .month: "month"
        case .year: "year"
        @unknown default: "period"
        }
    }

    /// Billing period: "month", "year", "3 months".
    static func period(_ unit: Product.SubscriptionPeriod.Unit, value: Int) -> String {
        if unit == .day && value == 7 { return "week" }
        return value == 1 ? name(unit) : "\(value) \(name(unit))s"
    }

    /// Length of an offer: "1 week", "3 days", "2 months".
    static func duration(_ unit: Product.SubscriptionPeriod.Unit, value: Int, periods: Int = 1) -> String {
        let total = value * max(periods, 1)
        if unit == .day && total % 7 == 0 {
            let w = total / 7
            return w == 1 ? "1 week" : "\(w) weeks"
        }
        return total == 1 ? "1 \(name(unit))" : "\(total) \(name(unit))s"
    }
}
