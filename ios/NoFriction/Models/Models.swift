import Foundation
import SwiftData

/// One recording. Calendar fields are filled when the recording overlaps an
/// event on the user's calendar.
@Model
final class Meeting {
    @Attribute(.unique) var id: UUID
    var title: String
    var startedAt: Date
    var endedAt: Date?

    // From the calendar invite
    var calendarEventID: String?
    var scheduledStart: Date?
    var scheduledEnd: Date?
    var location: String?
    var meetingURL: String?
    var inviteNotes: String?

    /// File name of the recorded audio (in Documents/Audio)
    var audioFileName: String?

    /// Summary / decisions / action items from the user's AI provider (Markdown)
    var aiNotes: String?
    var aiNotesAt: Date?
    /// The transcript was edited (Delete / Strike) after the notes were made:
    /// the Notes section offers to regenerate. See docs/REDACTION.md.
    var aiNotesStale: Bool = false

    // Apple Watch recordings (docs/WATCH_APP.md). All optional, so stores
    // from before the watch app migrate without a schema version.
    /// "watch" when recorded on Apple Watch; nil when recorded on this device
    var source: String?
    /// The watch's recording id. Each one is imported once.
    var sourceRecordingID: String?
    /// Transcription of an imported recording: see `ImportState`. nil = done (or recorded live).
    var importState: String?
    /// Why the last attempt failed (a system message, never transcript text)
    var importError: String?
    /// Seconds of the audio file transcribed so far: where a resumed run starts
    var importProgress: Double?
    /// Length of the audio file in seconds (imported recordings)
    var audioDuration: Double?
    /// JSON `[[at, length]]`: where the watch recording was paused (file
    /// seconds, wall-clock seconds), to map file time back to clock time
    var sourcePausesJSON: String?

    // Timed recording and classes (docs/TIMED_RECORDING_AND_CLASSES.md).
    // Optional, so older stores migrate without a schema version.
    /// The class this recording belongs to ("BIO 101 — Cell Biology"); nil = not a class.
    /// User-entered; deleted with the meeting. (Not `className`: Core Data
    /// resolves that key to NSObject's `className`, the object's class name.)
    var courseName: String?
    /// Planned length in minutes ("how long?"); nil = no limit
    var plannedMinutes: Int?

    @Relationship(deleteRule: .cascade, inverse: \Segment.meeting) var segments: [Segment] = []
    @Relationship(deleteRule: .cascade, inverse: \Snapshot.meeting) var snapshots: [Snapshot] = []
    @Relationship(deleteRule: .cascade, inverse: \Attendance.meeting) var attendances: [Attendance] = []
    /// "Stricken from the record" markers (and Delete records during their undo window)
    @Relationship(deleteRule: .cascade, inverse: \Redaction.meeting) var redactions: [Redaction] = []

    init(title: String, startedAt: Date = .now) {
        self.id = UUID()
        self.title = title
        self.startedAt = startedAt
    }

    var duration: TimeInterval? { endedAt.map { $0.timeIntervalSince(startedAt) } }

    var orderedSegments: [Segment] { segments.sorted { $0.start < $1.start } }
    var orderedSnapshots: [Snapshot] { snapshots.sorted { $0.takenAt < $1.takenAt } }

    /// Everyone on the invite except you, organizer first.
    var people: [(person: Person, role: String)] {
        attendances
            .compactMap { a in a.person.map { ($0, a.role) } }
            .filter { !$0.0.isSelf }
            .sorted { ($0.1 == "organizer" ? 0 : 1, $0.0.displayName) < ($1.1 == "organizer" ? 0 : 1, $1.0.displayName) }
    }

    /// Plain-text transcript, for sharing, search and AI prompts. Stricken
    /// spans read `[stricken from the record]`; struck screens appear at their
    /// capture time as `[screen stricken from the record]`.
    var transcriptText: String { RedactionText.plainTranscript(self) }

