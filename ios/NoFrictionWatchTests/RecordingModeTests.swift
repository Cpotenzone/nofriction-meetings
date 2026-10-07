import WatchKit
import XCTest
@testable import NoFrictionWatch

/// "How long?" on the watch: the same wall-clock rules as the iPhone, in
/// the testable state machine.
final class WatchTimeLimitTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_790_000_000)
    private func at(_ s: TimeInterval) -> Date { t0.addingTimeInterval(s) }

    func testWarnsFiveMinutesAheadThenStopsAtTheDeadline() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0), limit: .minutes(30))
        XCTAssertEqual(m.deadline, at(30 * 60))
        XCTAssertEqual(m.plannedMinutes, 30)
        XCTAssertEqual(m.timeLeft(at: at(60)) ?? -1, 29 * 60, accuracy: 0.001)
        XCTAssertEqual(m.tickLimit(at: at(25 * 60 - 1)), .none)
        XCTAssertEqual(m.tickLimit(at: at(25 * 60)), .warn(secondsLeft: 300))
        XCTAssertTrue(m.warned)
        XCTAssertEqual(m.tickLimit(at: at(25 * 60 + 1)), .none, "warns once")
        XCTAssertEqual(m.tickLimit(at: at(30 * 60)), .stop)
        XCTAssertEqual(m.tickLimit(at: at(30 * 60 + 1)), .none, "asks to stop once")
    }

    func testPausingDoesNotMoveTheDeadline() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0), limit: .minutes(30))
        try m.pause(at: at(10 * 60))
        XCTAssertEqual(m.timeLeft(at: at(20 * 60)) ?? -1, 10 * 60, accuracy: 0.001, "the clock runs while paused")
        try m.resume(at: at(20 * 60))
        XCTAssertEqual(m.deadline, at(30 * 60))
        try m.pause(at: at(29 * 60))
        XCTAssertEqual(m.tickLimit(at: at(30 * 60)), .stop, "stops at the deadline even while paused")
    }

    func testFifteenMinutePlanWarnsTwoMinutesAhead() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0), limit: .minutes(15))
        XCTAssertEqual(m.tickLimit(at: at(12 * 60)), .none)
        XCTAssertEqual(m.tickLimit(at: at(13 * 60)), .warn(secondsLeft: 120))
    }

    func testExtendMovesTheDeadlineAndRearmsTheWarning() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0), limit: .minutes(30))
        XCTAssertEqual(m.tickLimit(at: at(26 * 60)), .warn(secondsLeft: 240))
        XCTAssertTrue(m.extendLimit(at: at(26 * 60)))
        XCTAssertEqual(m.deadline, at(45 * 60))
        XCTAssertEqual(m.plannedMinutes, 45)
        XCTAssertFalse(m.warned, "a new deadline gets its own warning")
        XCTAssertEqual(m.tickLimit(at: at(30 * 60)), .none, "the old deadline is gone")
        XCTAssertEqual(m.tickLimit(at: at(40 * 60)), .warn(secondsLeft: 300))
        XCTAssertEqual(m.tickLimit(at: at(45 * 60)), .stop)
    }

    func testExtendAfterTheDeadlineCountsFromNow() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0), limit: .minutes(15))
        XCTAssertEqual(m.tickLimit(at: at(15 * 60 + 5)), .stop)
        XCTAssertTrue(m.extendLimit(at: at(15 * 60 + 5)))
        XCTAssertEqual(m.deadline, at(30 * 60 + 5))
        XCTAssertEqual(m.tickLimit(at: at(16 * 60)), .none, "no longer stopping")
    }

    func testRemoveLimitCancelsTheStop() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0), limit: .minutes(60))
        XCTAssertTrue(m.removeLimit())
        XCTAssertNil(m.deadline)
        XCTAssertNil(m.plannedMinutes)
        XCTAssertNil(m.timeLeft(at: at(10)))
        XCTAssertEqual(m.tickLimit(at: at(10 * 3600)), .none)
        XCTAssertFalse(m.removeLimit(), "already no limit")
        XCTAssertFalse(m.extendLimit(at: at(100)), "nothing to extend")
    }

    func testNoLimitNeverStopsAndIdleNeverTicks() throws {
        var idle = RecorderStateMachine()
        XCTAssertEqual(idle.tickLimit(at: at(0)), .none)
        XCTAssertFalse(idle.extendLimit(at: at(0)))
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        XCTAssertNil(m.deadline)
        XCTAssertEqual(m.tickLimit(at: at(24 * 3600)), .none)
    }

    func testStopCarriesTypeNotebookPlanAndMarkers() throws {
        var m = RecorderStateMachine()
        let id = UUID()
        try m.start(id: id, at: at(0), kind: .class, notebook: "  BIO   101 ", limit: .minutes(60))
        m.extendLimit(at: at(10))
        let star = try XCTUnwrap(m.mark(.important, at: at(30)))
        let test = try XCTUnwrap(m.mark(.test, at: at(90)))
        let meta = try m.stop(at: at(120), appVersion: "x")
        XCTAssertEqual(meta.kind, .class)
        XCTAssertEqual(meta.notebook, "BIO 101")
        XCTAssertEqual(meta.plannedMinutes, 75)
        XCTAssertEqual(meta.markers, [star, test])
        XCTAssertEqual(meta.recordingID, id)
    }
}

