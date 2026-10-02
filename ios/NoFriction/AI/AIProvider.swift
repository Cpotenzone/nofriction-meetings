import Foundation

/// Wire protocol a provider speaks. See docs/AI_PROVIDERS.md (shared with the Mac app).
enum AIProtocol: String, Codable, Sendable {
    case openai            // POST {base}/chat/completions
    case anthropic         // POST {base}/messages
    case foundationModels  // Apple on-device (iOS 26+)
}

/// A provider preset. Users bring their own key; we ship none and run no servers.
struct AIProvider: Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let proto: AIProtocol
    /// nil for `custom` (user enters it) and `apple` (no network).
    let defaultBaseURL: URL?
    /// Key prefixes used for auto-detect. Longest match across all presets wins.
    let keyPrefixes: [String]
    let requiresKey: Bool
    /// The user may change the base URL (local servers, custom endpoints).
    let editableBaseURL: Bool
    let getKeyURL: URL?
    /// Default-model preference, matched exactly first, then as a prefix.
    let preferredModels: [String]
    /// Models to offer when the provider has no usable list endpoint.
    let staticModels: [String]
    /// Absolute URL used to check the key when `{base}/models` isn't
    /// authenticated (OpenRouter) or lives elsewhere (Perplexity).
    let validationURL: URL?

    var isOnDevice: Bool { proto == .foundationModels }

    static func == (a: AIProvider, b: AIProvider) -> Bool { a.id == b.id }
    func hash(into h: inout Hasher) { h.combine(id) }
}

extension AIProvider {
    private static func u(_ s: String) -> URL { URL(string: s)! }

    static let openai = AIProvider(
        id: "openai", name: "OpenAI", proto: .openai, defaultBaseURL: u("https://api.openai.com/v1"),
        keyPrefixes: ["sk-proj-", "sk-svcacct-", "sk-"], requiresKey: true, editableBaseURL: false,
        getKeyURL: u("https://platform.openai.com/api-keys"),
        preferredModels: ["gpt-5-mini", "gpt-5", "gpt-4.1-mini", "gpt-4o-mini"], staticModels: [], validationURL: nil)

    static let anthropic = AIProvider(
        id: "anthropic", name: "Anthropic Claude", proto: .anthropic, defaultBaseURL: u("https://api.anthropic.com/v1"),
        keyPrefixes: ["sk-ant-"], requiresKey: true, editableBaseURL: false,
        getKeyURL: u("https://console.anthropic.com/settings/keys"),
        preferredModels: ["claude-sonnet-5", "claude-opus-5-5", "claude-haiku-4-5"], staticModels: [], validationURL: nil)

    static let gemini = AIProvider(
        id: "gemini", name: "Google Gemini", proto: .openai,
        defaultBaseURL: u("https://generativelanguage.googleapis.com/v1beta/openai"),
        keyPrefixes: ["AIza"], requiresKey: true, editableBaseURL: false,
        getKeyURL: u("https://aistudio.google.com/apikey"),
        preferredModels: ["gemini-3-flash", "gemini-2.5-flash", "gemini-flash-latest", "gemini-2.5-pro"],
        staticModels: [], validationURL: nil)

    static let xai = AIProvider(
        id: "xai", name: "xAI Grok", proto: .openai, defaultBaseURL: u("https://api.x.ai/v1"),
        keyPrefixes: ["xai-"], requiresKey: true, editableBaseURL: false, getKeyURL: u("https://console.x.ai"),
        preferredModels: ["grok-4", "grok-3-mini", "grok-3"], staticModels: [], validationURL: nil)

    static let groq = AIProvider(
        id: "groq", name: "Groq", proto: .openai, defaultBaseURL: u("https://api.groq.com/openai/v1"),
        keyPrefixes: ["gsk_"], requiresKey: true, editableBaseURL: false, getKeyURL: u("https://console.groq.com/keys"),
        preferredModels: ["llama-3.3-70b-versatile", "openai/gpt-oss-120b"], staticModels: [], validationURL: nil)

    static let openrouter = AIProvider(
        id: "openrouter", name: "OpenRouter", proto: .openai, defaultBaseURL: u("https://openrouter.ai/api/v1"),
        keyPrefixes: ["sk-or-"], requiresKey: true, editableBaseURL: false, getKeyURL: u("https://openrouter.ai/keys"),
        preferredModels: ["openai/gpt-5-mini", "anthropic/claude-sonnet-5", "openai/gpt-4o-mini"], staticModels: [],
        // /models is public on OpenRouter, so it can't tell a bad key from a good one
        validationURL: u("https://openrouter.ai/api/v1/key"))