    /// Strike markers for screens, oldest first.
    var screenStrikes: [Redaction] {
        redactions.filter { $0.isStrike && $0.kind == Redaction.Kind.screen.rawValue }
            .sorted { ($0.coveredFrom ?? $0.createdAt) < ($1.coveredFrom ?? $1.createdAt) }
    }

    func redaction(id: UUID) -> Redaction? { redactions.first { $0.id == id } }

    enum Source { static let watch = "watch" }

    /// Where an imported recording's transcription stands.
    enum ImportState: String {
        /// Waiting its turn (or for the app to come to the foreground)
        case pending
        case transcribing
        /// Stopped with `importError`; Retry starts again where it stopped
        case failed
    }

    var isFromWatch: Bool { source == Source.watch }
    var importPhase: ImportState? { importState.flatMap(ImportState.init(rawValue:)) }

    /// 0…1 while an imported recording is being transcribed
    var importFraction: Double? {
        guard importPhase != nil, let total = audioDuration, total > 0 else { return nil }
        return min(1, max(0, (importProgress ?? 0) / total))
    }
}

/// A finalized stretch of transcript.
@Model
final class Segment {
    /// May contain strike marker tokens (`RedactionText.markerToken`), never the stricken words.
    var text: String
    var start: Date
    var duration: TimeInterval
    var meeting: Meeting?
    /// Where this line starts in the meeting's audio file, in seconds. nil for
    /// lines recorded before word timings were stored.
    var audioOffset: Double?
    /// JSON `[WordTiming]`: per-word audio times, so Delete / Strike can
    /// silence exactly those words. nil when the engine gave none.
    var wordTimingsJSON: String?

    init(text: String, start: Date, duration: TimeInterval, audioOffset: Double? = nil, wordTimings: [WordTiming] = []) {
        self.text = text
        self.start = start
        self.duration = duration
        self.audioOffset = audioOffset
        self.wordTimings = wordTimings
    }

    var wordTimings: [WordTiming] {
        get {
            guard let json = wordTimingsJSON, let data = json.data(using: .utf8) else { return [] }
            return (try? JSONDecoder().decode([WordTiming].self, from: data)) ?? []
        }
        set {
            wordTimingsJSON = newValue.isEmpty ? nil : (try? JSONEncoder().encode(newValue)).flatMap { String(data: $0, encoding: .utf8) }
        }
    }
}

/// One word's place in the segment text (UTF-16 offsets) and in the audio file (seconds).
struct WordTiming: Codable, Equatable, Sendable {
    var location: Int
    var length: Int
    var start: Double
    var end: Double

    var range: NSRange { NSRange(location: location, length: length) }
}

/// One Delete or Strike action (docs/REDACTION.md "Data model"). Records
/// that something was removed, where and when — never what. Strike records
/// are permanent markers; Delete records exist only during the 5-second undo
/// window (so a crash in that window still finishes the purge at next launch).
@Model
final class Redaction {
    enum Kind: String { case words, line, screen }
    enum Action: String { case delete, strike }

    @Attribute(.unique) var id: UUID
    /// words | line | screen
    var kind: String
    /// delete | strike
    var action: String
    /// Seconds into the meeting's audio covered by the removal
    var mediaStart: Double
    var mediaEnd: Double
    /// Wall-clock meeting time covered (for the marker caption)
    var coveredFrom: Date?
    var coveredTo: Date?
    var createdAt: Date
    var reason: String?
    var meeting: Meeting?
    /// Delete records only, while pending: JSON `[[start, end]]` audio ranges
    /// and `[fileName]` photo files still to purge. Positions and file names, never content.
    var pendingAudioJSON: String?
    var pendingFilesJSON: String?

    init(id: UUID = UUID(), kind: Kind, action: Action, mediaStart: Double, mediaEnd: Double,
         coveredFrom: Date?, coveredTo: Date?, reason: String? = nil, createdAt: Date = .now) {
        self.id = id
        self.kind = kind.rawValue
        self.action = action.rawValue
        self.mediaStart = mediaStart
        self.mediaEnd = mediaEnd
        self.coveredFrom = coveredFrom
        self.coveredTo = coveredTo
        self.reason = reason
        self.createdAt = createdAt
    }

