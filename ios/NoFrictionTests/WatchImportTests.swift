import AVFoundation
import SwiftData
import XCTest
@testable import noFriction

// Apple Watch → iPhone import (docs/WATCH_APP.md). The real Speech framework
// doesn't run in the Simulator, so transcription uses a scripted fake.

final class WatchMetadataTests: XCTestCase {
    private let start = Date(timeIntervalSince1970: 1_790_000_000)

    private func sample() -> WatchRecordingMetadata {
        WatchRecordingMetadata(recordingID: UUID(), startedAt: start, endedAt: start.addingTimeInterval(3_700),
                               duration: 3_600, appVersion: "1.0.0 (4)", pauses: [.init(at: 600, length: 100)])
    }

    func testDictionaryRoundTrip() {
        let meta = sample()
        XCTAssertEqual(WatchRecordingMetadata(dictionary: meta.dictionary), meta)
    }

    func testSurvivesPropertyListEncoding() throws {
        // WCSession metadata must be a property list
        let meta = sample()
        let data = try PropertyListSerialization.data(fromPropertyList: meta.dictionary, format: .binary, options: 0)
        let back = try XCTUnwrap(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
        XCTAssertEqual(WatchRecordingMetadata(dictionary: back), meta)
    }

    func testCodableRoundTrip() throws {
        let meta = sample()
        let back = try JSONDecoder().decode(WatchRecordingMetadata.self, from: JSONEncoder().encode(meta))
        XCTAssertEqual(back, meta)
    }

    func testRejectsMissingOrInvalidFields() {
        var d = sample().dictionary
        d[WatchTransfer.Key.recordingID] = "not-a-uuid"
        XCTAssertNil(WatchRecordingMetadata(dictionary: d))

        d = sample().dictionary
        d.removeValue(forKey: WatchTransfer.Key.startedAt)
        XCTAssertNil(WatchRecordingMetadata(dictionary: d))

        d = sample().dictionary
        d[WatchTransfer.Key.endedAt] = start.addingTimeInterval(-1)
        XCTAssertNil(WatchRecordingMetadata(dictionary: d), "ends before it starts")

        d = sample().dictionary
        d[WatchTransfer.Key.duration] = Double.nan
        XCTAssertNil(WatchRecordingMetadata(dictionary: d))

        d = sample().dictionary
        d[WatchTransfer.Key.duration] = -5.0
        XCTAssertNil(WatchRecordingMetadata(dictionary: d))
    }

    func testToleratesExtraKeysIntegersAndMissingOptionals() throws {
        var d = sample().dictionary
        d["futureField"] = "ignored"
        d[WatchTransfer.Key.duration] = 3_600            // Int, not Double
        d.removeValue(forKey: WatchTransfer.Key.appVersion)
        d.removeValue(forKey: WatchTransfer.Key.pauses)
        let meta = try XCTUnwrap(WatchRecordingMetadata(dictionary: d))
        XCTAssertEqual(meta.duration, 3_600)
        XCTAssertEqual(meta.appVersion, "unknown")
        XCTAssertEqual(meta.pauses, [])
    }

    func testWallClockAddsPausesBeforeTheOffset() {
        let meta = sample()
        XCTAssertEqual(meta.wallClock(atFileOffset: 0), start)
        XCTAssertEqual(meta.wallClock(atFileOffset: 599), start.addingTimeInterval(599))
        XCTAssertEqual(meta.wallClock(atFileOffset: 601), start.addingTimeInterval(701))
    }

    func testSharedRecordingNoticeText() {
        XCTAssertEqual(RecordingNoticeSheet.text, RecordingNotice.text)
        XCTAssertTrue(RecordingNotice.text.contains("Tell participants"))
    }
}

/// Splitting recognizer text into transcript lines.
final class LineSplitterTests: XCTestCase {
    private func words(_ text: String, times: [(Double, Double)]) -> [WordTiming] {
        let toks = RedactionText.tokens(text)
        return zip(toks, times).map { t, s in WordTiming(location: t.range.location, length: t.range.length, start: s.0, end: s.1) }
    }