    static let mistral = AIProvider(
        id: "mistral", name: "Mistral", proto: .openai, defaultBaseURL: u("https://api.mistral.ai/v1"),
        keyPrefixes: [], requiresKey: true, editableBaseURL: false, getKeyURL: u("https://console.mistral.ai/api-keys"),
        preferredModels: ["mistral-medium-latest", "mistral-large-latest", "mistral-small-latest"], staticModels: [],
        validationURL: nil)

    static let deepseek = AIProvider(
        id: "deepseek", name: "DeepSeek", proto: .openai, defaultBaseURL: u("https://api.deepseek.com/v1"),
        keyPrefixes: [], requiresKey: true, editableBaseURL: false, getKeyURL: u("https://platform.deepseek.com/api_keys"),
        preferredModels: ["deepseek-chat"], staticModels: [], validationURL: nil)

    static let perplexity = AIProvider(
        id: "perplexity", name: "Perplexity", proto: .openai, defaultBaseURL: u("https://api.perplexity.ai"),
        keyPrefixes: ["pplx-"], requiresKey: true, editableBaseURL: false,
        getKeyURL: u("https://www.perplexity.ai/settings/api"),
        preferredModels: ["sonar"], staticModels: ["sonar", "sonar-pro", "sonar-reasoning-pro"],
        // Chat lives at /chat/completions; the (authenticated) model list is /v1/models
        validationURL: u("https://api.perplexity.ai/v1/models"))

    static let together = AIProvider(
        id: "together", name: "Together AI", proto: .openai, defaultBaseURL: u("https://api.together.xyz/v1"),
        keyPrefixes: [], requiresKey: true, editableBaseURL: false,
        getKeyURL: u("https://api.together.ai/settings/api-keys"),
        preferredModels: ["meta-llama/Llama-3.3-70B-Instruct-Turbo"], staticModels: [], validationURL: nil)

    static let ollama = AIProvider(
        id: "ollama", name: "Ollama (local)", proto: .openai, defaultBaseURL: u("http://localhost:11434/v1"),
        keyPrefixes: [], requiresKey: false, editableBaseURL: true, getKeyURL: u("https://ollama.com"),
        preferredModels: [], staticModels: [], validationURL: nil)

    static let lmstudio = AIProvider(
        id: "lmstudio", name: "LM Studio (local)", proto: .openai, defaultBaseURL: u("http://localhost:1234/v1"),
        keyPrefixes: [], requiresKey: false, editableBaseURL: true, getKeyURL: u("https://lmstudio.ai"),
        preferredModels: [], staticModels: [], validationURL: nil)

    static let custom = AIProvider(
        id: "custom", name: "Custom (OpenAI-compatible)", proto: .openai, defaultBaseURL: nil,
        keyPrefixes: [], requiresKey: false, editableBaseURL: true, getKeyURL: nil,
        preferredModels: [], staticModels: [], validationURL: nil)

    static let apple = AIProvider(
        id: "apple", name: "Apple on-device", proto: .foundationModels, defaultBaseURL: nil,
        keyPrefixes: [], requiresKey: false, editableBaseURL: false, getKeyURL: nil,
        preferredModels: ["apple-on-device"], staticModels: ["apple-on-device"], validationURL: nil)

    /// Table order from the spec.
    static let all: [AIProvider] = [
        .openai, .anthropic, .gemini, .xai, .groq, .openrouter, .mistral, .deepseek,
        .perplexity, .together, .ollama, .lmstudio, .custom, .apple,
    ]

    /// Providers you paste a key for.
    static let cloud: [AIProvider] = all.filter { $0.requiresKey }
    /// Providers you point at your own server.
    static let selfHosted: [AIProvider] = [.ollama, .lmstudio, .custom]

    static func byID(_ id: String) -> AIProvider? { all.first { $0.id == id } }
}

// MARK: - Key detection

enum KeyDetector {
    /// Trim whitespace, surrounding quotes and a leading "Bearer ".
    static func normalize(_ raw: String) -> String {
        var s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let quotes: Set<Character> = ["\"", "'", "`", "“", "”", "‘", "’"]
        while let f = s.first, quotes.contains(f) { s.removeFirst() }
        while let l = s.last, quotes.contains(l) { s.removeLast() }
        s = s.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.lowercased().hasPrefix("bearer ") { s = String(s.dropFirst(7)) }
        // Keys never contain whitespace; a stray newline from a copy is common
        return s.components(separatedBy: .whitespacesAndNewlines).joined()
    }

