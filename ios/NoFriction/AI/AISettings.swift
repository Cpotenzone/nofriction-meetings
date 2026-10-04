import Foundation
import Observation

/// Non-secret AI configuration (active provider, model, base URL, consent).
/// Keys themselves are only in the Keychain; the UI only ever sees last4.
@MainActor
@Observable
final class AISettings {
    struct Saved: Codable, Equatable {
        var last4: String?
        var baseURL: String?
        var model: String
        var models: [ModelInfo]

        var contextTokens: Int {
            models.first { $0.id == model }?.contextTokens ?? ContextFit.defaultContextTokens
        }
    }

    struct KeyStore {
        var get: (String) -> String?
        var set: (String, String) throws -> Void
        var delete: (String) throws -> Void

        static let live = KeyStore(
            get: { KeychainStore.get($0) },
            set: { key, account in try KeychainStore.set(key, for: account) },
            delete: { try KeychainStore.deleteChecked($0) })
    }

    private enum Keys {
        static let providers = "ai.providers"
        static let active = "ai.activeProvider"
        static let consent = "ai.consent"
    }

    @ObservationIgnored private let defaults: UserDefaults
    @ObservationIgnored private let keyStore: KeyStore
    private(set) var saved: [String: Saved]
    private(set) var consented: Set<String>
    private(set) var activeProviderID: String?

    init(defaults: UserDefaults = .standard, keyStore: KeyStore = .live) {
        self.defaults = defaults
        self.keyStore = keyStore
        saved = (defaults.data(forKey: Keys.providers)).flatMap { try? JSONDecoder().decode([String: Saved].self, from: $0) } ?? [:]
        consented = Set(defaults.stringArray(forKey: Keys.consent) ?? [])
        activeProviderID = defaults.string(forKey: Keys.active)
        // Keep old selections and saved data intact. Unsupported selections fail closed
        // below instead of silently switching the recipient or on-device model.
        persist()
    }

    /// Saved providers in table order.
    var savedProviders: [AIProvider] { AIProvider.all.filter { saved[$0.id] != nil } }

    /// A fresh installation may use available Apple AI. Any explicit selection
    /// must remain valid; legacy named providers require the user to configure AI.
    var effectiveProvider: AIProvider? {
        if let id = activeProviderID {
            guard let provider = AIProvider.byID(id), saved[id] != nil else { return nil }
            if provider == .apple { return AppleOnDevice.isAvailable ? .apple : nil }
            guard let base = baseURL(for: provider), URLPolicy.isAllowed(base),
                  let model = saved[id]?.model, !model.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
            return provider
        }
        return AppleOnDevice.isAvailable ? .apple : nil
    }

    var needsEndpointSetup: Bool {
        activeProviderID != nil && effectiveProvider == nil
    }

    var isConfigured: Bool { effectiveProvider != nil }

    func model(for id: String) -> String? { saved[id]?.model }

    func baseURL(for provider: AIProvider) -> URL? {
        if let s = saved[provider.id]?.baseURL, let u = URL(string: s) { return u }
        return nil
    }

    func hasConsent(_ provider: AIProvider) -> Bool {
        !URLPolicy.needsConsent(provider: provider, baseURL: baseURL(for: provider)) || consented.contains(provider.id)
    }

    /// Resolve everything a request needs. Reads the key from the Keychain.
    func endpoint() -> AIEndpoint? {
        guard let p = effectiveProvider else { return nil }
        if p == .apple {
            return AIEndpoint(provider: p, baseURL: nil, apiKey: nil, model: "apple-on-device",
                              contextTokens: AppleOnDevice.contextTokens, consentGranted: true)
        }
        guard let s = saved[p.id] else { return nil }
        // A key is read only when this saved endpoint explicitly includes one.
        let key = s.last4 == nil ? nil : keyStore.get(p.id)
        if s.last4 != nil && (key ?? "").isEmpty { return nil }

        return AIEndpoint(provider: p, baseURL: baseURL(for: p), apiKey: key, model: s.model,
                          contextTokens: s.contextTokens, consentGranted: hasConsent(p))
    }

    // MARK: Changes

    func save(_ provider: AIProvider, key: String?, baseURL: URL?, models: [ModelInfo]) throws {
        guard provider == .custom, let baseURL else { throw AIError.notConfigured }
        guard URLPolicy.isAllowed(baseURL) else { throw AIError.insecureURL }
        guard models.count == 1,
              let model = models.first?.id.trimmingCharacters(in: .whitespacesAndNewlines),
              !model.isEmpty else { throw AIError.notConfigured }
        // All routing input is checked before a key is stored. Saving is local only.
        if let key, !key.isEmpty {
            try keyStore.set(key, provider.id)
        } else {
            try keyStore.delete(provider.id)
        }
        let previous = saved[provider.id]
        let newBase = baseURL.absoluteString
        if previous?.baseURL != newBase { consented.remove(provider.id) }
        saved[provider.id] = Saved(last4: key.flatMap { $0.isEmpty ? nil : KeyDetector.last4($0) },
                                   baseURL: newBase, model: model, models: models)
        activeProviderID = provider.id
        persist()
    }

    func useApple() {
        saved[AIProvider.apple.id] = Saved(last4: nil, baseURL: nil, model: "apple-on-device",
                                           models: [ModelInfo(id: "apple-on-device", contextTokens: AppleOnDevice.contextTokens)])
        activeProviderID = AIProvider.apple.id
        persist()
    }

    func remove(_ provider: AIProvider) {
        try? keyStore.delete(provider.id)
        saved[provider.id] = nil
        consented.remove(provider.id)
        if activeProviderID == provider.id { activeProviderID = savedProviders.first?.id }
        persist()
    }

    func setActive(_ provider: AIProvider) {
        guard AIProvider.byID(provider.id) == provider, saved[provider.id] != nil else { return }
        activeProviderID = provider.id
        persist()
    }

    func setModel(_ model: String, for provider: AIProvider) {
        let m = model.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !m.isEmpty, saved[provider.id] != nil else { return }
        saved[provider.id]?.model = m
        persist()
    }

    func grantConsent(_ provider: AIProvider) {
        consented.insert(provider.id)
        persist()
    }

    func revokeConsent(_ provider: AIProvider) {
        consented.remove(provider.id)
        persist()
    }

    private func persist() {
        if let data = try? JSONEncoder().encode(saved) { defaults.set(data, forKey: Keys.providers) }
        defaults.set(activeProviderID, forKey: Keys.active)
        defaults.set(Array(consented).sorted(), forKey: Keys.consent)
    }
}