/// Moments marked on the watch: wall-clock time plus the audio offset, and
/// the contract's mapping turns one into the other.
final class WatchMarkerTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_790_000_000)
    private func at(_ s: TimeInterval) -> Date { t0.addingTimeInterval(s) }

    func testMarkKeepsWallClockAndAudioOffset() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        try m.pause(at: at(10))
        try m.resume(at: at(30))
        let marker = try XCTUnwrap(m.mark(.question, at: at(40)))
        XCTAssertEqual(marker.at, at(40))
        XCTAssertEqual(marker.offset ?? -1, 20, accuracy: 0.001, "20 s of audio before it; 20 s of pause excluded")
        let meta = try m.stop(at: at(50), appVersion: "x")
        XCTAssertEqual(meta.wallClock(atFileOffset: marker.offset!), marker.at, "the contract maps the offset back to the same time")
    }

    func testNoMarkWhileIdleOrPausedAndRepeatsAreOneMark() throws {
        var m = RecorderStateMachine()
        XCTAssertNil(m.mark(at: at(0)), "not recording")
        try m.start(id: UUID(), at: at(0))
        XCTAssertNotNil(m.mark(.important, at: at(5)))
        XCTAssertNil(m.mark(.important, at: at(5.5)), "a double tap is one mark")
        XCTAssertNotNil(m.mark(.test, at: at(5.6)), "another kind right after is its own mark")
        XCTAssertNotNil(m.mark(.important, at: at(6)))
        try m.pause(at: at(7))
        XCTAssertNil(m.mark(at: at(8)), "nothing is recorded while paused")
        XCTAssertEqual(m.markers.map(\.kind), [.important, .test, .important])
        XCTAssertEqual(MarkerKind.default, .important, "one tap marks Important")
    }

    func testMarkersAreCapped() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        for i in 0..<(WatchTransfer.maxMarkers + 5) { m.mark(i % 2 == 0 ? .important : .question, at: at(Double(i))) }
        XCTAssertEqual(m.markers.count, WatchTransfer.maxMarkers)
    }

    func testThirdKindIsLabeledByType() {
        XCTAssertEqual(MarkerKind.test.label(for: .class), "On the test")
        XCTAssertEqual(MarkerKind.test.label(for: .meeting), "Follow up")
        XCTAssertEqual(MarkerKind.test.label(for: .personal), "Remember")
        XCTAssertEqual(MarkerKind.test.rawValue, "test", "the stored value never changes")
    }
}

/// The transfer metadata with the optional keys, both directions.
final class StudentMetadataTests: XCTestCase {
    private let start = Date(timeIntervalSince1970: 1_790_000_000)

    private func full() -> WatchRecordingMetadata {
        WatchRecordingMetadata(recordingID: UUID(), startedAt: start, endedAt: start.addingTimeInterval(3_700),
                               duration: 3_600, appVersion: "1.0.0 (5)", pauses: [.init(at: 600, length: 100)],
                               kind: .class, notebook: "BIO 101", plannedMinutes: 75,
                               markers: [WatchMarker(kind: .important, at: start.addingTimeInterval(60), offset: 60),
                                         WatchMarker(kind: .test, at: start.addingTimeInterval(800), offset: 700)])
    }

