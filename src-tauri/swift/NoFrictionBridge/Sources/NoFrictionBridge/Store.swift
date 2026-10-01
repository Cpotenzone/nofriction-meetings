// StoreKit 2 bridge for the Mac App Store build (compiled only with
// -DNF_STOREKIT, i.e. `cargo build --features mas`). On-device verification
// only; there is no noFriction server.
//
// Products (subscription group "noFriction Pro"):
//   com.nofriction.meetings.pro.monthly, com.nofriction.meetings.pro.yearly

#if NF_STOREKIT
import Foundation
import StoreKit

let nfProProductIDs: Set<String> = [
    "com.nofriction.meetings.pro.monthly",
    "com.nofriction.meetings.pro.yearly",
]

@available(macOS 12.0, *)
private func periodText(_ p: Product.SubscriptionPeriod) -> (String, Int) {
    let unit: String
    switch p.unit {
    case .day: unit = "day"
    case .week: unit = "week"
    case .month: unit = "month"
    case .year: unit = "year"
    @unknown default: unit = "period"
    }
    return (unit, p.value)
}

@available(macOS 12.0, *)
private func periodPhrase(_ p: Product.SubscriptionPeriod) -> String {
    let (unit, value) = periodText(p)
    return value == 1 ? "1 \(unit)" : "\(value) \(unit)s"
}

@available(macOS 12.0, *)
private func introText(_ offer: Product.SubscriptionOffer) -> String {
    let span = offer.periodCount > 1
        ? "\(offer.periodCount) × \(periodPhrase(offer.period))"
        : periodPhrase(offer.period)
    switch offer.paymentMode {
    case .freeTrial:
        return "\(span) free"
    case .payAsYouGo:
        return "\(offer.displayPrice) per \(periodPhrase(offer.period)) for \(offer.periodCount) periods"
    case .payUpFront:
        return "\(offer.displayPrice) for \(span)"
    default:
        return "Introductory offer: \(offer.displayPrice) for \(span)"
    }
}

@available(macOS 12.0, *)
private func productJSON(_ p: Product) async -> [String: Any] {
    var out: [String: Any] = [
        "id": p.id,
        "displayName": p.displayName,
        "description": p.description,
        "displayPrice": p.displayPrice,
    ]
    if let sub = p.subscription {
        let (unit, value) = periodText(sub.subscriptionPeriod)
        out["periodUnit"] = unit
        out["periodValue"] = value
        out["period"] = periodPhrase(sub.subscriptionPeriod)
        if let intro = sub.introductoryOffer {
            let eligible = await sub.isEligibleForIntroOffer
            if eligible {
                out["introOffer"] = introText(intro)
            }
        }
    }
    return out
}

/// Current entitlement from `Transaction.currentEntitlements`.
@available(macOS 12.0, *)
func nfEntitlement() async -> [String: Any] {
    var best: Transaction?
    for await result in Transaction.currentEntitlements {
        guard case .verified(let t) = result,
              nfProProductIDs.contains(t.productID),
              t.revocationDate == nil
        else { continue }
        if let exp = t.expirationDate, exp < Date() { continue }
        if best == nil || (t.expirationDate ?? .distantFuture) > (best?.expirationDate ?? .distantPast) {
            best = t
        }
    }
    guard let t = best else {
        return ["ok": true, "isPro": false]
    }
    var out: [String: Any] = ["ok": true, "isPro": true, "productId": t.productID]
    if let exp = t.expirationDate {
        out["expiration"] = nfISO.string(from: exp)
    }
    // Auto-renew status lives on the subscription status, not the transaction
    if let product = try? await Product.products(for: [t.productID]).first,
       let statuses = try? await product.subscription?.status {
        for status in statuses {
            if case .verified(let info) = status.renewalInfo, info.currentProductID == t.productID || info.originalTransactionID == t.originalID {
                out["willRenew"] = info.willAutoRenew
                break
            }
        }
    }
    return out
}

/// Products for the given ids (JSON array string).
@_cdecl("nf_store_products")
public func nf_store_products(_ idsJson: UnsafePointer<CChar>, _ ctx: UnsafeMutableRawPointer?, _ cb: NFCallback) {
    let context = NFContext(ptr: ctx, cb: cb)
    let raw = String(cString: idsJson)
    let ids = (try? JSONSerialization.jsonObject(with: Data(raw.utf8)) as? [String]) ?? Array(nfProProductIDs)
    Task.detached {
        do {
            let products = try await Product.products(for: ids)
            var list: [[String: Any]] = []
            for p in products.sorted(by: { $0.price < $1.price }) {
                list.append(await productJSON(p))
            }
            context.send(["ok": true, "products": list])
        } catch {
            context.fail(error)
        }
    }
}

/// Buy a product. Result `status`: purchased | cancelled | pending, plus the
/// fresh entitlement on success.
@_cdecl("nf_store_purchase")
public func nf_store_purchase(_ productId: UnsafePointer<CChar>, _ ctx: UnsafeMutableRawPointer?, _ cb: NFCallback) {
    let context = NFContext(ptr: ctx, cb: cb)
    let id = String(cString: productId)
    Task.detached {
        do {
            guard nfProProductIDs.contains(id),
                  let product = try await Product.products(for: [id]).first
            else {
                context.send(["ok": false, "error": "Product \(id) is not available"])
                return
            }
            let result = try await product.purchase()
            switch result {
            case .success(let verification):
                switch verification {
                case .verified(let t):
                    await t.finish()
                    var out = await nfEntitlement()
                    out["status"] = "purchased"
                    context.send(out)
                case .unverified(_, let err):
                    context.send(["ok": false, "error": "Purchase could not be verified: \(err)"])
                }
            case .userCancelled:
                context.send(["ok": true, "status": "cancelled"])
            case .pending:
                // Ask to Buy / SCA: completes later via Transaction.updates
                context.send(["ok": true, "status": "pending"])
            @unknown default:
                context.send(["ok": true, "status": "unknown"])
            }
        } catch {
            context.fail(error)
        }
    }
}

@_cdecl("nf_store_entitlement")
public func nf_store_entitlement(_ ctx: UnsafeMutableRawPointer?, _ cb: NFCallback) {
    let context = NFContext(ptr: ctx, cb: cb)
    Task.detached {
        context.send(await nfEntitlement())
    }
}

/// Restore Purchases: `AppStore.sync()` (may show an App Store sign-in),
/// then the fresh entitlement.
@_cdecl("nf_store_restore")
public func nf_store_restore(_ ctx: UnsafeMutableRawPointer?, _ cb: NFCallback) {
    let context = NFContext(ptr: ctx, cb: cb)
    Task.detached {
        do {
            try await AppStore.sync()
            context.send(await nfEntitlement())
        } catch {
            context.fail(error)
        }
    }
}

private final class NFListener: @unchecked Sendable {
    static var task: Task<Void, Never>?
}

/// Start the `Transaction.updates` listener (once per launch). `cb(nil, json)`
/// receives the fresh entitlement after every update (renewal, refund,
/// purchase on another device, Ask to Buy approval).
@_cdecl("nf_store_start_listener")
public func nf_store_start_listener(_ cb: NFCallback) {
    guard NFListener.task == nil else { return }
    let context = NFContext(ptr: nil, cb: cb)
    NFListener.task = Task.detached(priority: .background) {
        for await update in Transaction.updates {
            if case .verified(let t) = update {
                await t.finish()
            }
            context.send(await nfEntitlement())
        }
    }
}
#endif
