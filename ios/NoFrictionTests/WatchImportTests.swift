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

    func testPartNumbersRoundTripAndAreValidated() throws {
        let meta = sample().forPart(2, of: 3)
        let back = try XCTUnwrap(WatchRecordingMetadata(dictionary: meta.dictionary))
        XCTAssertEqual(back.part, 2)
        XCTAssertEqual(back.partCount, 3)
        XCTAssertNil(WatchRecordingMetadata(dictionary: sample().forPart(3, of: 3).dictionary), "part out of range")
        XCTAssertNil(WatchRecordingMetadata(dictionary: sample().forPart(0, of: 0).dictionary))
        // A single-file recording may omit them
        var d = sample().dictionary
        d.removeValue(forKey: WatchTransfer.Key.part)
        d.removeValue(forKey: WatchTransfer.Key.partCount)
        XCTAssertEqual(WatchRecordingMetadata(dictionary: d)?.partCount, 1)
    }

    func testARecordingPausedOvernightIsValid() {
        let meta = WatchRecordingMetadata(recordingID: UUID(), startedAt: start, endedAt: start.addingTimeInterval(30 * 3600),
                                          duration: 1200, appVersion: "x")
        XCTAssertNotNil(WatchRecordingMetadata(dictionary: meta.dictionary))
    }

    func testPartKeys() {
        let id = UUID()
        let key = WatchTransfer.partKey(id, 3)
        XCTAssertEqual(WatchTransfer.parsePartKey(key)?.id, id)
        XCTAssertEqual(WatchTransfer.parsePartKey(key)?.part, 3)
        XCTAssertNil(WatchTransfer.parsePartKey("nonsense"))
    }

    func testDecodesStagedJSONWithoutNewerFields() throws {
        let json = #"{"recordingID":"6F1C1B8E-2C8B-4E55-9C59-6E1D7C1A0B11","startedAt":0,"endedAt":60,"duration":60}"#
        let meta = try JSONDecoder().decode(WatchRecordingMetadata.self, from: Data(json.utf8))
        XCTAssertEqual(meta.part, 0)
        XCTAssertEqual(meta.partCount, 1)
        XCTAssertEqual(meta.pauses, [])
        XCTAssertEqual(meta.appVersion, "unknown")
    }

    func testTypeNotebookPlanAndMarkersRoundTripAndStayOptional() throws {
        var meta = sample()
        meta.kind = .personal
        meta.notebook = "Health"
        meta.plannedMinutes = 30
        meta.markers = [WatchMarker(kind: .test, at: start.addingTimeInterval(90), offset: 90)]
        let data = try PropertyListSerialization.data(fromPropertyList: meta.dictionary, format: .binary, options: 0)
        let back = try XCTUnwrap(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
        XCTAssertEqual(WatchRecordingMetadata(dictionary: back), meta)
        XCTAssertEqual(try JSONDecoder().decode(WatchRecordingMetadata.self, from: JSONEncoder().encode(meta)), meta,
                       "the inbox's staged JSON keeps them")
        // Without them (an older watch): the same recording, nothing extra
        var old = meta.dictionary
        for key in ["kind", "notebook", "plannedMinutes", "markers"] { old.removeValue(forKey: key) }
        let plain = try XCTUnwrap(WatchRecordingMetadata(dictionary: old))
        XCTAssertEqual(plain.recordingID, meta.recordingID)
        XCTAssertNil(plain.kind)
        XCTAssertEqual(plain.markers, [])
        XCTAssertEqual(meta.dictionary["v"] as? Int, 1, "additive keys keep version 1")
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
    private var logSuite = ""
    private var importLog: ImportedRecordingLog!
    private var mayTranscribeNow = true
    private let start = Date(timeIntervalSince1970: 1_790_000_000)

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        context = container.mainContext
        root = FileManager.default.temporaryDirectory.appending(path: "nf-watch-import-\(UUID().uuidString)", directoryHint: .isDirectory)
        inbox = WatchInbox(directory: root.appending(path: "inbox", directoryHint: .isDirectory))
        audioDir = root.appending(path: "audio", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: audioDir, withIntermediateDirectories: true)
        notified = []
        logSuite = "nf-watch-import-tests-\(UUID().uuidString)"
        importLog = ImportedRecordingLog(defaults: UserDefaults(suiteName: logSuite)!)
        mayTranscribeNow = true
        Storage.prepare()
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: root)
        for url in cleanup { try? FileManager.default.removeItem(at: url) }
        UserDefaults().removePersistentDomain(forName: logSuite)
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
            mayTranscribe: { [weak self] in mayTranscribe && (self?.mayTranscribeNow ?? true) },
            notify: { [weak self] title, _, _ in self?.notified.append(title) },
            importLog: importLog
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

    func testStagingMovesTheFileAndImportCreatesAWatchMeeting() async throws {
        let meta = try stageRecording(seconds: 5)
        XCTAssertEqual(inbox.pending().count, 1)
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        let imported = await importer.processInbox()
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

    func testSameRecordingTwiceMakesOneMeeting() async throws {
        let id = UUID()
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        try stageRecording(seconds: 3, id: id)
        await importer.processInbox()
        // The watch sent it again (e.g. its didFinish was lost): same id
        try stageRecording(seconds: 3, id: id)
        await importer.processInbox()
        XCTAssertEqual(meetings().count, 1)
        XCTAssertEqual(inbox.pending().count, 0, "duplicate discarded")
        let files = try FileManager.default.contentsOfDirectory(atPath: audioDir.path(percentEncoded: false))
        XCTAssertEqual(files.count, 1)
    }

    func testPausedRecordingWaitsForEveryPartThenJoinsThemInOrder() async throws {
        let id = UUID()
        let lengths = [4.0, 6.0]
        let meta = WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(10 + 30),
                                          duration: 10, appVersion: "test", pauses: [.init(at: 4, length: 30)])
        func stage(_ part: Int) throws {
            let file = root.appending(path: "part-\(part)-\(UUID().uuidString).m4a")
            try WatchAudioFixture.write(file, seconds: lengths[part], hz: part == 0 ? 300 : 600)
            try inbox.stage(file, metadata: meta.forPart(part, of: 2))
        }
        let fake = ScriptedTranscriber(chunk: 30) { _, _ in [ScriptedTranscriber.line("Before and after the break.", at: 5)] }
        let importer = makeImporter(fake)

        try stage(1)                                     // parts can arrive in any order
        let none = await importer.processInbox()
        XCTAssertTrue(none.isEmpty, "waits for the missing part")
        XCTAssertTrue(meetings().isEmpty)
        try stage(0)
        await importer.runQueue()

        let m = try XCTUnwrap(meetings().first)
        let url = audioDir.appending(path: try XCTUnwrap(m.audioFileName))
        let file = try AVAudioFile(forReading: url)
        XCTAssertEqual(Double(file.length) / file.processingFormat.sampleRate, 10, accuracy: 0.1, "one file, both parts")
        // Part order kept: 300 Hz first, 600 Hz after second 4
        let (samples, rate, _) = try AudioFixtures.decode(url)
        XCTAssertEqual(WatchAudioFixture.zeroCrossingHz(samples, rate, 1.0...3.0), 300, accuracy: 15)
        XCTAssertEqual(WatchAudioFixture.zeroCrossingHz(samples, rate, 6.0...9.0), 600, accuracy: 15)
        XCTAssertTrue(inbox.pending().isEmpty)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: inbox.directory.path(percentEncoded: false)), [])
        // File second 5 is after the 30 s pause on the clock
        XCTAssertEqual(m.orderedSegments.first?.start.timeIntervalSince(start) ?? -1, 35, accuracy: 0.01)
    }

    func testFinishesAnImportInterruptedAfterSaving() async throws {
        // The app died after saving the meeting, before its audio was placed
        let meta = try stageRecording(seconds: 3)
        let key = meta.recordingID.uuidString
        let saved = Meeting(title: "Saved first", startedAt: meta.startedAt)
        saved.source = Meeting.Source.watch
        saved.sourceRecordingID = key
        saved.audioFileName = "watch-\(key).m4a"
        saved.importState = Meeting.ImportState.failed.rawValue      // a transcription attempt found no file
        saved.importError = "The recording's audio file is missing."
        context.insert(saved)
        try context.save()

        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        await importer.processInbox()
        XCTAssertEqual(meetings().count, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: audioDir.appending(path: "watch-\(key).m4a").path(percentEncoded: false)))
        XCTAssertEqual(saved.importPhase, .pending, "transcription can run now")
        XCTAssertNil(saved.importError)
        XCTAssertTrue(inbox.pending().isEmpty)
    }

    func testDeletedMeetingIsNotBroughtBackByARedelivery() async throws {
        let id = UUID()
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        try stageRecording(seconds: 3, id: id)
        await importer.processInbox()
        let m = try XCTUnwrap(meetings().first)
        // The user deletes the meeting (as MeetingDetailView does)
        try? FileManager.default.removeItem(at: audioDir.appending(path: m.audioFileName!))
        context.delete(m)
        try context.save()
        // The watch sends it again (its confirmation was lost)
        try stageRecording(seconds: 3, id: id)
        await importer.processInbox()
        XCTAssertTrue(meetings().isEmpty, "Delete purges everywhere: a re-delivery stays deleted")
        XCTAssertTrue(inbox.stagedRecordingIDs().isEmpty)
    }

    func testLatePartOfAnImportedRecordingIsDiscarded() async throws {
        let id = UUID()
        let meta = WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(10),
                                          duration: 4, appVersion: "test")
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        for part in 0..<2 {
            let file = root.appending(path: "p\(part)-\(UUID().uuidString).m4a")
            try WatchAudioFixture.write(file, seconds: 2)
            try inbox.stage(file, metadata: meta.forPart(part, of: 2))
        }
        await importer.processInbox()
        XCTAssertEqual(meetings().count, 1)
        // A duplicate of part 1 arrives afterwards: alone it would wait forever
        let again = root.appending(path: "again-\(UUID().uuidString).m4a")
        try WatchAudioFixture.write(again, seconds: 2)
        try inbox.stage(again, metadata: meta.forPart(1, of: 2))
        await importer.processInbox()
        XCTAssertTrue(inbox.stagedRecordingIDs().isEmpty)
        XCTAssertEqual(meetings().count, 1)
    }

    func testIncompleteRecordingIsGivenUpAfterTheLimit() throws {
        final class TestClock: @unchecked Sendable { var now = Date(timeIntervalSince1970: 1_790_000_000) }
        let clock = TestClock()
        let clockInbox = WatchInbox(directory: inbox.directory, clock: { clock.now })
        let meta = WatchRecordingMetadata(recordingID: UUID(), startedAt: start, endedAt: start.addingTimeInterval(10),
                                          duration: 4, appVersion: "test")
        let file = root.appending(path: "lonely-\(UUID().uuidString).m4a")
        try WatchAudioFixture.write(file, seconds: 2)
        try clockInbox.stage(file, metadata: meta.forPart(1, of: 2))
        XCTAssertTrue(clockInbox.pending().isEmpty)
        XCTAssertEqual(clockInbox.stagedRecordingIDs().count, 1, "waits for part 0")
        clock.now = start.addingTimeInterval(WatchInbox.incompleteLimit + 60)
        XCTAssertTrue(clockInbox.pending().isEmpty)
        XCTAssertTrue(clockInbox.stagedRecordingIDs().isEmpty, "audio not kept forever")
    }

    func testYieldsToALiveRecordingBetweenChunks() async throws {
        try stageRecording(seconds: 70)
        let fake = ScriptedTranscriber(chunk: 30) { [unowned self] i, _ in
            self.mayTranscribeNow = false            // the iPhone starts recording mid-import
            return [ScriptedTranscriber.line("Chunk \(i) words here.", at: 1)]
        }
        let importer = makeImporter(fake)
        await importer.runQueue()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(fake.chunkLengths.count, 1, "stopped after the chunk in progress")
        XCTAssertEqual(m.importPhase, .pending)
        XCTAssertEqual(m.segments.count, 1)
    }

    func testRemovesLeftoverTemporaryFiles() throws {
        let joining = audioDir.appending(path: ".joining-\(UUID().uuidString).m4a")
        let silencing = audioDir.appending(path: ".silencing-\(UUID().uuidString).m4a")
        let chunk = FileManager.default.temporaryDirectory.appending(path: "nf-chunk-\(UUID().uuidString).caf")
        let keep = audioDir.appending(path: "watch-\(UUID().uuidString).m4a")
        for url in [joining, silencing, chunk, keep] { FileManager.default.createFile(atPath: url.path(percentEncoded: false), contents: Data([1])) }
        makeImporter(ScriptedTranscriber { _, _ in [] }).removeLeftoverTemporaryFiles()
        XCTAssertFalse([joining, silencing, chunk].contains { FileManager.default.fileExists(atPath: $0.path(percentEncoded: false)) })
        XCTAssertTrue(FileManager.default.fileExists(atPath: keep.path(percentEncoded: false)))
    }

    func testCalendarMatchNamesTheMeetingAndAddsAttendees() async throws {
        try stageRecording(seconds: 60)
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] },
                                    events: [event("Brightwater pilot kickoff", from: -120, minutes: 30),
                                             event("Unrelated later", from: 7200, minutes: 30)])
        await importer.processInbox()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(m.title, "Brightwater pilot kickoff")
        XCTAssertEqual(m.calendarEventID, "evt-Brightwater pilot kickoff")
        XCTAssertEqual(m.location, "Room 4")
        XCTAssertEqual(m.meetingURL, "https://meet.example.com/abc")
        XCTAssertEqual(m.people.map(\.person.displayName), ["Dana Whitfield", "Jonah Kim"], "organizer first, self excluded")
    }

    // MARK: Type, notebook, planned length and markers from the watch

    /// Stage one part of a recording with the given metadata.
    private func stagePart(_ meta: WatchRecordingMetadata, seconds: Double = 2) throws {
        let file = root.appending(path: "part-\(meta.part)-\(UUID().uuidString).m4a")
        try WatchAudioFixture.write(file, seconds: seconds)
        try inbox.stage(file, metadata: meta)
    }

    private func markerCount() -> Int { (try? context.fetchCount(FetchDescriptor<MomentMarker>())) ?? -1 }

    func testImportSetsTypeNotebookPlanAndMarkersAndIsIdempotent() async throws {
        // A notebook the phone already spells "BIO 101"
        let earlier = Meeting(title: "Lecture 2", startedAt: start.addingTimeInterval(-86_400))
        earlier.courseName = "BIO 101"
        earlier.kind = .class
        context.insert(earlier)
        try context.save()

        let id = UUID()
        let star = WatchMarker(kind: .important, at: start.addingTimeInterval(1), offset: 1)
        let test = WatchMarker(kind: .test, at: start.addingTimeInterval(24), offset: 3)   // after a 20 s pause
        let meta = WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(24),
                                          duration: 4, appVersion: "test", pauses: [.init(at: 2, length: 20)],
                                          kind: .class, notebook: "bio 101", plannedMinutes: 75, markers: [star])
        // Markers may arrive with any part: the second part knows one more
        var second = meta.forPart(1, of: 2)
        second.markers = [star, test]
        try stagePart(meta.forPart(0, of: 2))
        try stagePart(second)

        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        await importer.processInbox()
        let m = try XCTUnwrap(meetings().first { $0.sourceRecordingID == id.uuidString })
        XCTAssertEqual(m.recordingKind, "class")
        XCTAssertEqual(m.kind, .class)
        XCTAssertEqual(m.courseName, "BIO 101", "joins the existing notebook's spelling")
        XCTAssertEqual(m.plannedMinutes, 75)
        XCTAssertTrue(m.title.hasPrefix("BIO 101 — "), "no calendar event: named by its notebook, like a live recording")
        XCTAssertEqual(m.orderedMarkers.map(\.id), [star.id, test.id], "the watch's marker ids are kept")
        XCTAssertEqual(m.orderedMarkers.map(\.markerKind), [.important, .test])
        XCTAssertEqual(m.orderedMarkers.map { $0.offset(in: m) }, [1, 24], "wall clock, like the transcript lines")
        XCTAssertEqual(m.orderedMarkers.map(\.label), ["Important", "On the test"])
        XCTAssertTrue(m.orderedMarkers.allSatisfy { $0.note == nil }, "no notes on the watch")

        // Delivered again: nothing doubles
        try stagePart(meta.forPart(0, of: 2))
        try stagePart(second)
        await importer.processInbox()
        XCTAssertEqual(meetings().filter { $0.sourceRecordingID == id.uuidString }.count, 1)
        XCTAssertEqual(markerCount(), 2)

        // The app died after saving, before placing the audio: the next pass
        // finishes it and still adds no marker twice
        try FileManager.default.removeItem(at: audioDir.appending(path: try XCTUnwrap(m.audioFileName)))
        try stagePart(meta.forPart(0, of: 2))
        try stagePart(second)
        await importer.processInbox()
        XCTAssertEqual(markerCount(), 2)
        XCTAssertTrue(FileManager.default.fileExists(atPath: audioDir.appending(path: m.audioFileName!).path(percentEncoded: false)))
        importer.applyWatchFields(second, to: m)
        XCTAssertEqual(markerCount(), 2, "applying twice changes nothing")
    }

    func testUserChangesSurviveALateRedelivery() async throws {
        let id = UUID()
        let mark = WatchMarker(kind: .question, at: start.addingTimeInterval(1))
        let meta = WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(2), duration: 2,
                                          appVersion: "test", kind: .personal, notebook: "Health", markers: [mark])
        let importer = makeImporter(ScriptedTranscriber { _, _ in [] })
        try stagePart(meta)
        await importer.processInbox()
        let m = try XCTUnwrap(meetings().first)
        // The user deletes the marker and moves it to another notebook
        for k in m.markers { context.delete(k) }
        m.courseName = "Family"
        try context.save()
        try stagePart(meta)
        await importer.processInbox()
        XCTAssertEqual(markerCount(), 0, "a duplicate delivery brings nothing back")
        XCTAssertEqual(m.courseName, "Family")
        XCTAssertEqual(m.kind, .personal)
    }

    func testOlderWatchMetadataImportsAsAMeetingWithNothingExtra() async throws {
        // What a watch app from before types sends: only the first-version keys
        let id = UUID()
        let file = root.appending(path: "old-\(UUID().uuidString).m4a")
        try WatchAudioFixture.write(file, seconds: 2)
        let d: [String: Any] = ["recordingId": id.uuidString, "startedAt": start, "endedAt": start.addingTimeInterval(2),
                                "duration": 2.0, "appVersion": "1.0.0 (4)", "pauses": [[Double]](), "part": 0, "parts": 1, "v": 1]
        let meta = try XCTUnwrap(WatchRecordingMetadata(dictionary: d))
        try inbox.stage(file, metadata: meta)
        await makeImporter(ScriptedTranscriber { _, _ in [] }).processInbox()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertNil(m.recordingKind)
        XCTAssertEqual(m.kind, .meeting)
        XCTAssertNil(m.courseName)
        XCTAssertNil(m.plannedMinutes)
        XCTAssertTrue(m.markers.isEmpty)
        XCTAssertTrue(m.title.hasPrefix("Meeting — "))
    }

    func testImportedMarkersAreClampedAndDeletedWithTheRecording() async throws {
        let id = UUID()
        let early = WatchMarker(kind: .important, at: start.addingTimeInterval(-30))
        let late = WatchMarker(kind: .test, at: start.addingTimeInterval(3_600))
        let meta = WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(2), duration: 2,
                                          appVersion: "test", kind: .meeting, markers: [early, late])
        try stagePart(meta)
        await makeImporter(ScriptedTranscriber { _, _ in [] }).processInbox()
        let m = try XCTUnwrap(meetings().first)
        XCTAssertEqual(m.orderedMarkers.map(\.at), [start, start.addingTimeInterval(2)], "kept inside the recording")
        XCTAssertEqual(m.orderedMarkers.last?.label, "Follow up", "a meeting's third kind")
        // Delete Meeting removes them with the rest (cascade)
        context.delete(m)
        try context.save()
        XCTAssertEqual(markerCount(), 0)
    }

    func testInboxMergesWhatAnyPartCarries() {
        let a = WatchMarker(kind: .important, at: start)
        let b = WatchMarker(kind: .question, at: start.addingTimeInterval(5))
        let first = WatchRecordingMetadata(recordingID: UUID(), startedAt: start, endedAt: start, duration: 0, appVersion: "x",
                                           partCount: 2, markers: [a])
        var other = first.forPart(1, of: 2)
        other.kind = .class
        other.notebook = "BIO 101"
        other.plannedMinutes = 30
        other.markers = [b, a]
        let merged = WatchInbox.merged(first, [first, other])
        XCTAssertEqual(merged.markers, [a, b])
        XCTAssertEqual(merged.kind, .class)
        XCTAssertEqual(merged.notebook, "BIO 101")
        XCTAssertEqual(merged.plannedMinutes, 30)
        XCTAssertEqual(merged.part, 0)
    }

    func testNotebookListForTheWatchIsNamesOnly() {
        let context = WatchTransfer.notebookContext(["Acme project", "acme project", " BIO 101 "] + (1...10).map { "N\($0)" })
        XCTAssertEqual(Array(context.keys), ["recentNotebooks"])
        XCTAssertEqual((context["recentNotebooks"] as? [String]).map { Array($0.prefix(2)) }, ["Acme project", "BIO 101"])
        XCTAssertEqual((context["recentNotebooks"] as? [String])?.count, WatchTransfer.maxRecentNotebooks)
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
    static func write(_ url: URL, seconds: Double, hz: Float = 300) throws {
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
        for i in 0..<Int(n) { p[i] = 0.5 * sinf(2 * .pi * hz * Float(i) / Float(rate)) }
        try file.write(from: buf)
    }

    /// Rough frequency of a tone from its zero crossings.
    static func zeroCrossingHz(_ samples: [Float], _ rate: Double, _ range: ClosedRange<Double>) -> Double {
        let a = Int(range.lowerBound * rate), b = min(samples.count - 1, Int(range.upperBound * rate))
        guard a < b else { return 0 }
        var crossings = 0
        for i in (a + 1)...b where (samples[i - 1] < 0) != (samples[i] < 0) { crossings += 1 }
        return Double(crossings) / 2 / (Double(b - a) / rate)
    }
}
