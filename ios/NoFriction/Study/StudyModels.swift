import Foundation
import SwiftData

/// The three ways to mark a moment while recording (docs/STUDY_TOOLS.md).
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

    var label: String {
        switch self {
        case .important: "Important"
        case .question: "Question"
        case .test: "On the test"
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

/// A moment the user marked while recording (★ Important, ? Question,
/// ✎ On the test), with an optional short note. The note is user content:
/// it goes with its meeting (cascade). Same rules as the Mac
/// (`meeting_markers`).
@Model
final class MomentMarker {
    @Attribute(.unique) var id: UUID
    /// Wall clock, like `Segment.start`
    var at: Date
    /// `MarkerKind.rawValue`
    var kind: String
    var note: String?
    var createdAt: Date
    var meeting: Meeting?

    static let maxNoteLength = 280

    init(at: Date, kind: MarkerKind = .default, note: String? = nil) {
        self.id = UUID()
        self.at = at
        self.kind = kind.rawValue
        self.note = MomentMarker.clean(note)
        self.createdAt = .now
    }

    var markerKind: MarkerKind { MarkerKind(rawValue: kind) ?? .default }

    func setKind(_ k: MarkerKind) { kind = k.rawValue }
    func setNote(_ n: String?) { note = MomentMarker.clean(n) }

    /// Seconds from the meeting start
    func offset(in meeting: Meeting) -> TimeInterval { max(0, at.timeIntervalSince(meeting.startedAt)) }

    /// Trimmed, whitespace closed up, at most `maxNoteLength`; empty → nil.
    static func clean(_ note: String?) -> String? {
        guard let note else { return nil }
        let n = note.split(whereSeparator: { $0.isWhitespace || $0.isNewline }).joined(separator: " ")
        if n.isEmpty { return nil }
        return String(n.prefix(maxNoteLength))
    }
}

/// One part of a meeting's study guide: validated JSON (`StudyParse`), never
/// raw model output. Deleted whenever the meeting's transcript is edited
/// (Delete or Strike, docs/REDACTION.md) and with its meeting.
@Model
final class StudyMaterial {
    /// `StudyKind.rawValue`
    var kind: String
    var json: String
    /// SHA-256 of the transcript the part was made from (`StudyInput.fingerprint`)
    var transcriptFingerprint: String
    var createdAt: Date
    var meeting: Meeting?

    init(kind: StudyKind, json: String, transcriptFingerprint: String, createdAt: Date = .now) {
        self.kind = kind.rawValue
        self.json = json
        self.transcriptFingerprint = transcriptFingerprint
        self.createdAt = createdAt
    }

    var studyKind: StudyKind? { StudyKind(rawValue: kind) }
}

extension Meeting {
    var orderedMarkers: [MomentMarker] { markers.sorted { ($0.at, $0.createdAt) < ($1.at, $1.createdAt) } }

    func studyMaterial(_ kind: StudyKind) -> StudyMaterial? {
        studyMaterials.filter { $0.kind == kind.rawValue }.max { $0.createdAt < $1.createdAt }
    }
}

/// Saving and purging study material.
@MainActor
enum StudyStore {
    enum Failure: LocalizedError, Equatable {
        case transcriptChanged
        var errorDescription: String? {
            "The transcript changed while the study guide was being made (it was edited), so it wasn't saved. Make it again."
        }
    }

    /// Save generated parts, replacing earlier ones of the same kinds — only
    /// if the transcript still reads exactly as it did when generation
    /// started, so a guide made before an edit is never kept after it.
    static func save(_ parts: [(StudyKind, String)], fingerprint: String, meeting: Meeting, context: ModelContext) throws {
        guard !parts.isEmpty else { return }
        guard StudyInput(meeting: meeting).fingerprint == fingerprint else { throw Failure.transcriptChanged }
        for (kind, json) in parts {
            for old in meeting.studyMaterials where old.kind == kind.rawValue { context.delete(old) }
            let m = StudyMaterial(kind: kind, json: json, transcriptFingerprint: fingerprint)
            context.insert(m)
            m.meeting = meeting
        }
        try context.save()
    }

    /// Purge: every study material of the meeting. A guide paraphrases the
    /// lecture, so matching the removed words can't clean it.
    static func purge(_ meeting: Meeting, context: ModelContext) {
        for m in meeting.studyMaterials { context.delete(m) }
    }
}

extension RecordingSession {
    /// Mark the current moment of the live meeting (one tap: ★).
    @discardableResult
    func addMarker(kind: MarkerKind = .default, note: String? = nil, at time: Date = .now) -> MomentMarker? {
        guard isActive, let meeting, let context = meeting.modelContext else { return nil }
        let marker = MomentMarker(at: time, kind: kind, note: note)
        context.insert(marker)
        marker.meeting = meeting
        try? context.save()
        return marker
    }
}