    /// The first-version parser, as an iPhone app from before types has it
    /// (only the keys it knew): a new watch's metadata must still import there.
    private func parseLikeVersion1(_ d: [String: Any]) -> (id: UUID, start: Date, end: Date, duration: Double, part: Int, parts: Int)? {
        guard let id = (d["recordingId"] as? String).flatMap(UUID.init(uuidString:)),
              let s = d["startedAt"] as? Date, let e = d["endedAt"] as? Date,
              let duration = d["duration"] as? Double, duration >= 0, e >= s else { return nil }
        let part = (d["part"] as? Int) ?? 0, parts = (d["parts"] as? Int) ?? 1
        guard parts >= 1, part >= 0, part < parts else { return nil }
        if let pauses = d["pauses"], !(pauses is [[Double]]) { return nil }
        return (id, s, e, duration, part, parts)
    }

    func testNewKeysRoundTripThroughAPropertyList() throws {
        let meta = full()
        let data = try PropertyListSerialization.data(fromPropertyList: meta.dictionary, format: .binary, options: 0)
        let back = try XCTUnwrap(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
        XCTAssertEqual(WatchRecordingMetadata(dictionary: back), meta)
        XCTAssertEqual(back[WatchTransfer.Key.kind] as? String, "class")
        XCTAssertEqual(back[WatchTransfer.Key.notebook] as? String, "BIO 101")
        XCTAssertEqual(back[WatchTransfer.Key.plannedMinutes] as? Int, 75)
        XCTAssertEqual((back[WatchTransfer.Key.markers] as? [[String: Any]])?.count, 2)
        // And through JSON (the iPhone's inbox and the watch's index)
        XCTAssertEqual(try JSONDecoder().decode(WatchRecordingMetadata.self, from: JSONEncoder().encode(meta)), meta)
    }

    func testAnOlderIPhoneStillImportsTheNewMetadata() throws {
        let meta = full().forPart(1, of: 2)
        let d = meta.dictionary
        let old = try XCTUnwrap(parseLikeVersion1(d), "the first-version keys keep their names and types")
        XCTAssertEqual(old.id, meta.recordingID)
        XCTAssertEqual(old.part, 1)
        XCTAssertEqual(old.parts, 2)
        XCTAssertEqual(d[WatchTransfer.Key.version] as? Int, 1, "additive: still version 1")
        XCTAssertEqual(WatchTransfer.metadataVersion, 1)
    }

    func testOlderWatchMetadataWithoutTheNewKeysStillDecodes() throws {
        var d = full().dictionary
        for key in [WatchTransfer.Key.kind, WatchTransfer.Key.notebook, WatchTransfer.Key.plannedMinutes, WatchTransfer.Key.markers] {
            d.removeValue(forKey: key)
        }
        let meta = try XCTUnwrap(WatchRecordingMetadata(dictionary: d))
        XCTAssertNil(meta.kind)
        XCTAssertNil(meta.notebook)
        XCTAssertNil(meta.plannedMinutes)
        XCTAssertEqual(meta.markers, [])
        // A plain recording adds none of the optional keys
        let plain = WatchRecordingMetadata(recordingID: UUID(), startedAt: start, endedAt: start, duration: 0, appVersion: "x")
        XCTAssertEqual(Set(plain.dictionary.keys), ["recordingId", "startedAt", "endedAt", "duration", "appVersion", "pauses", "part", "parts", "v"])
        // Staged JSON from before the new fields
        let json = #"{"recordingID":"6F1C1B8E-2C8B-4E55-9C59-6E1D7C1A0B11","startedAt":0,"endedAt":60,"duration":60}"#
        let staged = try JSONDecoder().decode(WatchRecordingMetadata.self, from: Data(json.utf8))
        XCTAssertNil(staged.kind)
        XCTAssertEqual(staged.markers, [])
    }

    func testBadOptionalValuesAreDroppedNotTheRecording() throws {
        var d = full().dictionary
        d[WatchTransfer.Key.kind] = "lecture"
        d[WatchTransfer.Key.notebook] = "  " + String(repeating: "n", count: 200)
        d[WatchTransfer.Key.plannedMinutes] = 0
        d[WatchTransfer.Key.markers] = [
            ["id": "not-a-uuid", "kind": "important", "at": start],
            ["id": UUID().uuidString, "kind": "future-kind", "at": start.addingTimeInterval(5)],
            "garbage",
        ] as [Any]
        let meta = try XCTUnwrap(WatchRecordingMetadata(dictionary: d), "the recording still imports")
        XCTAssertNil(meta.kind)
        XCTAssertEqual(meta.notebook?.count, Notebook.maxLength)
        XCTAssertNil(meta.plannedMinutes)
        XCTAssertEqual(meta.markers.count, 1)
        XCTAssertEqual(meta.markers.first?.kind, .important, "an unknown kind keeps the moment as Important")
        d[WatchTransfer.Key.plannedMinutes] = 10_000
        XCTAssertNil(WatchRecordingMetadata(dictionary: d)?.plannedMinutes)
        d[WatchTransfer.Key.plannedMinutes] = Double.nan
        XCTAssertNil(WatchRecordingMetadata(dictionary: d)?.plannedMinutes)
    }

    func testMarkersMergeByIdInTimeOrder() {
        let a = WatchMarker(kind: .important, at: start.addingTimeInterval(30))
        let b = WatchMarker(kind: .test, at: start.addingTimeInterval(10))
        let merged = WatchMarker.merged([[a, b], [b, a], [a]])
        XCTAssertEqual(merged, [b, a])
    }

    func testNotebookContextIsNamesOnlyCleanedAndCapped() {
        let names = ["  BIO 101 ", "bio 101", "", "HIST 200"] + (1...20).map { "Course \($0)" }
        let context = WatchTransfer.notebookContext(names)
        XCTAssertEqual(Array(context.keys), [WatchTransfer.Key.recentNotebooks], "nothing but the names")
        let sent = context[WatchTransfer.Key.recentNotebooks] as? [String] ?? []
        XCTAssertEqual(sent.count, WatchTransfer.maxRecentNotebooks)
        XCTAssertEqual(Array(sent.prefix(2)), ["BIO 101", "HIST 200"])
        XCTAssertEqual(WatchTransfer.notebooks(fromContext: [WatchTransfer.Key.recentNotebooks: [1, " Health ", "health"] as [Any]]), ["Health"])
        XCTAssertEqual(WatchTransfer.notebooks(fromContext: ["other": "x"]), [])
        XCTAssertEqual(WatchTransfer.notebooks(fromContext: context), sent)
    }
}

/// The store keeps the type, notebook, plan and markers while recording
/// (crash recovery), sends them with every part, and drops the notebook
/// name and the markers once the iPhone has everything.
@MainActor
final class StudentStoreTests: XCTestCase {
    private var dir: URL!

