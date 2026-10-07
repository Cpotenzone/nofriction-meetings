import Foundation

// The shared vocabulary of noFriction (identical strings on the Mac, the
// iPhone and Apple Watch): what a recording is, the notebook it belongs to,
// and the moment-marker labels. Compiled into the iPhone app and the watch
// app. Pure Foundation: no UI, no storage beyond the remembered choice.

/// "What is it?": Meeting · Class · Personal. Stored as `meeting` / `class` /
/// `personal`. A recording without a type (made before types existed) is a
/// meeting.
enum RecordingKind: String, CaseIterable, Codable, Sendable, Identifiable {
    case meeting
    case `class`
    case personal

    static let `default`: RecordingKind = .meeting
    static let pickerTitle = "What is it?"
    static let help = "Personal covers everything else: conversations, appointments, talks, ideas."

    var id: String { rawValue }

    /// nil or an unknown value (from another build) reads as a meeting.
    init(stored: String?) {
        self = stored.flatMap(RecordingKind.init(rawValue:)) ?? .default
    }

    /// Picker label
    var label: String {
        switch self {
        case .meeting: "Meeting"
        case .class: "Class"
        case .personal: "Personal"
        }
    }

    var systemImage: String {
        switch self {
        case .meeting: "person.2"
        case .class: "graduationcap"
        case .personal: "person"
        }
    }

    /// Placeholder of the Notebook field
    var notebookPlaceholder: String {
        switch self {
        case .meeting: "e.g. Acme project"
        case .class: "e.g. BIO 101"
        case .personal: "e.g. Health"
        }
    }

    /// The generated review material: "Study guide" for a class, "Review guide" otherwise.
    var guideTitle: String { self == .class ? "Study guide" : "Review guide" }
}

/// The remembered "What is it?" choice (preselected; used by starts without the sheet).
enum RecordingKindStore {
    static let key = "recordingKindDefault"

    /// Never chosen (or unreadable): Meeting.
    static func remembered(_ defaults: UserDefaults = .standard) -> RecordingKind {
        RecordingKind(stored: defaults.string(forKey: key))
    }

    static func remember(_ kind: RecordingKind, _ defaults: UserDefaults = .standard) {
        defaults.set(kind.rawValue, forKey: key)
    }
}

/// The notebook a recording belongs to: an optional grouping for any type
/// ("Acme project", "BIO 101", "Health"). Stored in `Meeting.courseName` on
/// iOS and `meetings.class_name` on the Mac (the names predate notebooks).
/// Cleaned, matched ignoring case; recent ones come from saved recordings.
enum Notebook {
    /// Field label
    static let label = "Notebook"
    /// Filter title
    static let filterTitle = "Notebooks"
    static let maxLength = 80

    /// Trimmed, whitespace collapsed, control characters dropped, ≤ 80 characters. Empty → nil.
    static func normalize(_ input: String?) -> String? {
        guard let input else { return nil }
        let words = input.unicodeScalars
            .filter { !CharacterSet.controlCharacters.subtracting(.whitespacesAndNewlines).contains($0) }
            .map(String.init).joined()
            .split(whereSeparator: { $0.isWhitespace })
        let joined = String(words.joined(separator: " ").prefix(maxLength))
        let trimmed = joined.trimmingCharacters(in: .whitespaces)
        return trimmed.isEmpty ? nil : trimmed
    }

    /// An existing notebook with the same name ignoring case wins ("bio 101" → "BIO 101").
    static func canonical(_ input: String?, existing: [String]) -> String? {
        guard let name = normalize(input) else { return nil }
        return existing.first { $0.caseInsensitiveCompare(name) == .orderedSame } ?? name
    }

    /// Most recently recorded first, one per name ignoring case. Derived from
    /// the recordings themselves, so deleting a recording removes its notebook here.
    static func recent(_ recordings: [(notebook: String?, startedAt: Date)], limit: Int = 12) -> [String] {
        var seen = Set<String>()
        var out: [String] = []
        for m in recordings.sorted(by: { $0.startedAt > $1.startedAt }) {
            guard let c = normalize(m.notebook), seen.insert(c.lowercased()).inserted else { continue }
            out.append(c)
            if out.count == limit { break }
        }
        return out
    }

    /// Chips while typing: recents when empty, prefix matches first, then contains.
    static func suggestions(_ input: String, recents: [String], max: Int = 6) -> [String] {
        guard let q = normalize(input)?.lowercased() else { return Array(recents.prefix(max)) }
        let starts = recents.filter { $0.lowercased().hasPrefix(q) }
        let contains = recents.filter { !$0.lowercased().hasPrefix(q) && $0.lowercased().contains(q) }
        return Array((starts + contains).prefix(max))
    }

    static func matches(_ notebook: String?, filter: String?) -> Bool {
        guard let filter else { return true }
        return notebook?.caseInsensitiveCompare(filter) == .orderedSame
    }
}

/// One-time, non-blocking reminder shown with the first **Class** recording.
enum ClassNotice {
    static let shownKey = "classRecordingNoticeShown"
    static let text = "Many schools require the instructor's permission to record a class, and some require classmates' consent. Check your school's policy."
}

/// The three ways to mark a moment while recording (docs/STUDY_TOOLS.md).
/// The stored value never changes; the third kind's label depends on what
/// the recording is: On the test (Class), Follow up (Meeting), Remember (Personal).
enum MarkerKind: String, CaseIterable, Codable, Sendable {
    case important, question, test

    /// One tap marks this
    static let `default`: MarkerKind = .important

    var symbol: String {
        switch self {
        case .important: "★"
        case .question: "?"
        case .test: "✎"
        }
    }

    func label(for kind: RecordingKind) -> String {
        switch self {
        case .important: "Important"
        case .question: "Question"
        case .test:
            switch kind {
            case .class: "On the test"
            case .meeting: "Follow up"
            case .personal: "Remember"
            }
        }
    }

    /// SF Symbol for buttons and rows
    var systemImage: String {
        switch self {
        case .important: "star.circle.fill"
        case .question: "questionmark.circle.fill"
        case .test: "pencil.circle.fill"
        }
    }
}
