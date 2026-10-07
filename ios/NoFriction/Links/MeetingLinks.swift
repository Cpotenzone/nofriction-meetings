import Foundation
import SwiftData

/// A link the user added to a meeting: the syllabus, a reading, the slides
/// (docs/LINKS.md). Deleted with its meeting (cascade). Its title and note
/// are the user's own words: Delete / Strike of transcript words never
/// rewrite them (like comments on the Mac).
@Model
final class MeetingReference {
    @Attribute(.unique) var id: UUID
    /// http(s) only (`MeetingLinks.referenceURL`)
    var url: String
    var title: String?
    var note: String?
    var createdAt: Date
    var meeting: Meeting?

    init(url: String, title: String? = nil, note: String? = nil, createdAt: Date = .now) {
        self.id = UUID()
        self.url = url
        self.title = title
        self.note = note
        self.createdAt = createdAt
    }
}

/// One row of a meeting's Links section.
struct MeetingLinkItem: Identifiable, Equatable {
    enum Source: String { case added, said }

    let key: String
    let url: String
    let host: String
    let path: String
    let title: String?
    let note: String?
    let sources: [Source]
    /// Mentions in the transcript
    let saidCount: Int
    /// When it was first said
    let firstSaid: Date?
    /// Set for an added reference
    let referenceID: UUID?

    var id: String { referenceID?.uuidString ?? "said:\(key)" }
}

/// The Links section of a meeting. "Said" links are derived from the
/// transcript every time (never stored), so deleting or striking the words
/// removes them. iOS has no screen capture, so there is no "On screen".
enum MeetingLinks {
    static let maxTitle = 200
    static let maxNote = 2000
    static let maxURL = 2048

    /// Links said in the transcript, by first mention.
    static func said(in meeting: Meeting) -> [MeetingLinkItem] {
        var order: [String] = []
        var groups: [String: (link: LinkDetector.Normalized, https: Bool, count: Int, first: Date)] = [:]
        for segment in meeting.orderedSegments {
            for f in LinkDetector.detect(segment.text, spoken: true) {
                if var g = groups[f.link.key] {
                    g.count += 1
                    g.https = g.https || f.link.scheme == "https"
                    g.first = min(g.first, segment.start)
                    groups[f.link.key] = g
                } else {
                    order.append(f.link.key)
                    groups[f.link.key] = (f.link, f.link.scheme == "https", 1, segment.start)
                }
            }
        }
        return order.compactMap { key in
            guard let g = groups[key] else { return nil }
            let scheme = g.https ? "https" : g.link.scheme
            return MeetingLinkItem(key: key, url: "\(scheme)://\(g.link.www ? "www." : "")\(key)", host: g.link.host,
                                   path: g.link.path, title: nil, note: nil, sources: [.said], saidCount: g.count,
                                   firstSaid: g.first, referenceID: nil)
        }
        .sorted { ($0.firstSaid ?? .distantFuture) < ($1.firstSaid ?? .distantFuture) }
    }

    /// Added references first (in the order added), then said links. A said
    /// link with the same address joins its reference.
    static func items(for meeting: Meeting) -> [MeetingLinkItem] {
        var detected = said(in: meeting)
        var out: [MeetingLinkItem] = []
        for r in meeting.references.sorted(by: { $0.createdAt < $1.createdAt }) {
            let n = LinkDetector.normalize(r.url)
            let key = n?.key ?? r.url
            let match = detected.firstIndex { $0.key == key }.map { detected.remove(at: $0) }
            out.append(MeetingLinkItem(key: key, url: r.url, host: n?.host ?? "", path: n?.path ?? "",
                                       title: r.title, note: r.note,
                                       sources: match == nil ? [.added] : [.added, .said],
                                       saidCount: match?.saidCount ?? 0, firstSaid: match?.firstSaid,
                                       referenceID: r.id))
        }
        return out + detected
    }

    enum InputError: LocalizedError, Equatable {
        case notWebAddress, tooLong(String)
        var errorDescription: String? {
            switch self {
            case .notWebAddress: "Enter a web address that starts with http:// or https:// (like https://example.edu/syllabus)."
            case .tooLong(let what): "The \(what) is too long."
            }
        }
    }

    /// A typed address as stored: `https://` added when there's no scheme.
    /// Only http(s) web addresses are accepted.
    static func referenceURL(_ input: String) throws -> String {
        let s = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !s.isEmpty, LinkDetector.normalize(s) != nil else { throw InputError.notWebAddress }
        guard s.utf8.count <= maxURL else { throw InputError.tooLong("address") }
        let lower = s.lowercased()
        let url = lower.hasPrefix("http://") || lower.hasPrefix("https://") ? s : "https://" + s
        guard LinkDetector.isOpenable(url) else { throw InputError.notWebAddress }
        return url
    }

    private static func clean(_ s: String?, max: Int, what: String) throws -> String? {
        guard let t = s?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty else { return nil }
        guard t.count <= max else { throw InputError.tooLong(what) }
        return t
    }

    @MainActor
    @discardableResult
    static func add(to meeting: Meeting, url: String, title: String?, note: String?, context: ModelContext) throws -> MeetingReference {
        let r = MeetingReference(url: try referenceURL(url), title: try clean(title, max: maxTitle, what: "title"),
                                 note: try clean(note, max: maxNote, what: "note"))
        context.insert(r)
        r.meeting = meeting
        try context.save()
        return r
    }

    @MainActor
    static func update(_ r: MeetingReference, url: String, title: String?, note: String?, context: ModelContext) throws {
        let u = try referenceURL(url)
        let t = try clean(title, max: maxTitle, what: "title")
        let n = try clean(note, max: maxNote, what: "note")
        r.url = u
        r.title = t
        r.note = n
        try context.save()
    }

    @MainActor
    static func delete(_ r: MeetingReference, context: ModelContext) throws {
        context.delete(r)
        try context.save()
    }

    /// "/courses/…/week-3": long paths keep their first and last segment.
    static func shortPath(_ path: String, max: Int = 36) -> String {
        guard path.count > max else { return path }
        let q = path.firstIndex(where: { $0 == "?" || $0 == "#" })
        let bare = q.map { String(path[..<$0]) } ?? path
        let segs = bare.split(separator: "/")
        if segs.count >= 3 {
            let s = "/\(segs[0])/…/\(segs[segs.count - 1])\(q == nil ? "" : "?…")"
            if s.count <= max { return s }
        }
        return String(path.prefix(Swift.max(1, max - 1))) + "…"
    }

    static func display(_ item: MeetingLinkItem) -> String {
        item.host.isEmpty ? item.url : item.host + shortPath(item.path)
    }
}
