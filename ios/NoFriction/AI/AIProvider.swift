import Foundation

/// Supported execution paths: an explicitly configured compatible endpoint or Apple on-device.
enum AIProtocol: String, Codable, Sendable {
    case openai            // POST {user-entered base}/chat/completions
    case foundationModels  // Apple on-device (iOS 26+)
}

struct AIProvider: Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let proto: AIProtocol
    var isOnDevice: Bool { proto == .foundationModels }

    static let custom = AIProvider(id: "custom", name: "Your AI endpoint", proto: .openai)
    static let apple = AIProvider(id: "apple", name: "Apple on-device", proto: .foundationModels)
    static let all: [AIProvider] = [.custom, .apple]
    static func byID(_ id: String) -> AIProvider? { all.first { $0.id == id } }
}

// MARK: - Endpoint presets (UI convenience over the one custom endpoint)

/// A named provider preset. Tapping one only pre-fills the custom endpoint's
/// base URL and model in the setup form; the user still pastes their own key,
/// saves, and consents before any recording content is sent. No preset is
/// selected or active until tapped, and the saved connection stays `.custom`
/// (same Keychain binding, same consent). This table is the only place in the
/// iOS source where provider hosts may appear; `scripts/check-ai-provider-policy.py`
/// enforces that and checks it matches the Mac table.
struct AIPreset: Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    /// OpenAI-compatible chat-completions base (the app appends `chat/completions`)
    let baseURL: String
    let defaultModel: String
    /// Other model ids worth trying, shown under the model field
    let modelHint: String
    /// Where the user creates their own API key ("Get a key" link, opens Safari)
    let keyURL: String
    /// One line shown on the card
    let note: String

    static let all: [AIPreset] = [
        // OpenAI Chat Completions API (https://developers.openai.com/api/docs/changelog
        // lists gpt-6-luna, gpt-6.1-sol and gpt-6-astra under v1/chat/completions;
        // catalogue: https://developers.openai.com/api/docs/models).
        AIPreset(id: "openai", name: "ChatGPT (OpenAI)", baseURL: "https://api.openai.com/v1",
                 defaultModel: "gpt-6-luna", modelHint: "gpt-6.1-sol, gpt-6-astra",
                 keyURL: "https://platform.openai.com/api-keys", note: "Your OpenAI API key; billed by OpenAI."),
        // Anthropic's OpenAI SDK compatibility layer: base https://api.anthropic.com/v1/
        // with the Claude key as a Bearer token
        // (https://platform.claude.com/docs/en/cli-sdks-libraries/libraries/openai-sdk);
        // model ids: https://platform.claude.com/docs/en/models/overview.
        AIPreset(id: "anthropic", name: "Anthropic (Claude)", baseURL: "https://api.anthropic.com/v1",
                 defaultModel: "claude-sonnet-5-5", modelHint: "claude-haiku-5-5, claude-opus-5-5",
                 keyURL: "https://platform.claude.com/settings/keys", note: "Your Claude API key; billed by Anthropic."),
        // Meta Model API (Muse): OpenAI-compatible Chat Completions at
        // https://api.meta.ai/v1, Bearer auth; default muse-spark-1.3
        // (https://dev.meta.ai/docs/quickstart). Keys: https://dev.meta.ai → API keys.
        AIPreset(id: "meta", name: "Muse (Meta)", baseURL: "https://api.meta.ai/v1",
                 defaultModel: "muse-spark-1.3", modelHint: "muse-spark-1.1",
                 keyURL: "https://dev.meta.ai", note: "Your Meta Model API key; billed by Meta."),
        // xAI: OpenAI-compatible base https://api.x.ai/v1, Bearer auth
        // (https://docs.x.ai/docs/guides/chat); /v1/chat/completions is kept but
        // marked deprecated in favour of /v1/responses
        // (https://docs.x.ai/developers/model-capabilities/text/comparison).
        AIPreset(id: "xai", name: "Grok (xAI)", baseURL: "https://api.x.ai/v1",
                 defaultModel: "grok-4.7", modelHint: "grok-4.3",
                 keyURL: "https://console.x.ai", note: "Your xAI API key; billed by xAI."),
        // Mistral: POST https://api.mistral.ai/v1/chat/completions, Bearer auth
        // (https://docs.mistral.ai/api/).
        AIPreset(id: "mistral", name: "Mistral", baseURL: "https://api.mistral.ai/v1",
                 defaultModel: "mistral-large-latest", modelHint: "mistral-small-latest",
                 keyURL: "https://console.mistral.ai/api-keys", note: "Your Mistral API key; billed by Mistral."),
    ]

    static func byID(_ id: String) -> AIPreset? { all.first { $0.id == id } }

    /// The preset whose base URL is the given one (trailing slash and case
    /// ignored). Derived from the URL, never stored: a URL edited to a proxy
    /// is no longer "on" the preset.
    static func matching(_ url: URL?) -> AIPreset? {
        guard let url else { return nil }
        let wanted = normalized(url.absoluteString)
        return all.first { normalized($0.baseURL) == wanted }
    }

    static func matching(_ raw: String) -> AIPreset? {
        matching(URLPolicy.parseBaseURL(raw))
    }

    private static func normalized(_ s: String) -> String {
        var t = s.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        while t.hasSuffix("/") { t.removeLast() }
        return t
    }

    var host: String { URL(string: baseURL)?.host() ?? baseURL }
}

// MARK: - Key input

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
        if url.user != nil || url.password != nil || url.query != nil || url.fragment != nil { return false }  // no credentials in URLs
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