    func testBreaksAtLongPausesAndKeepsTimings() {
        let text = "Okay let's start. The budget is approved"
        let w = words(text, times: [(0, 0.3), (0.4, 0.6), (0.7, 1.0), (3.0, 3.2), (3.3, 3.6), (3.7, 3.8), (3.9, 4.4)])
        let lines = LineSplitter.split(text: text, words: w)
        XCTAssertEqual(lines.map(\.text), ["Okay let's start.", "The budget is approved"])
        XCTAssertEqual(lines[1].start, 3.0)
        XCTAssertEqual(lines[1].end, 4.4)
        let ns = lines[1].text as NSString
        XCTAssertEqual(lines[1].words.map { ns.substring(with: $0.range) }, ["The", "budget", "is", "approved"])
    }

    func testNoTimingsNoLines() {
        XCTAssertEqual(LineSplitter.split(text: "hello", words: []), [])
    }
}

final class ChunkPlanTests: XCTestCase {
    func testCutsAtTheQuietestPointBeforeTheLimit() {
        // 0.1 s windows over 100 s: loud, with a quiet dip at 26.0 s
        var values = [Float](repeating: 0.8, count: 1000)
        values[260] = 0.05
        let levels = AudioChunks.Levels(values: values, window: 0.1, duration: 100)
        let chunks = AudioChunks.plan(from: 0, duration: 100, maxChunk: 30, levels: levels)
        XCTAssertEqual(chunks.first?.lowerBound, 0)
        XCTAssertEqual(chunks.first?.upperBound ?? 0, 26.05, accuracy: 0.001)
        XCTAssertEqual(chunks.last?.upperBound, 100)
        // Contiguous, none over the limit
        for (a, b) in zip(chunks, chunks.dropFirst()) { XCTAssertEqual(a.upperBound, b.lowerBound) }
        XCTAssertTrue(chunks.allSatisfy { $0.upperBound - $0.lowerBound <= 30 + 0.001 })
    }

    func testResumesFromCheckpoint() {
        let chunks = AudioChunks.plan(from: 60, duration: 100, maxChunk: 30, levels: nil)
        XCTAssertEqual(chunks, [60..<90, 90..<100])
        XCTAssertEqual(AudioChunks.plan(from: 100, duration: 100, maxChunk: 30, levels: nil), [])
    }
}

/// Fake on-device transcriber: returns scripted lines per chunk. Main-actor
/// isolated, so scripts can touch the test's SwiftData context safely.
@MainActor
final class ScriptedTranscriber: FileTranscriber {
    nonisolated let name = "Scripted"
    nonisolated let preferredChunkSeconds: Double
    var prepareError: Error?
    var script: (Int, Double) throws -> [TranscribedLine]
    private(set) var chunkLengths: [Double] = []
    private(set) var vocabulary: [String] = []
    private(set) var prepareInteractive: [Bool] = []

    init(chunk: Double = 30, script: @escaping (Int, Double) throws -> [TranscribedLine]) {
        preferredChunkSeconds = chunk
        self.script = script
    }

    func prepare(interactive: Bool) async throws {
        prepareInteractive.append(interactive)
        if let prepareError { throw prepareError }
    }

    func transcribe(fileAt url: URL, vocabulary: [String]) async throws -> [TranscribedLine] {
        let file = try AVAudioFile(forReading: url)
        let length = Double(file.length) / file.processingFormat.sampleRate
        let index = chunkLengths.count
        chunkLengths.append(length)
        self.vocabulary = vocabulary
        return try script(index, length)
    }