    override func setUp() async throws {
        dir = URL.temporaryDirectory.appending(path: "nf-watch-student-\(UUID().uuidString)", directoryHint: .isDirectory)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
    }

    private let start = Date(timeIntervalSince1970: 1_790_000_000)

    func testEveryPartCarriesTheMarkersAndDeliveryDropsTheNotebook() throws {
        let store = WatchRecordingStore(directory: dir)
        let transport = WatchTransferQueueTests.FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = UUID()
        let p0 = store.beginRecording(id: id, startedAt: start, kind: .class, notebook: "BIO 101", plannedMinutes: 60)
        FileManager.default.createFile(atPath: p0.path(percentEncoded: false), contents: Data([1]))
        let p1 = try XCTUnwrap(store.beginPart(id))
        FileManager.default.createFile(atPath: p1.path(percentEncoded: false), contents: Data([2]))
        let marks = [WatchMarker(kind: .important, at: start.addingTimeInterval(5), offset: 5),
                     WatchMarker(kind: .test, at: start.addingTimeInterval(40), offset: 15)]
        store.finish(WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(50), duration: 20,
                                            appVersion: "x", pauses: [.init(at: 10, length: 30)], kind: .class, notebook: "BIO 101",
                                            plannedMinutes: 60, markers: marks),
                     partLengths: [10, 10])
        XCTAssertEqual(queue.sendPending(), 2)
        let metas = transport.sent.compactMap { WatchRecordingMetadata(dictionary: $0.1) }
        XCTAssertEqual(metas.map(\.part), [0, 1])
        XCTAssertTrue(metas.allSatisfy { $0.markers == marks && $0.notebook == "BIO 101" && $0.kind == .class && $0.plannedMinutes == 60 },
                      "markers may arrive with any part")

        queue.confirmed([WatchTransfer.partKey(id, 0), WatchTransfer.partKey(id, 1)])
        let entry = try XCTUnwrap(store.entry(id))
        XCTAssertEqual(entry.status, .delivered)
        XCTAssertNil(entry.notebook, "the notebook name doesn't stay on the watch")
        XCTAssertEqual(entry.markers, [])
        XCTAssertNil(entry.metadata?.notebook)
        XCTAssertEqual(entry.metadata?.markers, [])
        let index = try String(contentsOf: dir.appending(path: "index.json"), encoding: .utf8)
        XCTAssertFalse(index.contains("BIO 101"))
    }

