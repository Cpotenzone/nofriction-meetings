import Foundation
import SwiftData

/// The one search, at the top of Recordings: titles, notebooks, people,
/// topics, and anything said. Same behavior as the Mac
/// (`src-tauri/src/search.rs`, `src/lib/searchLogic.ts`): results group by
/// recording with up to 3 matching lines and their time, and a line opens the
/// recording at that moment.
///
/// Everything is looked up on this device. The query and the matching lines
/// are never logged. Matching is case- and diacritic-insensitive. Only words
/// still in the transcript can match: Delete removes the words from the
/// store, and Strike leaves a marker token that carries an id, never the
/// words (docs/REDACTION.md), so marker tokens are never matched.
enum RecordingSearch {
    /// Lines shown per recording; the rest are counted ("and 2 more")
    static let maxLines = 3
    /// Matching lines fetched per search, so a very common word stays fast
    static let lineFetchLimit = 1000
    /// A long line is shown from a little before the match
    static let excerptLead = 40

    /// A transcript line that matched. A value (no model reference), so a
    /// line deleted after the search can't be touched through it.
    struct Line: Identifiable, Hashable {
        /// The segment, to scroll to in the recording
        let id: PersistentIdentifier
        let meetingID: UUID
        /// From the recording's start, in seconds
        let offset: TimeInterval
        /// As shown: struck words read `[stricken from the record]`; a long
        /// line starts shortly before the match
        let text: String
        /// Heard from what was playing during screen capture ("On screen")
        let isFromScreen: Bool

        /// "At 12:03: <text>" (VoiceOver)
        var accessibilityLabel: String {
            "At \(offset.clock)\(isFromScreen ? ", \(ScreenSpeaker.label.lowercased())" : ""): \(text)"
        }
    }

    /// The matching lines of one recording.
    struct Group: Equatable {
        /// Up to `maxLines`, earliest first
        var lines: [Line]
        /// Lines that matched beyond `maxLines`
        var moreLines: Int
    }

    // MARK: Matching (pure)

    /// The query to search for; nil when it's only whitespace.
    static func normalized(_ query: String) -> String? {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        return q.isEmpty ? nil : q
    }

    private static let options: String.CompareOptions = [.caseInsensitive, .diacriticInsensitive, .widthInsensitive]

    /// Whether `text` contains `query`, ignoring case and diacritics.
    static func matches(_ text: String, query: String) -> Bool {
        guard let q = normalized(query) else { return false }
        return text.range(of: q, options: options, locale: .current) != nil
    }

    /// The words of a segment that can be searched: strike markers removed.
    static func searchableText(_ segmentText: String) -> String {
        RedactionText.pieces(segmentText).compactMap { piece -> String? in
            if case .text(let t) = piece { return t }
            return nil
        }
        .joined(separator: " ")
    }

    /// Whether the words of a segment (not its strike markers) contain `query`.
    static func lineMatches(_ segmentText: String, query: String) -> Bool {
        matches(searchableText(segmentText), query: query)
    }

    /// The line as shown in results: markers as `[stricken from the record]`,
    /// and a long line starting at a word shortly before the first match
    /// (outside a placeholder), with "…".
    static func displayText(_ segmentText: String, query: String) -> String {
        let plain = RedactionText.plain(segmentText).trimmingCharacters(in: .whitespacesAndNewlines)
        guard let q = normalized(query), let hit = firstMatch(of: q, in: plain) else { return plain }
        let before = plain.distance(from: plain.startIndex, to: hit.lowerBound)
        guard before > excerptLead + 10 else { return plain }
        var start = plain.index(hit.lowerBound, offsetBy: -excerptLead)
        // Begin at a word: just after the nearest space at or after `start`
        if let space = plain[start..<hit.lowerBound].firstIndex(where: \.isWhitespace) {
            start = plain.index(after: space)
        }
        return "…" + String(plain[start...])
    }

    /// First match of `q` in `plain` that isn't inside a strike placeholder.
    private static func firstMatch(of q: String, in plain: String) -> Range<String.Index>? {
        var placeholders: [Range<String.Index>] = []
        var from = plain.startIndex
        while let r = plain.range(of: RedactionText.placeholder, range: from..<plain.endIndex) {
            placeholders.append(r)
            from = r.upperBound
        }
        from = plain.startIndex
        while from < plain.endIndex, let r = plain.range(of: q, options: options, range: from..<plain.endIndex, locale: .current) {
            if !placeholders.contains(where: { $0.overlaps(r) }) { return r }
            from = r.upperBound
        }
        return nil
    }

    /// Lines per recording: earliest first, at most `maxLines`, the rest counted.
    static func group(_ lines: [Line], maxLines: Int = maxLines) -> [UUID: Group] {
        Dictionary(grouping: lines, by: \.meetingID).mapValues { all in
            let sorted = all.sorted { $0.offset < $1.offset }
            return Group(lines: Array(sorted.prefix(maxLines)), moreLines: max(0, sorted.count - maxLines))
        }
    }

    /// Title, notebook, people (name, email, company) or topics contain the
    /// query. "Anything said" is `lines(matching:in:)`.
    @MainActor
    static func meetingMatches(_ m: Meeting, query: String) -> Bool {
        guard let q = normalized(query) else { return true }
        return matches(m.title, query: q)
            || (m.courseName.map { matches($0, query: q) } ?? false)
            || m.people.contains { p in
                matches(p.person.displayName, query: q) || matches(p.person.email, query: q)
                    || (p.person.company.map { matches($0, query: q) } ?? false)
            }
            || m.topics.contains { matches($0.label, query: q) }
    }

    // MARK: Store

    /// Transcript lines containing `query`, found with a store predicate
    /// (so transcripts aren't loaded to search them), then checked against
    /// the words actually in the line: a strike marker's id never matches,
    /// and a line deleted in this context (Delete's undo window) is skipped.
    /// Lines of `excluding` (the recording in progress) are left out.
    @MainActor
    static func lines(matching query: String, in context: ModelContext, excluding: UUID? = nil,
                      limit: Int = lineFetchLimit) -> [Line] {
        guard let q = normalized(query) else { return [] }
        var descriptor = FetchDescriptor<Segment>(
            predicate: #Predicate { $0.text.localizedStandardContains(q) },
            sortBy: [SortDescriptor(\.start)])
        descriptor.fetchLimit = limit
        let segments = (try? context.fetch(descriptor)) ?? []
        return segments.compactMap { s in
            guard !s.isDeleted, let m = s.meeting, !m.isDeleted, m.id != excluding,
                  lineMatches(s.text, query: q) else { return nil }
            return Line(id: s.persistentModelID, meetingID: m.id, offset: max(0, s.start.timeIntervalSince(m.startedAt)),
                        text: displayText(s.text, query: q), isFromScreen: s.isFromScreen)
        }
    }
}
