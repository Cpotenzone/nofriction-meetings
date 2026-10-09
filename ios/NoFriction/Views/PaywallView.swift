import StoreKit
import SwiftUI

/// Guideline 3.1.2: price, billing period, trial terms, Restore, Terms + Privacy.
struct PaywallView: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var purchasing: String?
    @State private var restoring = false
    @State private var message: String?
    @State private var trialEligible: [String: Bool] = [:]

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    header
                    features
                    plans
                    restore
                    finePrint
                }
                .padding(24)
                .frame(maxWidth: 560, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
            .background(Theme.background)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
        }
        .task {
            if store.products.isEmpty { await store.loadProducts() }
            for p in store.products {
                trialEligible[p.id] = await p.subscription?.isEligibleForIntroOffer ?? false
            }
        }
        .onChange(of: store.isPro) { _, pro in if pro { dismiss() } }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            Image(systemName: "sparkles")
                .font(.system(size: 38))
                .foregroundStyle(Theme.ai)
            Text("noFriction Pro").font(.largeTitle.weight(.bold))
            Text("Notes, a review guide and answers for every recording, with the AI you choose.")
                .foregroundStyle(.secondary)
        }
    }

    private var features: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Notes", systemImage: "list.bullet.rectangle")
            Label("Review guide", systemImage: "text.book.closed")
            Label("Chat", systemImage: "bubble.left.and.text.bubble.right")
            Label("Follow-up email", systemImage: "envelope")
            Text("Apple on-device or your own AI endpoint. Recording, transcription, calendar and people stay free.")
                .font(.footnote).foregroundStyle(.secondary)
                .padding(.top, 4)
        }
        .font(.callout)
    }

    @ViewBuilder private var plans: some View {
        if store.products.isEmpty {
            if store.isLoading {
                ProgressView().frame(maxWidth: .infinity)
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    Text(store.loadError ?? "Subscriptions aren't available right now.")
                        .font(.footnote).foregroundStyle(.secondary)
                    Button("Try Again") { Task { await store.loadProducts() } }
                }
            }
        } else {
            VStack(spacing: 12) {
                ForEach(store.products, id: \.id) { product in
                    planButton(product)
                }
            }
        }
        if let message {
            Text(message).font(.footnote).foregroundStyle(.orange)
        }
    }

    private func planButton(_ product: Product) -> some View {
        Button {
            buy(product)
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text(product.displayName).font(.headline)
                    Spacer()
                    if purchasing == product.id { ProgressView() }
                }
                Text(priceLine(product)).font(.subheadline.weight(.medium))
                if let trial = trialLine(product) {
                    Text(trial).font(.footnote).foregroundStyle(Theme.ai)
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Theme.hairline))
        }
        .buttonStyle(.plain)
        .disabled(purchasing != nil)
        .accessibilityIdentifier("plan-\(product.id)")
    }

    private var restore: some View {
        Button {
            restoring = true
            message = nil
            Task {
                defer { restoring = false }
                do {
                    try await store.restore()
                    if !store.isPro { message = "No active subscription found for this Apple Account." }
                } catch {
                    message = "Couldn't restore: \(error.localizedDescription)"
                }
            }
        } label: {
            HStack {
                Text("Restore Purchases")
                if restoring { ProgressView() }
            }
        }
        .disabled(restoring)
        .frame(maxWidth: .infinity)
    }

    private var finePrint: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Payment is charged to your Apple Account when you confirm. The subscription renews automatically at the price shown unless you cancel at least 24 hours before the end of the current period. Any unused part of a free trial ends when you subscribe. Manage or cancel in Settings → your name → Subscriptions.")
            Text("Local and Apple on-device models support offline AI after setup. If you choose a remote AI endpoint, transcript text is sent directly to that endpoint under its operator's terms. noFriction offers no hosted models and receives none of this content.")
            HStack(spacing: 16) {
                Link("Terms of Use (EULA)", destination: AppLinks.terms)
                Link("Privacy Policy", destination: AppLinks.privacyPolicy)
            }
            .font(.footnote.weight(.medium))
        }
        .font(.caption)
        .foregroundStyle(.secondary)
    }

    // MARK: Text

    private func priceLine(_ p: Product) -> String {
        guard let period = p.subscription?.subscriptionPeriod else { return p.displayPrice }
        return "\(p.displayPrice) / \(SubscriptionText.period(period.unit, value: period.value))"
    }

    private func trialLine(_ p: Product) -> String? {
        guard let sub = p.subscription, let offer = sub.introductoryOffer, trialEligible[p.id] == true else { return nil }
        let then = "then \(priceLine(p))"
        let length = SubscriptionText.duration(offer.period.unit, value: offer.period.value, periods: offer.periodCount)
        switch offer.paymentMode {
        case .freeTrial:
            return "Free for \(length), \(then)"
        case .payUpFront:
            return "\(offer.displayPrice) for the first \(length), \(then)"
        case .payAsYouGo:
            let each = SubscriptionText.period(offer.period.unit, value: offer.period.value)
            return "\(offer.displayPrice) / \(each) for \(length), \(then)"
        default:
            return nil
        }
    }

    private func buy(_ product: Product) {
        purchasing = product.id
        message = nil
        Task {
            defer { purchasing = nil }
            do {
                switch try await store.purchase(product) {
                case .purchased: dismiss()
                case .pending: message = "Purchase pending approval."
                case .cancelled: break
                }
            } catch {
                message = "Purchase failed: \(error.localizedDescription)"
            }
        }
    }
}