    func testACrashKeepsTypeNotebookPlanAndMarkers() throws {
        let id = UUID()
        do {
            let store = WatchRecordingStore(directory: dir)
            let p0 = store.beginRecording(id: id, startedAt: start, kind: .personal, notebook: "Health", plannedMinutes: 30)
            FileManager.default.createFile(atPath: p0.path(percentEncoded: false), contents: Data([1]))
            store.recordMarkers(id, [WatchMarker(kind: .test, at: start.addingTimeInterval(8), offset: 8)])
            store.recordPlan(id, plannedMinutes: 45)
        }
        // Relaunch after the app was ended mid-recording
        let store = WatchRecordingStore(directory: dir)
        let p0 = store.url(WatchRecordingStore.partName(id, 0))
        store.recoverInterrupted(appVersion: "x") { $0 == p0 ? 12 : nil }
        let meta = try XCTUnwrap(store.entry(id)?.metadata)
        XCTAssertEqual(meta.kind, .personal)
        XCTAssertEqual(meta.notebook, "Health")
        XCTAssertEqual(meta.plannedMinutes, 45)
        XCTAssertEqual(meta.markers.map(\.kind), [.test])
    }

    func testIndexFromBeforeTypesStillReads() throws {
        let id = UUID()
        let legacy = """
        [{"id":"\(id.uuidString)","startedAt":0,"parts":["\(id.uuidString)-p0.m4a"],"status":"saved","updatedAt":0,
          "metadata":{"recordingID":"\(id.uuidString)","startedAt":0,"endedAt":42,"duration":42,"appVersion":"1.0.0 (4)","pauses":[],"version":1}}]
        """
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(legacy.utf8).write(to: dir.appending(path: "index.json"))
        FileManager.default.createFile(atPath: dir.appending(path: "\(id.uuidString)-p0.m4a").path(percentEncoded: false), contents: Data([1]))
        let store = WatchRecordingStore(directory: dir)
        let entry = try XCTUnwrap(store.entry(id))
        XCTAssertEqual(entry.status, .saved)
        XCTAssertNil(entry.kind)
        XCTAssertEqual(entry.markers, [])
        XCTAssertNil(entry.metadata?.kind)
    }
}

/// Discreet: the setting, the start options and what the screen shows.
final class DiscreetModeTests: XCTestCase {
    private var suite = ""
    private var defaults: UserDefaults!

    override func setUp() {
        suite = "nf-watch-discreet-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suite)
    }

    override func tearDown() {
        defaults.removePersistentDomain(forName: suite)
    }

    func testDiscreetIsOffByDefaultAndRemembered() {
        XCTAssertFalse(DiscreetSetting.isOn(defaults))
        DiscreetSetting.set(true, defaults)
        XCTAssertTrue(DiscreetSetting.isOn(defaults))
        XCTAssertTrue(UserDefaults(suiteName: suite)!.bool(forKey: DiscreetSetting.key), "persisted")
        DiscreetSetting.set(false, defaults)
        XCTAssertFalse(DiscreetSetting.isOn(defaults))
    }

    func testStartOptionsRememberTypeLengthAndDiscreetButNotTheNotebook() {
        XCTAssertEqual(WatchStartOptions.remembered(defaults), WatchStartOptions(kind: .meeting, limit: .noLimit, discreet: false),
                       "never chosen: Meeting, no limit, not Discreet")
        WatchStartOptions(kind: .class, limit: .minutes(90), notebook: "BIO 101", discreet: true).remember(defaults)
        let again = WatchStartOptions.remembered(defaults)
        XCTAssertEqual(again.kind, .class)
        XCTAssertEqual(again.limit, .minutes(90))
        XCTAssertTrue(again.discreet)
        XCTAssertNil(again.notebook, "the App Intent starts with no notebook")
    }

    func testNormalDisplayShowsTheStandardScreen() {
        let p = RecordingPresentation.make(discreet: false, luminanceReduced: false, reduceMotion: false)
        XCTAssertTrue(p.showsStandardUI)
        XCTAssertNil(p.logo)
        XCTAssertFalse(p.tapAnywhereMarks)
        XCTAssertEqual(RecordingPresentation.make(discreet: false, luminanceReduced: true, reduceMotion: true, glancing: true), p)
    }

