import Foundation

/// LinkedIn has no public people-lookup API, so linking is: open a prefilled
/// search, then save the profile URL the user picks. Same rules as the Mac.
enum LinkedIn {
    static func searchURL(name: String?, email: String, company: String?) -> URL {
        let who = name ?? PersonNames.guess(fromEmail: email)
        let query = [who, company].compactMap { $0 }.joined(separator: " ")
        var c = URLComponents(string: "https://www.linkedin.com/search/results/people/")!
        c.queryItems = [URLQueryItem(name: "keywords", value: query)]
        return c.url!
    }

    enum NormalizeError: LocalizedError {
        case notLinkedIn, notProfile
        var errorDescription: String? {
            switch self {
            case .notLinkedIn: return "That isn't a LinkedIn link. Paste a profile URL like linkedin.com/in/jane-doe."
            case .notProfile: return "Paste a LinkedIn profile URL (linkedin.com/in/…)."
            }
        }
    }

    /// Accepts any common form of a profile link; returns
    /// "https://www.linkedin.com/in/<handle>".
    static func normalize(_ input: String) throws -> String {
        var t = input.trimmingCharacters(in: .whitespacesAndNewlines)
        while t.hasSuffix("/") { t.removeLast() }
        for prefix in ["https://", "http://"] where t.lowercased().hasPrefix(prefix) {
            t = String(t.dropFirst(prefix.count))
        }
        for prefix in ["www.", "m."] where t.lowercased().hasPrefix(prefix) {
            t = String(t.dropFirst(prefix.count))
        }
        guard t.lowercased().hasPrefix("linkedin.com/") else { throw NormalizeError.notLinkedIn }
        let path = t.dropFirst("linkedin.com/".count).split(whereSeparator: { $0 == "?" || $0 == "#" }).first ?? ""
        let parts = path.split(separator: "/")
        guard parts.count >= 2, parts[0] == "in", !parts[1].isEmpty else { throw NormalizeError.notProfile }
        return "https://www.linkedin.com/in/\(parts[1])"
    }
}