    /// The provider whose longest prefix matches, or nil.
    static func detect(_ key: String) -> AIProvider? {
        var best: (AIProvider, Int)?
        for p in AIProvider.all {
            for prefix in p.keyPrefixes where key.hasPrefix(prefix) {
                if prefix.count > (best?.1 ?? 0) { best = (p, prefix.count) }
            }
        }
        return best?.0
    }

    /// Providers to try, in order. A bare `sk-` key is OpenAI first, then DeepSeek.
    static func candidates(for key: String) -> [AIProvider] {
        guard let p = detect(key) else { return [] }
        if p == .openai, !key.hasPrefix("sk-proj-"), !key.hasPrefix("sk-svcacct-") {
            return [.openai, .deepseek]
        }
        return [p]
    }

    static func last4(_ key: String) -> String { String(key.suffix(4)) }
}

// MARK: - URL policy

enum URLPolicy {
    /// Cloud traffic is HTTPS only. Plain http is allowed only for hosts on
    /// this device, the LAN or a Tailscale tailnet.
    static func isAllowed(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased(), let host = url.host(percentEncoded: false), !host.isEmpty else {
            return false
        }
        if url.user != nil || url.password != nil { return false }  // no credentials in URLs
        switch scheme {
        case "https": return true
        case "http": return isPrivateHost(host)
        default: return false
        }
    }

    static func isPrivateHost(_ rawHost: String) -> Bool {
        let host = rawHost.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: "[]."))
        if host == "localhost" || host.hasSuffix(".localhost") { return true }
        if host == "::1" { return true }
        if host.hasSuffix(".local") || host.hasSuffix(".ts.net") { return true }
        if let o = ipv4(host) {
            switch (o[0], o[1]) {
            case (127, _), (10, _): return true
            case (172, 16...31): return true
            case (192, 168): return true
            case (169, 254): return true                 // link-local
            case (100, 64...127): return true            // Tailscale CGNAT 100.64.0.0/10
            default: return false
            }
        }
        if host.hasPrefix("fd") || host.hasPrefix("fc") { return host.contains(":") }  // IPv6 ULA
        if host.hasPrefix("fe80:") { return true }
        return false
    }

    private static func ipv4(_ s: String) -> [Int]? {
        let parts = s.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return nil }
        let nums = parts.compactMap { Int($0) }
        guard nums.count == 4, nums.allSatisfy({ (0...255).contains($0) }) else { return nil }
        return nums
    }

    /// Consent is needed for anything that leaves the device or the user's own network.
    static func needsConsent(provider: AIProvider, baseURL: URL?) -> Bool {
        if provider.isOnDevice { return false }
        guard let host = baseURL?.host(percentEncoded: false) else { return true }
        return !isPrivateHost(host)
    }

    /// Parse what a user typed ("192.168.1.5:11434", "http://box.local:1234/v1").
    static func parseBaseURL(_ raw: String) -> URL? {
        var s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !s.isEmpty else { return nil }
        if !s.contains("://") {
            let host = s.split(separator: "/").first.map(String.init) ?? s
            let hostOnly = host.split(separator: ":").first.map(String.init) ?? host
            s = (isPrivateHost(hostOnly) ? "http://" : "https://") + s
        }
        while s.hasSuffix("/") { s.removeLast() }
        guard let url = URL(string: s), url.host != nil else { return nil }
        return url
    }
}

// MARK: - Redaction

enum Redactor {
    private static let patterns: [NSRegularExpression] = [
        #"sk-[A-Za-z0-9_\-]{6,}"#,
        #"AIza[0-9A-Za-z_\-]{10,}"#,
        #"xai-[A-Za-z0-9_\-]{6,}"#,
        #"gsk_[A-Za-z0-9_\-]{6,}"#,
        #"pplx-[A-Za-z0-9_\-]{6,}"#,
        #"(?i)bearer\s+[^\s"',}]+"#,
        #"(?i)(x-api-key|api[_-]?key|authorization)(["']?\s*[:=]\s*["']?)[^\s"',}]+"#,
    ].map { try! NSRegularExpression(pattern: $0) }

    /// Replace anything that looks like a key (and the given secrets) with "[redacted]".
    static func redact(_ text: String, secrets: [String] = []) -> String {
        var out = text
        for s in secrets where s.count >= 6 {
            out = out.replacingOccurrences(of: s, with: "[redacted]")
        }
        for re in patterns {
            let range = NSRange(out.startIndex..., in: out)
            let template = re.numberOfCaptureGroups == 2 ? "$1$2[redacted]" : "[redacted]"
            out = re.stringByReplacingMatches(in: out, range: range, withTemplate: template)
        }
        return out
    }
}
