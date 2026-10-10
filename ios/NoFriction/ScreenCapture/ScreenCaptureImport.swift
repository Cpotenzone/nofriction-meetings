import Foundation
import SwiftData
import UIKit

// Screen capture on iPhone and iPad (docs/SCREEN_CAPTURE_IOS.md): the
// choices, the Pro gate for app audio, and the import of what the broadcast
// extension left in the App Group container into a recording.

/// How a line heard from what was playing is labeled.
enum ScreenSpeaker {
    static let label = "On screen"
    /// Before the line's text in exports and AI prompts
    static let prefix = "(On screen) "
}

/// The user's screen-capture choices (this device only).
enum ScreenCapturePrefs {
    static let captureKey = "screenCaptureOnRecord"
    static let appAudioKey = "screenCaptureTranscribePlaying"
    static let notebookKey = "screenCaptureLastNotebook"
    static let hiddenNoticeKey = "screenCaptureHiddenNoticeShown"

    /// "Capture screen" on the Record sheet (remembered like the type and length)
    static func captureOnRecord(_ d: UserDefaults = .standard) -> Bool { d.bool(forKey: captureKey) }
    static func setCaptureOnRecord(_ on: Bool, _ d: UserDefaults = .standard) { d.set(on, forKey: captureKey) }

    /// "Transcribe what's playing": on unless turned off. Only counts for Pro (`ScreenAudioPolicy`).
    static func wantsAppAudio(_ d: UserDefaults = .standard) -> Bool { d.object(forKey: appAudioKey) as? Bool ?? true }
    static func setWantsAppAudio(_ on: Bool, _ d: UserDefaults = .standard) { d.set(on, forKey: appAudioKey) }

    /// The notebook last picked on the Record sheet, for a recording that
    /// starts by itself when screen capture starts from Control Center.
    static func lastNotebook(_ d: UserDefaults = .standard) -> String? { Notebook.normalize(d.string(forKey: notebookKey)) }
    static func rememberNotebook(_ name: String?, _ d: UserDefaults = .standard) {
        if let name = Notebook.normalize(name) { d.set(name, forKey: notebookKey) } else { d.removeObject(forKey: notebookKey) }
    }
}

/// App audio is a Pro feature: the extension writes it, and the app keeps
/// it, only for a Pro user who left "Transcribe what's playing" on.
enum ScreenAudioPolicy {
    /// The Pro feature key the paywall names (docs/PRO.md)
    static let feature = ProFeature.transcribePlaying

    static func allowsAppAudio(isPro: Bool, wantsIt: Bool) -> Bool { isPro && wantsIt }

    /// What the switch shows: never on for a free user.
    static func switchShowsOn(isPro: Bool, wantsIt: Bool) -> Bool { allowsAppAudio(isPro: isPro, wantsIt: wantsIt) }

    /// Turning the switch on as a free user opens the paywall instead.
    static func needsPaywall(turningOn: Bool, isPro: Bool) -> Bool { turningOn && !isPro }

    /// Tell the extension (shared defaults). Anything but true means no app audio.
    static func publish(isPro: Bool, wantsIt: Bool, to defaults: UserDefaults? = ScreenCaptureContract.defaults()) {
        defaults?.set(allowsAppAudio(isPro: isPro, wantsIt: wantsIt), forKey: ScreenCaptureContract.Key.appAudio)
    }
}

/// App audio moved into the app, waiting to be transcribed. File name and times only.
struct PendingScreenAudio: Codable, Equatable, Sendable {
    /// In `Storage.audio`
    var file: String
    /// Wall-clock time of the file's first sample
    var start: Date
    /// Seconds already transcribed (resume point)
    var done: Double = 0
}

extension Meeting {
    /// App audio from screen capture waiting to be transcribed, oldest first.
    var pendingScreenAudio: [PendingScreenAudio] {
        get {
            guard let data = screenAudioJSON?.data(using: .utf8) else { return [] }
            return (try? JSONDecoder().decode([PendingScreenAudio].self, from: data)) ?? []
        }
        set {
            screenAudioJSON = newValue.isEmpty ? nil
                : (try? JSONEncoder().encode(newValue.sorted { $0.start < $1.start })).flatMap { String(data: $0, encoding: .utf8) }
        }
    }

    var screens: [Snapshot] { orderedSnapshots.filter(\.isScreen) }
}

/// Which broadcast belongs to which recording (ids only), so files that
/// arrive after a recording stopped, or after the app was closed, land in
/// the right one, and Delete Recording can find them.
struct ScreenCaptureLinks: @unchecked Sendable {
    let defaults: UserDefaults
    static let key = "screenCaptureLinks"
    static let shared = ScreenCaptureLinks(defaults: .standard)

    private var all: [String: String] { defaults.dictionary(forKey: Self.key) as? [String: String] ?? [:] }

