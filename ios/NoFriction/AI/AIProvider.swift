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
