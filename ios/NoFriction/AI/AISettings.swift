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

    private enum Keys {
        static let providers = "ai.providers"
        static let active = "ai.activeProvider"
        static let consent = "ai.consent"
    }

    @ObservationIgnored private let defaults: UserDefaults
    private(set) var saved: [String: Saved]
    private(set) var consented: Set<String>
    private(set) var activeProviderID: String?

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        saved = (defaults.data(forKey: Keys.providers)).flatMap { try? JSONDecoder().decode([String: Saved].self, from: $0) } ?? [:]
        consented = Set(defaults.stringArray(forKey: Keys.consent) ?? [])
        activeProviderID = defaults.string(forKey: Keys.active)
        // Drop entries whose key vanished (e.g. restored to a new device: keys are ThisDeviceOnly)
        for (id, _) in saved {
            if let p = AIProvider.byID(id), p.requiresKey, !KeychainStore.has(id) { saved[id] = nil }
        }
        if let a = activeProviderID, saved[a] == nil { activeProviderID = nil }
        persist()
    }

    /// Saved providers in table order.
    var savedProviders: [AIProvider] { AIProvider.all.filter { saved[$0.id] != nil } }

    /// The provider AI features use: the chosen one, else Apple on-device when available.
    var effectiveProvider: AIProvider? {
        if let a = activeProviderID, let p = AIProvider.byID(a), saved[a] != nil { return p }
        return AppleOnDevice.isAvailable ? .apple : nil
    }

    var isConfigured: Bool { effectiveProvider != nil }

    func model(for id: String) -> String? { saved[id]?.model }

    func baseURL(for provider: AIProvider) -> URL? {
        if let s = saved[provider.id]?.baseURL, let u = URL(string: s) { return u }
        return provider.defaultBaseURL
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
        let key = KeychainStore.get(p.id)
        if p.requiresKey && (key ?? "").isEmpty { return nil }
        return AIEndpoint(provider: p, baseURL: baseURL(for: p), apiKey: key, model: s.model,
                          contextTokens: s.contextTokens, consentGranted: hasConsent(p))
    }

    /// Endpoint used to validate a key before it's saved.
    static func probe(_ provider: AIProvider, key: String?, baseURL: URL?) -> AIEndpoint {
        AIEndpoint(provider: provider, baseURL: baseURL ?? provider.defaultBaseURL, apiKey: key, model: "",
                   contextTokens: ContextFit.defaultContextTokens, consentGranted: false)
    }

    // MARK: Changes

    func save(_ provider: AIProvider, key: String?, baseURL: URL?, models: [ModelInfo]) throws {
        if let key, !key.isEmpty {
            try KeychainStore.set(key, for: provider.id)
        } else if !provider.requiresKey {
            KeychainStore.delete(provider.id)
        }
        let previous = saved[provider.id]
        let ids = models.map(\.id)
        let model = previous.flatMap { ids.contains($0.model) ? $0.model : nil }
            ?? ModelPicker.defaultModel(for: provider, from: ids) ?? ""
        let newBase = provider.editableBaseURL ? baseURL?.absoluteString : nil
        // A different server is a different recipient: ask again
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
        KeychainStore.delete(provider.id)
        saved[provider.id] = nil
        consented.remove(provider.id)
        if activeProviderID == provider.id { activeProviderID = savedProviders.first?.id }
        persist()
    }

    func setActive(_ provider: AIProvider) {
        guard saved[provider.id] != nil else { return }
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