    func meetingID(for broadcast: UUID) -> UUID? { all[broadcast.uuidString].flatMap(UUID.init(uuidString:)) }

    func broadcasts(for meeting: UUID) -> [UUID] {
        all.filter { $0.value == meeting.uuidString }.compactMap { UUID(uuidString: $0.key) }
    }

    func link(_ broadcast: UUID, to meeting: UUID) {
        var d = all
        d[broadcast.uuidString] = meeting.uuidString
        defaults.set(d, forKey: Self.key)
    }

    func unlink(_ broadcast: UUID) {
        var d = all
        d.removeValue(forKey: broadcast.uuidString)
        defaults.set(d, forKey: Self.key)
    }
}

/// An "On screen" line that only repeats what the microphone already heard
/// (the video played out loud) is dropped, so the transcript doesn't say
/// everything twice. The microphone line stays: it has the audio.
enum ScreenTranscriptMerge {
    /// Microphone lines within this many seconds count as the same moment
    static let window: TimeInterval = 5
    /// Share of the screen line's words the microphone heard too
    static let echoShare = 0.7
    /// Shorter lines are never treated as echoes
    static let minWords = 3

    static func words(_ text: String) -> [String] {
        RedactionText.plain(text).lowercased()
            .split(whereSeparator: { !$0.isLetter && !$0.isNumber && $0 != "'" })
            .map(String.init)
    }

    static func isEcho(_ text: String, start: Date, end: Date, microphone: [(text: String, start: Date, end: Date)]) -> Bool {
        let mine = words(text)
        guard mine.count >= minWords else { return false }
        let lo = start.addingTimeInterval(-window), hi = end.addingTimeInterval(window)
        let heard = Set(microphone.filter { $0.end >= lo && $0.start <= hi }.flatMap { words($0.text) })
        guard !heard.isEmpty else { return false }
        let hits = mine.filter(heard.contains).count
        return Double(hits) / Double(mine.count) >= echoShare
    }
}

/// Moves a broadcast's screens and app audio into a recording, and
/// transcribes the app audio on the device.
@MainActor
final class ScreenCaptureImporter {
    struct Environment {
        /// The App Group's ScreenCapture folder (nil: App Group unavailable)
        var root: URL?
        var snapshotsDirectory: URL
        var audioDirectory: URL
        var makeTranscriber: @MainActor () -> FileTranscriber
        var isForeground: @MainActor () -> Bool
        /// Pro and "Transcribe what's playing" on, now
        var appAudioAllowed: @MainActor () -> Bool
        var speechLevel: Float = MeetingEndDetector.Config().speechLevel
        var now: () -> Date = Date.init
    }

    /// One broadcast's folder
    struct Session {
        let folder: URL
        let manifest: ScreenCaptureManifest?
        var key: String { folder.lastPathComponent }
    }

    let context: ModelContext
    var env: Environment

    init(context: ModelContext, env: Environment) {
        self.context = context
        self.env = env
    }

    // MARK: Folders