    var isStrike: Bool { action == Action.strike.rawValue }
}

/// A photo of a slide, whiteboard or screen taken during a meeting.
@Model
final class Snapshot {
    var fileName: String
    var takenAt: Date
    var meeting: Meeting?

    init(fileName: String, takenAt: Date = .now) {
        self.fileName = fileName
        self.takenAt = takenAt
    }

    var fileURL: URL { Storage.snapshots.appending(path: fileName) }
}

/// Someone from a calendar invite. One per email, shared across meetings.
@Model
final class Person {
    @Attribute(.unique) var email: String
    var name: String?
    var company: String?
    var linkedinURL: String?
    var isSelf: Bool = false
    @Relationship(deleteRule: .cascade, inverse: \Attendance.person) var attendances: [Attendance] = []

    init(email: String, name: String? = nil, company: String? = nil) {
        self.email = email.lowercased()
        self.name = name
        self.company = company
    }

    var displayName: String { name ?? PersonNames.guess(fromEmail: email) }

    var initials: String {
        let parts = displayName.split(whereSeparator: { $0 == " " || $0 == "." || $0 == "-" })
        let letters = parts.prefix(2).compactMap(\.first).map(String.init).joined()
        return letters.isEmpty ? "?" : letters.uppercased()
    }

    var meetings: [Meeting] {
        attendances.compactMap(\.meeting).sorted { $0.startedAt > $1.startedAt }
    }
}

@Model
final class Attendance {
    /// "organizer" or "attendee"
    var role: String
    var meeting: Meeting?
    var person: Person?

    init(role: String) { self.role = role }
}

enum Storage {
    static let documents = URL.documentsDirectory
    static let audio = documents.appending(path: "Audio", directoryHint: .isDirectory)
    static let snapshots = documents.appending(path: "Snapshots", directoryHint: .isDirectory)
    /// Apple Watch recordings received but not yet imported (file + metadata JSON)
    static let watchInbox = documents.appending(path: "WatchInbox", directoryHint: .isDirectory)

    static let modelTypes: [any PersistentModel.Type] = [
        Meeting.self, Segment.self, Snapshot.self, Person.self, Attendance.self, Redaction.self,
    ]

    /// The app's store. Opened in App.init, before any view, because Apple
    /// Watch recordings can arrive while the app runs in the background.
    @MainActor
    static func makeContainer() -> ModelContainer {
        do {
            return try ModelContainer(for: Schema(modelTypes))
        } catch {
            fatalError("Couldn't open the meeting store: \(error.localizedDescription)")
        }
    }

    static func prepare() {
        for dir in [audio, snapshots, watchInbox] {
            try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        }
    }
}

enum PersonNames {
    /// "jane.doe@acme.com" → "Jane Doe"
    static func guess(fromEmail email: String) -> String {
        let local = email.split(separator: "@").first.map(String.init) ?? email
        return local
            .split(whereSeparator: { ".-_+".contains($0) })
            .map { $0.prefix(1).uppercased() + $0.dropFirst() }
            .joined(separator: " ")
    }

    private static let personalDomains: Set<String> = [
        "gmail.com", "googlemail.com", "icloud.com", "me.com", "mac.com", "outlook.com",
        "hotmail.com", "live.com", "yahoo.com", "aol.com", "proton.me", "protonmail.com",
    ]

    /// "jane@acme-corp.com" → "Acme Corp"; nil for personal mail.
    static func company(fromEmail email: String) -> String? {
        guard let domain = email.split(separator: "@").last.map({ String($0).lowercased() }),
              !personalDomains.contains(domain) else { return nil }
        let labels = domain.split(separator: ".")
        guard labels.count >= 2 else { return nil }
        let name = labels[labels.count - 2]
        return name.split(separator: "-").map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " ")
    }
}