    /// A line at `at` seconds into the chunk, 0.4 s per word.
    nonisolated static func line(_ text: String, at: Double) -> TranscribedLine {
        let toks = RedactionText.tokens(text)
        let words = toks.enumerated().map { i, t in
            WordTiming(location: t.range.location, length: t.range.length, start: at + Double(i) * 0.4, end: at + Double(i) * 0.4 + 0.3)
        }
        return TranscribedLine(text: text, start: at, end: words.last?.end ?? at, words: words)
    }
}

@MainActor
final class WatchImporterTests: XCTestCase {
    private var container: ModelContainer!
    private var context: ModelContext!
    private var root: URL!
    private var inbox: WatchInbox!
    private var audioDir: URL!
    private var notified: [String] = []
    private var cleanup: [URL] = []
    private let start = Date(timeIntervalSince1970: 1_790_000_000)

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        context = container.mainContext
        root = FileManager.default.temporaryDirectory.appending(path: "nf-watch-import-\(UUID().uuidString)", directoryHint: .isDirectory)
        inbox = WatchInbox(directory: root.appending(path: "inbox", directoryHint: .isDirectory))
        audioDir = root.appending(path: "audio", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: audioDir, withIntermediateDirectories: true)
        notified = []
        Storage.prepare()
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: root)
        for url in cleanup { try? FileManager.default.removeItem(at: url) }
        container = nil
        context = nil
    }

    private func makeImporter(_ transcriber: ScriptedTranscriber, events: [CalendarEventInfo] = [],
                              foreground: Bool = true, mayTranscribe: Bool = true, audioDirectory: URL? = nil) -> WatchImporter {
        WatchImporter(context: context, env: .init(
            inbox: inbox,
            audioDirectory: audioDirectory ?? audioDir,
            events: { from, to in events.filter { $0.end > from && $0.start < to } },
            makeTranscriber: { transcriber },
            isForeground: { foreground },
            mayTranscribe: { mayTranscribe },
            notify: { [weak self] title, _, _ in self?.notified.append(title) }
        ))
    }

    /// A watch-format recording (AAC mono 16 kHz) staged in the inbox, like
    /// the WatchConnectivity delegate does.
    @discardableResult
    private func stageRecording(seconds: Double = 70, id: UUID = UUID(), pauses: [WatchRecordingMetadata.Pause] = []) throws -> WatchRecordingMetadata {
        let file = root.appending(path: "incoming-\(UUID().uuidString).m4a")
        try WatchAudioFixture.write(file, seconds: seconds)
        let paused = pauses.reduce(0) { $0 + $1.length }
        let meta = WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(seconds + paused),
                                          duration: seconds, appVersion: "test", pauses: pauses)
        try inbox.stage(file, metadata: meta)
        return meta
    }

    private func event(_ title: String, from: Double, minutes: Double) -> CalendarEventInfo {
        CalendarEventInfo(id: "evt-\(title)", title: title, start: start.addingTimeInterval(from),
                          end: start.addingTimeInterval(from + minutes * 60), isAllDay: false,
                          location: "Room 4", notes: nil, url: "https://meet.example.com/abc",
                          participants: [
                              .init(email: "dana@brightwater.example", name: "Dana Whitfield", isOrganizer: true, isSelf: false),
                              .init(email: "jonah@brightwater.example", name: "Jonah Kim", isOrganizer: false, isSelf: false),
                              .init(email: "me@lumen-labs.example", name: "Me", isOrganizer: false, isSelf: true),
                          ])
    }

    private func meetings() -> [Meeting] { (try? context.fetch(FetchDescriptor<Meeting>())) ?? [] }

    // MARK: Import

    func testStagingMovesTheFileAndImportCreatesAWatchMeeting() throws {
        let meta = try stageRecording(seconds: 5)
        XCTAssertEqual(inbox.pending().count, 1)
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        let imported = importer.processInbox()
        XCTAssertEqual(imported.count, 1)
        let m = try XCTUnwrap(meetings().first)
        XCTAssertTrue(m.isFromWatch)
        XCTAssertEqual(m.sourceRecordingID, meta.recordingID.uuidString)
        XCTAssertEqual(m.startedAt, meta.startedAt)
        XCTAssertEqual(m.endedAt, meta.endedAt)
        XCTAssertEqual(m.importPhase, .pending)
        XCTAssertTrue(RecordingSession.isDefaultTitle(m.title), "no calendar event: default title, so a later backfill can rename it")
        // The imported file is the meeting's audio file
        let audio = audioDir.appending(path: try XCTUnwrap(m.audioFileName))
        XCTAssertTrue(FileManager.default.fileExists(atPath: audio.path(percentEncoded: false)))
        XCTAssertEqual(inbox.pending().count, 0, "inbox cleared")
    }

    func testSameRecordingTwiceMakesOneMeeting() throws {
        let id = UUID()
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        try stageRecording(seconds: 3, id: id)
        importer.processInbox()
        // The watch sent it again (e.g. its didFinish was lost): same id
        try stageRecording(seconds: 3, id: id)
        importer.processInbox()
        XCTAssertEqual(meetings().count, 1)
        XCTAssertEqual(inbox.pending().count, 0, "duplicate discarded")
        let files = try FileManager.default.contentsOfDirectory(atPath: audioDir.path(percentEncoded: false))
        XCTAssertEqual(files.count, 1)
    }

    func testCalendarMatchNamesTheMeetingAndAddsAttendees() throws {
        try stageRecording(seconds: 60)
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] },
                                    events: [event("Brightwater pilot kickoff", from: -120, minutes: 30),
                                             event("Unrelated later", from: 7200, minutes: 30)])
        importer.processInbox()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(m.title, "Brightwater pilot kickoff")
        XCTAssertEqual(m.calendarEventID, "evt-Brightwater pilot kickoff")
        XCTAssertEqual(m.location, "Room 4")
        XCTAssertEqual(m.meetingURL, "https://meet.example.com/abc")
        XCTAssertEqual(m.people.map(\.person.displayName), ["Dana Whitfield", "Jonah Kim"], "organizer first, self excluded")
    }

    // MARK: Transcription

    func testTranscribesInChunksWithOffsetsFilterAndClockMapping() async throws {
        // Paused for 60 s on the watch at file second 20
        try stageRecording(seconds: 70, pauses: [.init(at: 20, length: 60)])
        let fake = ScriptedTranscriber(chunk: 30) { i, _ in
            var lines = [ScriptedTranscriber.line("Chunk \(i) has real words in it.", at: 2)]
            if i == 1 { lines.append(ScriptedTranscriber.line("Bye-bye. Bye-bye. Bye-bye.", at: 10)) }   // hallucination loop
            return lines
        }
        let importer = makeImporter(fake, events: [event("Weekly sync", from: 0, minutes: 30)])
        await importer.runQueue()

        let m = try XCTUnwrap(meetings().first)
        XCTAssertNil(m.importState, "done")
        XCTAssertNil(m.importError)
        XCTAssertEqual(fake.chunkLengths.count, 3)
        XCTAssertEqual(fake.chunkLengths.reduce(0, +), 70, accuracy: 0.2, "chunks cover the file once")
        XCTAssertEqual(fake.vocabulary, ["Brightwater", "Dana Whitfield", "Jonah Kim"], "attendee names prime the recognizer")

        let segs = m.orderedSegments
        XCTAssertEqual(segs.map(\.text), ["Chunk 0 has real words in it.", "Chunk 1 has real words in it.", "Chunk 2 has real words in it."],
                       "the bye-bye loop is filtered")
        let secondChunkStart = fake.chunkLengths[0]
        XCTAssertEqual(segs[0].audioOffset ?? -1, 2, accuracy: 0.01)
        XCTAssertEqual(segs[1].audioOffset ?? -1, secondChunkStart + 2, accuracy: 0.01)
        XCTAssertEqual(segs[0].start.timeIntervalSince(start), 2, accuracy: 0.01)
        XCTAssertEqual(segs[1].start.timeIntervalSince(start), secondChunkStart + 2 + 60, accuracy: 0.01, "after the pause")
        // Word timings are in file seconds, for Delete / Strike
        let ns = segs[1].text as NSString
        XCTAssertEqual(segs[1].wordTimings.first.map { ns.substring(with: $0.range) }, "Chunk")
        XCTAssertEqual(segs[1].wordTimings.first?.start ?? -1, secondChunkStart + 2, accuracy: 0.01)
        XCTAssertEqual(m.importProgress ?? 0, m.audioDuration ?? -1, accuracy: 0.01)
        XCTAssertEqual(notified, ["Weekly sync"], "one 'Watch recording added' notification")
    }

    func testResumesFromCheckpointAfterBeingCutShort() async throws {
        try stageRecording(seconds: 70)
        var importer: WatchImporter!
        let first = ScriptedTranscriber(chunk: 30) { i, _ in
            if i == 1 {
                importer.cancel()                  // background time ran out mid-chunk
                throw CancellationError()
            }
            return [ScriptedTranscriber.line("First run chunk \(i) words.", at: 1)]
        }
        importer = makeImporter(first)
        importer.resume()
        await importer.waitUntilIdle()

        var m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(m.importPhase, .pending, "cut short: waits, not failed")
        XCTAssertEqual(m.segments.count, 1)
        let checkpoint = try XCTUnwrap(m.importProgress)
        XCTAssertEqual(checkpoint, first.chunkLengths[0], accuracy: 0.01)

        // Relaunch after the app was killed mid-run
        m.importState = Meeting.ImportState.transcribing.rawValue
        try context.save()
        let second = ScriptedTranscriber(chunk: 30) { i, _ in [ScriptedTranscriber.line("Second run chunk \(i) words.", at: 1)] }
        let relaunched = makeImporter(second)
        XCTAssertEqual(relaunched.pendingMeetings().count, 1, "a run killed mid-way is still pending")
        await relaunched.runQueue()

        m = try XCTUnwrap(meetings().first)
        XCTAssertNil(m.importState)
        XCTAssertEqual(second.chunkLengths.count, 2, "only the rest of the file")
        XCTAssertEqual(m.orderedSegments.map(\.text), ["First run chunk 0 words.", "Second run chunk 0 words.", "Second run chunk 1 words."])
        XCTAssertEqual(m.orderedSegments[1].audioOffset ?? -1, checkpoint + 1, accuracy: 0.01)
    }

    func testWaitsForForegroundWhenPermissionCantBeAsked() async throws {
        try stageRecording(seconds: 5)
        let fake = ScriptedTranscriber { _, _ in [] }
        fake.prepareError = FileTranscriptionError.needsForeground
        let importer = makeImporter(fake, foreground: false)
        await importer.runQueue()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(m.importPhase, .pending)
        XCTAssertEqual(fake.prepareInteractive, [false], "never prompts from the background")
        XCTAssertTrue(notified.isEmpty)
    }

    func testFailureShowsMessageAndRetryFinishes() async throws {
        try stageRecording(seconds: 5)
        let fake = ScriptedTranscriber { _, _ in throw TranscriptionError.onDeviceUnavailable }
        let importer = makeImporter(fake)
        await importer.runQueue()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(m.importPhase, .failed)
        XCTAssertEqual(m.importError, TranscriptionError.onDeviceUnavailable.errorDescription)

        fake.script = { _, _ in [ScriptedTranscriber.line("Now it works fine.", at: 0.5)] }
        importer.retry(m)
        await importer.waitUntilIdle()
        XCTAssertNil(m.importState)
        XCTAssertEqual(m.orderedSegments.map(\.text), ["Now it works fine."])
    }

    func testWaitsWhileThePhoneIsRecording() async throws {
        try stageRecording(seconds: 5)
        let fake = ScriptedTranscriber { _, _ in [ScriptedTranscriber.line("Hello there everyone.", at: 0)] }
        let importer = makeImporter(fake, mayTranscribe: false)
        await importer.runQueue()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(m.importPhase, .pending, "imported, but transcription waits")
        XCTAssertTrue(fake.chunkLengths.isEmpty)
    }

    func testDeletedMeetingStopsTheRun() async throws {
        try stageRecording(seconds: 70)
        var importer: WatchImporter!
        let fake = ScriptedTranscriber(chunk: 30) { [unowned self] i, _ in
            if i == 0, let m = self.meetings().first {
                self.context.delete(m)
                try self.context.save()
            }
            return [ScriptedTranscriber.line("Words after delete.", at: 1)]
        }
        importer = makeImporter(fake)
        await importer.runQueue()
        XCTAssertTrue(meetings().isEmpty)
        XCTAssertEqual(fake.chunkLengths.count, 1, "nothing more transcribed for a deleted meeting")
        XCTAssertTrue(((try? context.fetch(FetchDescriptor<Segment>())) ?? []).isEmpty)
    }

    // MARK: Delete / Strike on a watch recording

    func testStrikeSilencesTheImportedWatchAudio() async throws {
        // RedactionEngine works on Storage.audio, so import there
        let meta = try stageRecording(seconds: 6)
        let fake = ScriptedTranscriber(chunk: 30) { _, _ in [ScriptedTranscriber.line("alpha beta gamma delta", at: 1)] }
        let importer = makeImporter(fake, audioDirectory: Storage.audio)
        await importer.runQueue()
        let m = try XCTUnwrap(meetings().first)
        let url = Storage.audio.appending(path: try XCTUnwrap(m.audioFileName))
        cleanup.append(url)
        XCTAssertEqual(m.audioFileName, "watch-\(meta.recordingID.uuidString).m4a")
        let frames = try AVAudioFile(forReading: url).length

        // "gamma": 1.8–2.1 s → silenced 1.65–2.25 s
        let seg = try XCTUnwrap(m.orderedSegments.first)
        try await RedactionEngine.strike(.words(seg, 2...2), reason: nil, meeting: m, context: context)

        XCTAssertFalse(seg.text.contains("gamma"))
        let (samples, rate, length) = try AudioFixtures.decode(url)
        XCTAssertEqual(rate, WatchTransfer.sampleRate)
        XCTAssertEqual(length, frames, "duration changed")
        XCTAssertEqual(AudioFixtures.peak(samples, rate, 1.8...2.1), 0, "struck word silenced in the phone's copy")
        XCTAssertGreaterThan(AudioFixtures.peak(samples, rate, 0.2...1.2), 0.1)
        XCTAssertGreaterThan(AudioFixtures.peak(samples, rate, 3.0...5.5), 0.1)
    }

    func testSilencerHandlesTheWatchFormat() throws {
        let url = root.appending(path: "watch.m4a")
        try WatchAudioFixture.write(url, seconds: 3)
        let frames = try AVAudioFile(forReading: url).length
        try AudioSilencer.silence(url, ranges: [1.0...2.0])
        let (samples, rate, length) = try AudioFixtures.decode(url)
        XCTAssertEqual(length, frames)
        let edge = 2048.0 / rate
        XCTAssertEqual(AudioFixtures.peak(samples, rate, (1.0 + edge)...(2.0 - edge)), 0)
        XCTAssertGreaterThan(AudioFixtures.peak(samples, rate, 0.1...0.8), 0.1)
    }

    func testBitRateCandidatesStartWithTheFilesOwn() {
        XCTAssertEqual(AudioSilencer.bitRateCandidates(existing: 32_000, channels: 1), [32_000, 64_000, 48_000, 24_000, 16_000])
        XCTAssertEqual(AudioSilencer.bitRateCandidates(existing: nil, channels: 1).first, 64_000)
    }
}

/// Writes audio exactly as the watch records it.
enum WatchAudioFixture {
    static func write(_ url: URL, seconds: Double) throws {
        let rate = WatchTransfer.sampleRate
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: rate,
            AVNumberOfChannelsKey: WatchTransfer.channels, AVEncoderBitRateKey: WatchTransfer.bitRate,
        ]
        let file = try AVAudioFile(forWriting: url, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
        let n = AVAudioFrameCount(seconds * rate)
        let buf = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: n))
        buf.frameLength = n
        let p = buf.floatChannelData![0]
        for i in 0..<Int(n) { p[i] = 0.5 * sinf(2 * .pi * 300 * Float(i) / Float(rate)) }
        try file.write(from: buf)
    }
}
