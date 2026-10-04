import Foundation
import Observation
import SwiftData
import UIKit
import UserNotifications

/// Turns Apple Watch recordings into ordinary meetings (docs/WATCH_APP.md):
///
/// 1. **Import** (fast, also in the background): the staged file moves into
///    `Storage.audio` and becomes the meeting's audio file, a `Meeting` is
///    created from the watch's times, and the calendar match names it and
///    adds the attendees. Idempotent by recording id.
/// 2. **Transcribe** (queued): the file is transcribed on the device in
///    chunks, lines pass through `TranscriptFilter`, and segments are saved
///    with audio offsets and word timings, so Delete / Strike silence exactly
///    the right audio. Progress is saved after every chunk; a run cut short
///    (app closed, background time over) resumes where it stopped.
///
/// From then on the meeting is like any other: AI notes, follow-up email,
/// Delete / Strike, export and People all work on it unchanged.
@MainActor
@Observable
final class WatchImporter {
    struct Environment {
        var inbox: WatchInbox
        var audioDirectory: URL
        /// Calendar events overlapping a window (EventKit; injected in tests)
        var events: @MainActor (Date, Date) -> [CalendarEventInfo]
        var makeTranscriber: @MainActor () -> FileTranscriber
        /// The app is on screen: permission prompts are allowed, and a
        /// failure is reported instead of quietly retried later
        var isForeground: @MainActor () -> Bool
        /// False while this iPhone is recording a meeting itself
        var mayTranscribe: @MainActor () -> Bool
        /// "Watch recording added" (title, length, meeting id)
        var notify: @MainActor (String, TimeInterval?, UUID) async -> Void
        /// Below this level a filler-only line counts as heard on silence
        var speechLevel: Float = MeetingEndDetector.Config().speechLevel
    }

    let context: ModelContext
    var env: Environment
    /// The meeting being transcribed right now
    private(set) var activeMeetingID: UUID?
    private var runner: Task<Void, Never>?
    private var anotherPass = false
    private var completions: [() -> Void] = []

    init(context: ModelContext, env: Environment) {
        self.context = context
        self.env = env
    }

    var isRunning: Bool { runner != nil }

    // MARK: Import

    /// Import everything staged in the inbox. Returns the new or existing meetings.
    @discardableResult
    func processInbox() -> [Meeting] {
        env.inbox.removeOrphans()
        var out: [Meeting] = []
        for item in env.inbox.pending() {
            do {
                out.append(try importRecording(item.metadata, audio: item.audioURLs))
            } catch {
                // Left in the inbox; tried again on the next pass
                continue
            }
        }
        return out
    }

    /// One recording → one meeting. `audio` is its parts in order (one file
    /// unless it was paused); they become the meeting's one audio file. A
    /// recording id seen before returns the existing meeting and discards
    /// the duplicate files.
    ///
    /// Order: the meeting is saved first, then the audio is placed, then the
    /// inbox is cleared. If the app dies in between, the next pass finds the
    /// meeting without its audio and finishes placing it.
    @discardableResult
    func importRecording(_ metadata: WatchRecordingMetadata, audio: [URL]) throws -> Meeting {
        let key = metadata.recordingID.uuidString
        // The imported file is the meeting's audio file: Delete / Strike
        // silence it in place, Delete Meeting removes it
        let fileName = "watch-\(key).\(WatchTransfer.fileExtension)"
        let target = env.audioDirectory.appending(path: fileName)
        let hasAudio = FileManager.default.fileExists(atPath: target.path(percentEncoded: false))

        let record: Meeting
        if let existing = self.meeting(sourceRecordingID: key) {
            guard !hasAudio else {
                // Delivered again after its import: a duplicate
                env.inbox.remove(metadata.recordingID)
                for url in audio { try? FileManager.default.removeItem(at: url) }
                return existing
            }
            record = existing
        } else {
            record = Meeting(title: RecordingSession.defaultTitle(for: metadata.startedAt), startedAt: metadata.startedAt)
            record.endedAt = metadata.endedAt
            record.source = Meeting.Source.watch
            record.sourceRecordingID = key
            record.audioFileName = fileName
            record.importState = Meeting.ImportState.pending.rawValue
            record.importProgress = 0
            record.audioDuration = metadata.duration
            record.sourcePausesJSON = Self.encodePauses(metadata.pauses)
            context.insert(record)
            // Same matching rules as a recording made on this iPhone
            let events = env.events(metadata.startedAt.addingTimeInterval(-3600), metadata.endedAt.addingTimeInterval(3600))
            if let event = CalendarMatching.bestEvent(start: metadata.startedAt, end: metadata.endedAt, in: events) {
                MeetingLinker.link(record, to: event, in: context)
            }
            try context.save()
        }

        if !hasAudio {
            try FileManager.default.createDirectory(at: env.audioDirectory, withIntermediateDirectories: true)
            if audio.count == 1, let only = audio.first {
                try FileManager.default.moveItem(at: only, to: target)
            } else {
                try AudioChunks.join(audio, to: target)
                for url in audio { try? FileManager.default.removeItem(at: url) }
            }
            // A transcription attempt that ran before the audio was in place
            if record.importPhase == .failed {
                record.importState = Meeting.ImportState.pending.rawValue
                record.importError = nil
                try? context.save()
            }
        }
        env.inbox.remove(metadata.recordingID)
        return record
    }