    /// Every broadcast folder, oldest first.
    func sessions() -> [Session] {
        guard let root = env.root else { return [] }
        let folders = (try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)) ?? []
        return folders.filter(\.hasDirectoryPath)
            .map { Session(folder: $0, manifest: ScreenCaptureManifest.load(from: $0)) }
            .sorted { ($0.manifest?.startedAt ?? .distantPast) < ($1.manifest?.startedAt ?? .distantPast) }
    }

    func session(id: UUID) -> Session? { sessions().first { $0.manifest?.id == id } }

    /// Screens waiting in a folder, oldest first.
    func frames(in session: Session) -> [(name: String, at: Date)] {
        let names = (try? FileManager.default.contentsOfDirectory(atPath: session.folder.path(percentEncoded: false))) ?? []
        return names.compactMap { n in ScreenCaptureContract.frameDate(fromName: n).map { (n, $0) } }.sorted { $0.1 < $1.1 }
    }

    /// Anything worth a recording: a screen or a finished stretch of app audio.
    func hasContent(_ session: Session) -> Bool {
        !frames(in: session).isEmpty || (session.manifest?.parts.contains { $0.end != nil } ?? false)
    }

    /// Delete a broadcast's folder and everything in it.
    func remove(_ session: Session) {
        try? FileManager.default.removeItem(at: session.folder)
    }

    // MARK: Import

    /// Move a broadcast's finished files into `meeting`: screens become
    /// `Snapshot`s marked as screens, at their capture time; complete app
    /// audio waits in `Storage.audio` for transcription. Screens after
    /// `keepUntil` (captured after the recording stopped) are deleted, not
    /// imported. Safe to run again: a moved file is gone from the folder.
    /// Returns how many screens were added.
    @discardableResult
    func importFiles(of session: Session, into meeting: Meeting, keepUntil: Date? = nil) -> Int {
        let fm = FileManager.default
        try? fm.createDirectory(at: env.snapshotsDirectory, withIntermediateDirectories: true)
        try? fm.createDirectory(at: env.audioDirectory, withIntermediateDirectories: true)
        var added = 0
        for frame in frames(in: session) {
            let source = session.folder.appending(path: frame.name)
            if let keepUntil, frame.at > keepUntil {
                try? fm.removeItem(at: source)
                continue
            }
            let name = "screen-\(UUID().uuidString).jpg"
            guard (try? fm.moveItem(at: source, to: env.snapshotsDirectory.appending(path: name))) != nil else { continue }
            let snapshot = Snapshot(fileName: name, takenAt: frame.at)
            snapshot.source = Snapshot.Source.screen
            context.insert(snapshot)
            snapshot.meeting = meeting
            added += 1
        }

        let live = session.manifest?.isLive(at: env.now()) ?? false
        let allowed = env.appAudioAllowed() && session.manifest?.appAudio == true
        var pending = meeting.pendingScreenAudio
        for part in session.manifest?.parts ?? [] {
            let source = session.folder.appending(path: part.file)
            guard fm.fileExists(atPath: source.path(percentEncoded: false)) else { continue }
            guard part.end != nil else {
                // Still being written, or cut off when the extension was killed (unreadable)
                if !live { try? fm.removeItem(at: source) }
                continue
            }
            if let keepUntil, part.start > keepUntil {
                try? fm.removeItem(at: source)
                continue
            }
            guard allowed else {
                // Not Pro (any more), or the option is off: app audio is never kept
                try? fm.removeItem(at: source)
                continue
            }
            let name = "screen-\(meeting.id.uuidString)-\(UUID().uuidString.prefix(8)).\(ScreenCaptureContract.audioExtension)"
            guard (try? fm.moveItem(at: source, to: env.audioDirectory.appending(path: name))) != nil else { continue }
            pending.append(PendingScreenAudio(file: name, start: part.start))
        }
        meeting.pendingScreenAudio = pending
        try? context.save()
        return added
    }

    /// A broadcast with no recording to go to (it started while noFriction
    /// wasn't recording and ended before the app could start one): its own
    /// recording, with the remembered type and notebook and no microphone audio.
    func makeRecording(for manifest: ScreenCaptureManifest, events: (Date, Date) -> [CalendarEventInfo] = { _, _ in [] }) -> Meeting {
        let kind = RecordingKindStore.remembered()
        let notebook = ScreenCapturePrefs.lastNotebook()
        let start = manifest.startedAt
        let end = max(manifest.endedAt ?? manifest.heartbeat, start)
        let title = notebook.map { RecordingSession.notebookTitle($0, at: start) } ?? RecordingSession.defaultTitle(for: start, kind: kind)
        let meeting = Meeting(title: title, startedAt: start)
        meeting.endedAt = end
        meeting.kind = kind
        meeting.courseName = notebook
        context.insert(meeting)
        if let event = CalendarMatching.bestEvent(start: start, end: end, in: events(start.addingTimeInterval(-3600), end.addingTimeInterval(3600))) {
            MeetingLinker.link(meeting, to: event, in: context)
        }
        try? context.save()
        return meeting
    }

    // MARK: Transcription ("Transcribe what's playing")

    /// Transcribe `meeting`'s waiting app audio on the device, oldest file
    /// first, from where it stopped. Lines pass `TranscriptFilter`, skip
    /// echoes of the microphone, and become segments marked as from the
    /// screen, at wall-clock time, so they interleave with the microphone's.
    /// Each file is deleted once transcribed. Returns false if it stopped
    /// early (it resumes on the next call).
    @discardableResult
    func transcribePending(meetingID id: UUID, mayContinue: @MainActor () -> Bool = { true }) async -> Bool {
        guard let first = meeting(id: id), !first.pendingScreenAudio.isEmpty else { return true }
        let transcriber = env.makeTranscriber()
        do {
            try await transcriber.prepare(interactive: env.isForeground())
        } catch {
            return false
        }
        while let m = meeting(id: id), let part = m.pendingScreenAudio.first {
            let url = env.audioDirectory.appending(path: part.file)
            guard FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) else {
                m.pendingScreenAudio = Array(m.pendingScreenAudio.dropFirst())
                try? context.save()
                continue
            }
            let levels = try? await Task.detached(priority: .utility) { try AudioChunks.levels(of: url) }.value
            guard let levels else {
                // Unreadable: nothing can be transcribed from it
                try? FileManager.default.removeItem(at: url)
                if let m = meeting(id: id) { m.pendingScreenAudio = Array(m.pendingScreenAudio.dropFirst()); try? context.save() }
                continue
            }
            let chunks = AudioChunks.plan(from: part.done, duration: levels.duration,
                                          maxChunk: transcriber.preferredChunkSeconds, levels: levels)
            var previous: String?
            for chunk in chunks {
                if Task.isCancelled || !mayContinue() { return false }
                let temp = FileManager.default.temporaryDirectory.appending(path: "nf-chunk-\(UUID().uuidString).caf")
                defer { try? FileManager.default.removeItem(at: temp) }
                let lines: [TranscribedLine]
                do {
                    try await Task.detached(priority: .utility) { try AudioChunks.export(url, range: chunk, to: temp) }.value
                    lines = try await transcriber.transcribe(fileAt: temp, vocabulary: [])
                } catch {
                    return false
                }
                guard let m = meeting(id: id) else { return true }   // deleted meanwhile
                let microphone = m.segments.filter { !$0.isFromScreen }
                    .map { (text: $0.text, start: $0.start, end: $0.start.addingTimeInterval(max($0.duration, 0))) }
                for line in lines.sorted(by: { $0.start < $1.start }) {
                    let from = chunk.lowerBound + max(0, line.start)
                    let to = chunk.lowerBound + max(line.end, line.start)
                    let nearSilence = levels.peak(max(0, from - 1)...(to + 1)).map { $0 < env.speechLevel } ?? false
                    guard case .keep(let text, _, _) = TranscriptFilter.clean(line.text, words: line.words, nearSilence: nearSilence, previous: previous)
                    else { continue }
                    let start = part.start.addingTimeInterval(from)
                    let end = part.start.addingTimeInterval(to)
                    guard !ScreenTranscriptMerge.isEcho(text, start: start, end: end, microphone: microphone) else { continue }
                    // No audio offset or word timings: this audio isn't kept
                    let segment = Segment(text: text, start: start, duration: max(0, to - from))
                    segment.source = Snapshot.Source.screen
                    context.insert(segment)
                    segment.meeting = m
                    previous = text
                }
                var queue = m.pendingScreenAudio
                if let i = queue.firstIndex(where: { $0.file == part.file }) { queue[i].done = chunk.upperBound }
                m.pendingScreenAudio = queue
                try? context.save()
            }
            // Transcribed: the app audio itself is not kept
            try? FileManager.default.removeItem(at: url)
            if let m = meeting(id: id) {
                m.pendingScreenAudio = m.pendingScreenAudio.filter { $0.file != part.file }
                try? context.save()
            }
        }
        return true
    }

    /// Recordings with app audio still to transcribe, oldest first.
    func meetingsWithPendingAudio() -> [Meeting] {
        let all = (try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.screenAudioJSON != nil },
                                                               sortBy: [SortDescriptor(\.startedAt)]))) ?? []
        return all.filter { !$0.pendingScreenAudio.isEmpty }
    }

    func meeting(id: UUID) -> Meeting? {
        try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.id == id })).first
    }

    // MARK: Purge (docs/REDACTION.md)

    /// Delete Recording: its waiting app audio, and every broadcast folder
    /// linked to it in the App Group container.
    func purge(meeting: Meeting, links: ScreenCaptureLinks) {
        for part in meeting.pendingScreenAudio {
            try? FileManager.default.removeItem(at: env.audioDirectory.appending(path: part.file))
        }
        meeting.screenAudioJSON = nil
        for id in links.broadcasts(for: meeting.id) {
            if let session = session(id: id) { remove(session) }
            links.unlink(id)
        }
    }

    /// At launch: app audio files no recording waits for (the app died
    /// between moving and saving), and folders of broadcasts whose
    /// recording was deleted.
    func removeLeftovers(links: ScreenCaptureLinks) {
        let fm = FileManager.default
        let wanted = Set(meetingsWithPendingAudio().flatMap { $0.pendingScreenAudio.map(\.file) })
        for url in (try? fm.contentsOfDirectory(at: env.audioDirectory, includingPropertiesForKeys: nil)) ?? []
        where url.lastPathComponent.hasPrefix("screen-") && !wanted.contains(url.lastPathComponent) {
            try? fm.removeItem(at: url)
        }
        for session in sessions() {
            guard let id = session.manifest?.id, let meetingID = links.meetingID(for: id), meeting(id: meetingID) == nil else { continue }
            remove(session)
            links.unlink(id)
        }
    }
}

extension ScreenCaptureImporter.Environment {
    @MainActor
    static func live(store: Store) -> Self {
        Self(
            root: ScreenCaptureContract.root(),
            snapshotsDirectory: Storage.snapshots,
            audioDirectory: Storage.audio,
            makeTranscriber: { FileTranscribers.best() },
            isForeground: { UIApplication.shared.applicationState == .active },
            appAudioAllowed: { [weak store] in
                ScreenAudioPolicy.allowsAppAudio(isPro: store?.isPro ?? false, wantsIt: ScreenCapturePrefs.wantsAppAudio())
            }
        )
    }
}