    func testDiscreetShowsOnlyAFaintPulsingLogo() throws {
        let p = RecordingPresentation.make(discreet: true, luminanceReduced: false, reduceMotion: false)
        XCTAssertFalse(p.showsStandardUI, "no red UI, no big timer")
        let logo = try XCTUnwrap(p.logo)
        XCTAssertTrue(logo.pulses)
        XCTAssertEqual(logo.low, 0.12)
        XCTAssertEqual(logo.high, 0.35)
        XCTAssertEqual(logo.cycle, 3.5)
        XCTAssertEqual(logo.opacity(at: 0), 0.12, accuracy: 0.0001)
        XCTAssertEqual(logo.opacity(at: logo.cycle / 2), 0.35, accuracy: 0.0001)
        for i in 0...100 {
            let o = logo.opacity(at: Double(i) * 0.137)
            XCTAssertTrue(o >= 0.12 - 1e-9 && o <= 0.35 + 1e-9, "never brighter than 0.35")
        }
        XCTAssertFalse(p.showsGlanceTime, "the time shows only after a tap or wrist raise")
        XCTAssertTrue(p.tapAnywhereMarks)
        let glance = RecordingPresentation.make(discreet: true, luminanceReduced: false, reduceMotion: false, glancing: true)
        XCTAssertTrue(glance.showsGlanceTime)
        XCTAssertEqual(glance.glanceOpacity, 0.45, "small, dim grey")
    }

    func testDiscreetWristDownIsOnlyADimmerStillLogo() throws {
        let p = RecordingPresentation.make(discreet: true, luminanceReduced: true, reduceMotion: false, glancing: true, markFlash: true)
        let logo = try XCTUnwrap(p.logo)
        XCTAssertFalse(logo.pulses)
        XCTAssertLessThan(logo.low, RecordingPresentation.pulseLow)
        XCTAssertEqual(logo.opacity(at: 1.7), logo.low)
        XCTAssertFalse(p.showsGlanceTime)
        XCTAssertFalse(p.showsStandardUI)
        XCTAssertFalse(p.tapAnywhereMarks)
    }

    func testDiscreetWithReduceMotionIsAStaticFadedLogo() throws {
        let p = RecordingPresentation.make(discreet: true, luminanceReduced: false, reduceMotion: true)
        let logo = try XCTUnwrap(p.logo)
        XCTAssertFalse(logo.pulses)
        XCTAssertEqual(logo.opacity(at: 0), logo.opacity(at: 1.75))
        XCTAssertTrue(logo.low >= 0.12 && logo.low <= 0.35)
        XCTAssertTrue(p.tapAnywhereMarks, "marking still works")
    }

    func testAMarkBrightensTheLogoOnlyFaintly() throws {
        for reduceMotion in [false, true] {
            let p = RecordingPresentation.make(discreet: true, luminanceReduced: false, reduceMotion: reduceMotion, markFlash: true)
            let logo = try XCTUnwrap(p.logo)
            XCTAssertEqual(logo.low, RecordingPresentation.markFlashOpacity)
            XCTAssertLessThanOrEqual(logo.low, 0.5, "a brighten, never a bright flash")
        }
    }

    func testDiscreetWhilePausedShowsPausedAndDoesNotMark() throws {
        let p = RecordingPresentation.make(discreet: true, luminanceReduced: false, reduceMotion: false, paused: true)
        XCTAssertFalse(try XCTUnwrap(p.logo).pulses)
        XCTAssertTrue(p.showsGlanceTime)
        XCTAssertFalse(p.tapAnywhereMarks)
    }

    func testDiscreetHapticsAreLightTaps() {
        let cues: [HapticCue] = [.start, .mark, .warning, .stop, .interrupted, .failure, .pauseResume]
        for cue in cues {
            XCTAssertTrue(cue.pattern(discreet: true).allSatisfy { $0 == .click }, "\(cue)")
            XCTAssertFalse(cue.pattern(discreet: true).isEmpty)
        }
        XCTAssertEqual(HapticCue.warning.pattern(discreet: true).count, 2, "a gentle pattern for the warning")
        XCTAssertEqual(HapticCue.start.pattern(discreet: false), [.start])
        XCTAssertEqual(HapticCue.mark.pattern(discreet: false), [.success])
        XCTAssertEqual(HapticCue.warning.pattern(discreet: false), [.notification])
        XCTAssertEqual(HapticCue.stop.pattern(discreet: false), [.stop])
    }
}