    // MARK: Queue

    /// Start working through the queue (inbox, then pending transcriptions)
    /// unless already running. `completion` runs when the queue stops.
    func resume(completion: (() -> Void)? = nil) {
        if let completion { completions.append(completion) }
        guard runner == nil else {
            anotherPass = true
            return
        }
        runner = Task { [weak self] in
            guard let self else { return }
            repeat {
                self.anotherPass = false
                await self.runQueue()
            } while self.anotherPass && !Task.isCancelled
            self.runner = nil
            let done = self.completions
            self.completions = []
            done.forEach { $0() }
        }
    }

    /// Stop after the current step (background time running out). The
    /// interrupted meeting goes back to pending and resumes later.
    func cancel() {
        runner?.cancel()
    }

    /// Wait for the current run (tests).
    func waitUntilIdle() async {
        while let runner { await runner.value }
    }

    /// One pass: import the inbox, then transcribe each pending meeting once.
    func runQueue() async {
        processInbox()
        var attempted = Set<UUID>()
        while !Task.isCancelled, env.mayTranscribe(), let next = nextPending(excluding: attempted) {
            attempted.insert(next)
            await transcribe(meetingID: next)
        }
    }

    /// At launch: failed transcriptions get one more automatic try.
    func retryFailedOnLaunch() {
        let failed = Meeting.ImportState.failed.rawValue
        let meetings = (try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.importState == failed }))) ?? []
        for m in meetings { m.importState = Meeting.ImportState.pending.rawValue }
        if !meetings.isEmpty { try? context.save() }
    }

    /// The Retry button.
    func retry(_ meeting: Meeting) {
        meeting.importState = Meeting.ImportState.pending.rawValue
        meeting.importError = nil
        try? context.save()
        resume()
    }

    /// Waiting or in progress, oldest first ("transcribing" left by a killed run counts).
    func pendingMeetings() -> [Meeting] {
        let pending = Meeting.ImportState.pending.rawValue
        let transcribing = Meeting.ImportState.transcribing.rawValue
        let descriptor = FetchDescriptor<Meeting>(
            predicate: #Predicate { $0.importState == pending || $0.importState == transcribing },
            sortBy: [SortDescriptor(\.startedAt)])
        return (try? context.fetch(descriptor)) ?? []
    }

    private func nextPending(excluding: Set<UUID>) -> UUID? {
        pendingMeetings().first { !excluding.contains($0.id) }?.id
    }

    // MARK: Transcription

    /// Transcribe one imported meeting from its checkpoint to the end.
    func transcribe(meetingID id: UUID) async {
        guard let m = meeting(id: id), let name = m.audioFileName else { return }
        let url = env.audioDirectory.appending(path: name)
        guard FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) else {
            fail(id, "The recording's audio file is missing.")
            return
        }
        activeMeetingID = id
        defer { activeMeetingID = nil }
        setState(id, .transcribing)

        let transcriber = env.makeTranscriber()
        do {
            try await transcriber.prepare(interactive: env.isForeground())
        } catch {
            settle(id, error)
            return
        }

        let levels = try? await Task.detached(priority: .utility) { try AudioChunks.levels(of: url) }.value
        guard let m = meeting(id: id) else { return }
        if let levels { m.audioDuration = levels.duration }
        let duration = m.audioDuration ?? 0
        let chunks = AudioChunks.plan(from: m.importProgress ?? 0, duration: duration,
                                      maxChunk: transcriber.preferredChunkSeconds, levels: levels)
        let vocabulary = Self.vocabulary(for: m)
        let clock = WatchRecordingMetadata(recordingID: m.id, startedAt: m.startedAt, endedAt: m.endedAt ?? m.startedAt,
                                           duration: duration, appVersion: "", pauses: Self.decodePauses(m.sourcePausesJSON))
        var previous = m.orderedSegments.last.map { RedactionText.plain($0.text) }
        try? context.save()

        for chunk in chunks {
            if Task.isCancelled { setState(id, .pending); return }
            let temp = FileManager.default.temporaryDirectory.appending(path: "nf-chunk-\(UUID().uuidString).caf")
            defer { try? FileManager.default.removeItem(at: temp) }
            let lines: [TranscribedLine]
            do {
                try await Task.detached(priority: .utility) { try AudioChunks.export(url, range: chunk, to: temp) }.value
                lines = try await transcriber.transcribe(fileAt: temp, vocabulary: vocabulary)
            } catch {
                settle(id, error)
                return
            }
            // Deleted while this chunk was being transcribed: nothing to save
            guard let m = meeting(id: id) else { return }
            for line in lines.sorted(by: { $0.start < $1.start }) {
                let start = chunk.lowerBound + max(0, line.start)
                let end = chunk.lowerBound + max(line.end, line.start)
                let words = line.words.map {
                    WordTiming(location: $0.location, length: $0.length, start: $0.start + chunk.lowerBound, end: $0.end + chunk.lowerBound)
                }
                let peak = levels?.peak(max(0, start - 1)...(end + 1))
                let nearSilence = peak.map { $0 < env.speechLevel } ?? false
                switch TranscriptFilter.clean(line.text, words: words, nearSilence: nearSilence, previous: previous) {
                case .drop:
                    continue
                case .keep(let text, let kept, _):
                    let segment = Segment(text: text, start: clock.wallClock(atFileOffset: start),
                                          duration: max(0, end - start), audioOffset: start, wordTimings: kept)
                    context.insert(segment)
                    segment.meeting = m
                    previous = text
                }
            }
            // Checkpoint: this chunk's lines and the resume point land together
            m.importProgress = chunk.upperBound
            try? context.save()
        }

        guard let m = meeting(id: id) else { return }
        m.importState = nil
        m.importError = nil
        m.importProgress = m.audioDuration
        try? context.save()
        await env.notify(m.title, m.duration, m.id)
    }

    // MARK: Helpers

    /// Errors: in the foreground they're shown (failed, with Retry); in the
    /// background, or when cut short, the meeting waits for the next run.
    private func settle(_ id: UUID, _ error: Error) {
        if Task.isCancelled || error is CancellationError || (error as? FileTranscriptionError) == .needsForeground || !env.isForeground() {
            setState(id, .pending)
        } else {
            fail(id, error.localizedDescription)
        }
    }

    private func fail(_ id: UUID, _ message: String) {
        guard let m = meeting(id: id) else { return }
        m.importState = Meeting.ImportState.failed.rawValue
        m.importError = message
        try? context.save()
    }

    private func setState(_ id: UUID, _ state: Meeting.ImportState) {
        guard let m = meeting(id: id) else { return }
        m.importState = state.rawValue
        if state != .failed { m.importError = nil }
        try? context.save()
    }

    private func meeting(id: UUID) -> Meeting? {
        try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.id == id })).first
    }

    private func meeting(sourceRecordingID key: String) -> Meeting? {
        try? context.fetch(FetchDescriptor<Meeting>(predicate: #Predicate { $0.sourceRecordingID == key })).first
    }

    /// Attendee names and companies help the recognizer spell them.
    static func vocabulary(for meeting: Meeting) -> [String] {
        var words: [String] = []
        for (person, _) in meeting.people {
            if let name = person.name { words.append(name) }
            if let company = person.company { words.append(company) }
        }
        return Array(Set(words)).sorted()
    }

    static func encodePauses(_ pauses: [WatchRecordingMetadata.Pause]) -> String? {
        guard !pauses.isEmpty else { return nil }
        return (try? JSONEncoder().encode(pauses.map { [$0.at, $0.length] })).flatMap { String(data: $0, encoding: .utf8) }
    }

    static func decodePauses(_ json: String?) -> [WatchRecordingMetadata.Pause] {
        guard let data = json?.data(using: .utf8), let pairs = try? JSONDecoder().decode([[Double]].self, from: data) else { return [] }
        return pairs.compactMap { $0.count == 2 ? .init(at: $0[0], length: $0[1]) : nil }
    }
}

/// "Watch recording added", only if notifications are already allowed
/// (the app never asks for this one).
enum WatchImportNotifier {
    static let category = "WATCH_IMPORT"

    @MainActor
    static func post(title: String, duration: TimeInterval?, meetingID: UUID) async {
        let center = UNUserNotificationCenter.current()
        let settings = await center.notificationSettings()
        let allowed: Set<UNAuthorizationStatus> = [.authorized, .provisional, .ephemeral]
        guard allowed.contains(settings.authorizationStatus) else { return }
        let content = UNMutableNotificationContent()
        content.title = "Watch recording added"
        content.body = [title, duration?.minutesLabel].compactMap { $0 }.joined(separator: " · ") + " — transcribed on this iPhone."
        content.categoryIdentifier = category
        content.sound = .default
        try? await center.add(UNNotificationRequest(identifier: "watch-import-\(meetingID.uuidString)", content: content, trigger: nil))
    }
}

extension WatchImporter.Environment {
    /// The app's real dependencies.
    @MainActor
    static func live(session: RecordingSession) -> Self {
        Self(
            inbox: .shared,
            audioDirectory: Storage.audio,
            events: { from, to in
                CalendarService.shared.isAuthorized ? CalendarService.shared.events(from: from, to: to) : []
            },
            makeTranscriber: { FileTranscribers.best() },
            isForeground: { UIApplication.shared.applicationState == .active },
            // The phone's own recording comes first (one recognizer at a time)
            mayTranscribe: { [weak session] in !(session?.isActive ?? false) },
            notify: { title, duration, id in await WatchImportNotifier.post(title: title, duration: duration, meetingID: id) }
        )
    }
}
