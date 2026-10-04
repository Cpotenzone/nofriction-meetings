import XCTest
@testable import NoFrictionWatch

/// Recorder state machine: transitions, pause bookkeeping, metadata.
final class RecorderStateMachineTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_790_000_000)
    private func at(_ s: TimeInterval) -> Date { t0.addingTimeInterval(s) }

    func testStartPauseResumeStop() throws {
        var m = RecorderStateMachine()
        let id = UUID()
        XCTAssertEqual(m.phase, .idle)
        try m.start(id: id, at: at(0))
        XCTAssertEqual(m.phase, .recording)
        XCTAssertEqual(m.elapsed(at: at(30)), 30, accuracy: 0.001)

        try m.pause(at: at(30))
        XCTAssertEqual(m.phase, .paused(.user))
        XCTAssertEqual(m.elapsed(at: at(50)), 30, accuracy: 0.001, "paused time doesn't count")

        try m.resume(at: at(50))
        XCTAssertEqual(m.phase, .recording)
        XCTAssertEqual(m.elapsed(at: at(70)), 50, accuracy: 0.001)

        let meta = try m.stop(at: at(100), appVersion: "1.0.0 (4)")
        XCTAssertEqual(m.phase, .finished)
        XCTAssertEqual(meta.recordingID, id)
        XCTAssertEqual(meta.startedAt, at(0))
        XCTAssertEqual(meta.endedAt, at(100))
        XCTAssertEqual(meta.duration, 80, accuracy: 0.001)
        XCTAssertEqual(meta.pauses, [.init(at: 30, length: 20)])
        // File second 40 happened 20 s of pause later on the wall clock
        XCTAssertEqual(meta.wallClock(atFileOffset: 40), at(60))
        XCTAssertEqual(meta.wallClock(atFileOffset: 10), at(10))
    }

    func testInvalidTransitionsAreRejected() throws {
        var m = RecorderStateMachine()
        XCTAssertThrowsError(try m.pause(at: at(0)))
        XCTAssertThrowsError(try m.resume(at: at(0)))
        XCTAssertThrowsError(try m.stop(at: at(0), appVersion: "x"))
        try m.start(id: UUID(), at: at(0))
        XCTAssertThrowsError(try m.start(id: UUID(), at: at(1)), "already recording")
        XCTAssertThrowsError(try m.resume(at: at(1)), "not paused")
        try m.pause(at: at(2))
        XCTAssertThrowsError(try m.pause(at: at(3)), "a user pause while paused")
    }

    func testInterruptionPausesAndNeedsUserResume() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        try m.pause(at: at(60), reason: .interruption)
        XCTAssertEqual(m.phase, .paused(.interruption))
        XCTAssertTrue(m.isActive)
        try m.resume(at: at(90))
        XCTAssertEqual(m.elapsed(at: at(100)), 70, accuracy: 0.001)
    }

    func testInterruptionWhileUserPausedKeepsOnePause() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        try m.pause(at: at(10))
        try m.pause(at: at(15), reason: .interruption)
        XCTAssertEqual(m.phase, .paused(.interruption))
        try m.resume(at: at(20))
        let meta = try m.stop(at: at(30), appVersion: "x")
        XCTAssertEqual(meta.pauses, [.init(at: 10, length: 10)])
        XCTAssertEqual(meta.duration, 20, accuracy: 0.001)
    }

    func testStopWhilePausedDropsTrailingPause() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        try m.pause(at: at(40))
        let meta = try m.stop(at: at(100), appVersion: "x")
        XCTAssertEqual(meta.pauses, [], "no audio follows a pause that ends the recording")
        XCTAssertEqual(meta.duration, 40, accuracy: 0.001)
        XCTAssertEqual(meta.endedAt, at(100))
    }

    func testRecorderDurationWinsWhenGiven() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        let meta = try m.stop(at: at(60), appVersion: "x", audioDuration: 59.4)
        XCTAssertEqual(meta.duration, 59.4, accuracy: 0.001)
    }

    func testStartAgainAfterFinish() throws {
        var m = RecorderStateMachine()
        try m.start(id: UUID(), at: at(0))
        try m.stop(at: at(5), appVersion: "x")
        let second = UUID()
        try m.start(id: second, at: at(10))
        XCTAssertEqual(m.recordingID, second)
        XCTAssertEqual(m.pauses, [])
        XCTAssertEqual(m.elapsed(at: at(12)), 2, accuracy: 0.001)
    }
}

/// Dropping empty or unreadable parts keeps file time and clock time aligned.
final class PartLayoutTests: XCTestCase {
    typealias Pause = WatchRecordingMetadata.Pause

    func testAllPartsKeptPausesAtBoundaries() {
        let r = PartLayout.normalize(lengths: [10, 20, 5], pauses: [Pause(at: 9.9, length: 30), Pause(at: 30, length: 60)])
        XCTAssertEqual(r.keep, [0, 1, 2])
        XCTAssertEqual(r.pauses, [Pause(at: 10, length: 30), Pause(at: 30, length: 60)], "moved to the real part ends")
        XCTAssertEqual(r.duration, 35)
        XCTAssertEqual(r.startShift, 0)
    }

    func testEmptyMiddlePartMergesItsPauses() {
        // Resume then straight back to Pause: part 1 has no audio
        let r = PartLayout.normalize(lengths: [10, 0.01, 5], pauses: [Pause(at: 10, length: 30), Pause(at: 10, length: 60)])
        XCTAssertEqual(r.keep, [0, 2])
        XCTAssertEqual(r.pauses.count, 1)
        XCTAssertEqual(r.pauses[0].at, 10)
        XCTAssertEqual(r.pauses[0].length, 90.01, accuracy: 0.0001)
    }

    func testUnreadableFirstPartShiftsTheStart() {
        let r = PartLayout.normalize(lengths: [nil, 8], pauses: [Pause(at: 3, length: 20)])
        XCTAssertEqual(r.keep, [1])
        XCTAssertEqual(r.pauses, [])
        XCTAssertEqual(r.startShift, 20, "the first kept audio began after the pause")
        XCTAssertEqual(r.duration, 8)
    }

    func testNothingUsable() {
        XCTAssertEqual(PartLayout.normalize(lengths: [nil, 0], pauses: []).keep, [])
    }

    func testTrailingPauseWithoutAudioIsIgnored() {
        // Killed while paused: the open pause has no audio after it
        let r = PartLayout.normalize(lengths: [12], pauses: [Pause(at: 12, length: 0)])
        XCTAssertEqual(r.keep, [0])
        XCTAssertEqual(r.pauses, [])
    }
}

/// Watch-side list + transfer queue: the file goes only when the iPhone
/// confirms it stored it; failures keep it and retry.
@MainActor
final class WatchTransferQueueTests: XCTestCase {
    private var dir: URL!

    override func setUp() async throws {
        dir = URL.temporaryDirectory.appending(path: "nf-watch-tests-\(UUID().uuidString)", directoryHint: .isDirectory)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
    }

    final class FakeTransport: RecordingTransport {
        var canTransfer = true
        var outstandingTransfers: Set<String> = []
        var sent: [(URL, [String: Any])] = []
        func transferFile(_ url: URL, metadata: [String: Any]) {
            sent.append((url, metadata))
            if let meta = WatchRecordingMetadata(dictionary: metadata) {
                outstandingTransfers.insert(WatchTransfer.partKey(meta.recordingID, meta.part))
            }
        }
        /// The system finished a transfer: no longer outstanding
        func finish(_ id: UUID, _ part: Int) { outstandingTransfers.remove(WatchTransfer.partKey(id, part)) }
    }

    private func exists(_ url: URL) -> Bool { FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) }

    /// A finished recording with `parts` files (one per pause, plus one), each 10 s.
    private func finishedRecording(_ store: WatchRecordingStore, parts: Int = 1) -> UUID {
        let id = UUID()
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        var url = store.beginRecording(id: id, startedAt: start)
        FileManager.default.createFile(atPath: url.path(percentEncoded: false), contents: Data(repeating: 1, count: 128))
        for _ in 1..<max(1, parts) {
            url = store.beginPart(id)!
            FileManager.default.createFile(atPath: url.path(percentEncoded: false), contents: Data(repeating: 2, count: 128))
        }
        let pauses = (1..<max(1, parts)).map { WatchRecordingMetadata.Pause(at: Double($0) * 10, length: 5) }
        let seconds = Double(max(1, parts)) * 10
        store.finish(WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(seconds + Double(pauses.count) * 5),
                                            duration: seconds, appVersion: "test", pauses: pauses),
                     partLengths: Array(repeating: 10, count: max(1, parts)))
        return id
    }

    func testFileStaysUntilTheIPhoneConfirms() throws {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store)

        XCTAssertEqual(queue.sendPending(), 1)
        XCTAssertEqual(store.entry(id)?.status, .sending)
        XCTAssertEqual(queue.sendPending(), 0, "already outstanding: not queued twice")
        let meta = try XCTUnwrap(WatchRecordingMetadata(dictionary: transport.sent[0].1))
        XCTAssertEqual(meta.recordingID, id)
        XCTAssertEqual(meta.duration, 10)
        XCTAssertEqual(meta.part, 0)
        XCTAssertEqual(meta.partCount, 1)

        let file = try XCTUnwrap(store.entry(id).map(store.partURLs)?.first)
        transport.finish(id, 0)
        queue.didFinish(recordingID: id, part: 0, errorMessage: nil)
        XCTAssertTrue(exists(file), "the system has it, the iPhone app hasn't confirmed: keep it")
        XCTAssertEqual(store.entry(id)?.status, .sending)
        XCTAssertEqual(queue.sendPending(), 0, "handed off recently: wait for the confirmation")

        queue.confirmed([WatchTransfer.partKey(id, 0)])
        XCTAssertEqual(store.entry(id)?.status, .delivered)
        XCTAssertFalse(exists(file), "deleted once the iPhone confirmed")
    }

    func testUnconfirmedHandOffIsSentAgainLater() {
        let store = WatchRecordingStore(directory: dir)
        var now = Date(timeIntervalSince1970: 1_790_000_000)
        store.clock = { now }
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store)
        queue.sendPending()
        transport.finish(id, 0)
        queue.didFinish(recordingID: id, part: 0, errorMessage: nil)
        now = now.addingTimeInterval(WatchRecordingStore.resendAfter + 1)
        XCTAssertEqual(queue.sendPending(), 1, "no confirmation came: send again (the iPhone ignores duplicates)")
    }

    func testPausedRecordingSendsEveryPartAndDeletesEachOnConfirmation() throws {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store, parts: 3)
        let files = try XCTUnwrap(store.entry(id).map(store.partURLs))
        XCTAssertEqual(files.count, 3)

        XCTAssertEqual(queue.sendPending(), 3)
        let metas = transport.sent.compactMap { WatchRecordingMetadata(dictionary: $0.1) }
        XCTAssertEqual(metas.map(\.part), [0, 1, 2])
        XCTAssertTrue(metas.allSatisfy { $0.partCount == 3 && $0.recordingID == id && $0.duration == 30 })
        XCTAssertEqual(metas[0].pauses, [.init(at: 10, length: 5), .init(at: 20, length: 5)], "every part carries the whole recording's pauses")
        XCTAssertEqual(transport.sent.map(\.0), files)

        queue.confirmed([WatchTransfer.partKey(id, 1)])
        XCTAssertFalse(exists(files[1]))
        XCTAssertTrue(exists(files[0]) && exists(files[2]))
        XCTAssertEqual(store.entry(id)?.status, .sending, "not delivered until every part is")
        XCTAssertFalse(store.entry(id)!.canDelete, "the iPhone holds part of it")
        queue.confirmed([WatchTransfer.partKey(id, 0), WatchTransfer.partKey(id, 2)])
        XCTAssertEqual(store.entry(id)?.status, .delivered)
        XCTAssertFalse(files.contains(where: exists))
    }

    func testFailureKeepsFileAndRetriesOnlyThatPart() {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store, parts: 2)
        queue.sendPending()
        queue.confirmed([WatchTransfer.partKey(id, 0)])
        transport.finish(id, 1)
        queue.didFinish(recordingID: id, part: 1, errorMessage: "The companion is not reachable.")
        XCTAssertEqual(store.entry(id)?.status, .failed)
        XCTAssertEqual(store.entry(id)?.lastError, "The companion is not reachable.")
        XCTAssertTrue(exists(store.url(store.entry(id)!.parts[1])), "kept on failure")
        XCTAssertEqual(queue.sendPending(), 1, "only the failed part is sent again")
        XCTAssertEqual(WatchRecordingMetadata(dictionary: transport.sent.last!.1)?.part, 1)
    }

    func testFailureOfADuplicateDoesNotUndoADelivery() {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store)
        XCTAssertEqual(queue.sendPending(), 1)
        queue.confirmed([WatchTransfer.partKey(id, 0)])
        queue.didFinish(recordingID: id, part: 0, errorMessage: "duplicate failed")
        XCTAssertEqual(store.entry(id)?.status, .delivered)
    }

    func testNothingSentWhileUnavailable() {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        transport.canTransfer = false
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store)
        XCTAssertEqual(queue.sendPending(), 0)
        XCTAssertEqual(store.entry(id)?.status, .saved, "kept on the watch until the iPhone is available")
        XCTAssertTrue(store.entry(id)!.canDelete)
    }

    func testRecordingInProgressIsNotSent() {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        let url = store.beginRecording(id: UUID(), startedAt: .now)
        FileManager.default.createFile(atPath: url.path(percentEncoded: false), contents: Data([1]))
        XCTAssertEqual(WatchTransferQueue(store: store, transport: transport).sendPending(), 0)
    }

    func testSendingRowLostAcrossRelaunchIsResent() {
        let id: UUID
        do {
            let store = WatchRecordingStore(directory: dir)
            id = finishedRecording(store)
            store.markSending(id)
        }
        // Relaunch: WatchConnectivity no longer lists it as outstanding
        let store = WatchRecordingStore(directory: dir)
        XCTAssertEqual(store.entry(id)?.status, .sending, "index persisted")
        let transport = FakeTransport()
        XCTAssertEqual(WatchTransferQueue(store: store, transport: transport).sendPending(), 1)
    }

    func testStopDropsEmptyPartsAndDiscardsAnEmptyRecording() throws {
        let store = WatchRecordingStore(directory: dir)
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        let id = UUID()
        let p0 = store.beginRecording(id: id, startedAt: start)
        let p1 = try XCTUnwrap(store.beginPart(id))
        for u in [p0, p1] { FileManager.default.createFile(atPath: u.path(percentEncoded: false), contents: Data([1])) }
        XCTAssertTrue(store.finish(WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(40),
                                                          duration: 10, appVersion: "x", pauses: [.init(at: 10, length: 30)]),
                                   partLengths: [10, nil]))
        XCTAssertEqual(store.entry(id)?.parts.count, 1)
        XCTAssertEqual(store.entry(id)?.metadata?.partCount, 1)
        XCTAssertEqual(store.entry(id)?.metadata?.pauses, [])
        XCTAssertFalse(exists(p1))

        let empty = UUID()
        let e0 = store.beginRecording(id: empty, startedAt: start)
        FileManager.default.createFile(atPath: e0.path(percentEncoded: false), contents: Data([1]))
        XCTAssertFalse(store.finish(WatchRecordingMetadata(recordingID: empty, startedAt: start, endedAt: start,
                                                           duration: 0, appVersion: "x"), partLengths: [0]))
        XCTAssertNil(store.entry(empty))
        XCTAssertFalse(exists(e0))
    }

    func testRecoverKeepsReadablePartsAfterACrash() throws {
        let store = WatchRecordingStore(directory: dir)
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        let keep = UUID(), lose = UUID()
        // Paused once (part 0 closed), then killed while recording part 1
        let p0 = store.beginRecording(id: keep, startedAt: start)
        FileManager.default.createFile(atPath: p0.path(percentEncoded: false), contents: Data([1]))
        store.recordPauses(keep, [.init(at: 12, length: 30)])
        let p1 = try XCTUnwrap(store.beginPart(keep))
        FileManager.default.createFile(atPath: p1.path(percentEncoded: false), contents: Data([2]))
        // Killed before anything was closed
        store.beginRecording(id: lose, startedAt: start)

        let result = store.recoverInterrupted(appVersion: "x") { $0 == p0 ? 12 : nil }
        XCTAssertEqual(result.kept, 1)
        XCTAssertEqual(result.lost, 1)
        let entry = try XCTUnwrap(store.entry(keep))
        XCTAssertEqual(entry.status, .saved)
        XCTAssertEqual(entry.parts.count, 1, "the unreadable part is dropped")
        XCTAssertEqual(entry.duration, 12)
        XCTAssertEqual(entry.metadata?.pauses, [], "no audio after that pause survived")
        XCTAssertFalse(exists(p1))
        XCTAssertNil(store.entry(lose))
    }

    func testUnreadableIndexNeverDeletesAudio() throws {
        let id: UUID
        let files: [URL]
        do {
            let store = WatchRecordingStore(directory: dir)
            id = finishedRecording(store, parts: 2)
            files = store.entry(id).map(store.partURLs) ?? []
        }
        try Data("{ not json".utf8).write(to: dir.appending(path: "index.json"))
        let store = WatchRecordingStore(directory: dir)
        XCTAssertTrue(files.allSatisfy(exists), "audio kept")
        let entry = try XCTUnwrap(store.entry(id), "rebuilt from the files")
        XCTAssertEqual(entry.parts, files.map(\.lastPathComponent))
        XCTAssertEqual(entry.status, .recording, "launch recovery finishes it")
        store.prune()
        XCTAssertTrue(files.allSatisfy(exists))
        let names = try FileManager.default.contentsOfDirectory(atPath: dir.path(percentEncoded: false))
        XCTAssertTrue(names.contains { $0.hasPrefix("index-unreadable-") }, "the bad index is kept aside")
    }

    func testReadsAnIndexFromTheFirstBuilds() throws {
        let id = UUID()
        let legacy = """
        [{"id":"\(id.uuidString)","startedAt":0,"fileName":"\(id.uuidString).m4a","status":"saved","updatedAt":0,
          "metadata":{"recordingID":"\(id.uuidString)","startedAt":0,"endedAt":42,"duration":42,"appVersion":"1.0.0 (3)","pauses":[],"version":1}}]
        """
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(legacy.utf8).write(to: dir.appending(path: "index.json"))
        FileManager.default.createFile(atPath: dir.appending(path: "\(id.uuidString).m4a").path(percentEncoded: false), contents: Data([1]))
        let store = WatchRecordingStore(directory: dir)
        let entry = try XCTUnwrap(store.entry(id))
        XCTAssertEqual(entry.parts, ["\(id.uuidString).m4a"])
        XCTAssertEqual(entry.status, .saved)
        let transport = FakeTransport()   // the queue holds its transport weakly
        XCTAssertEqual(WatchTransferQueue(store: store, transport: transport).sendPending(), 1, "still sent")
    }

    func testPruneKeepsRecentDeliveredRowsOnly() {
        let store = WatchRecordingStore(directory: dir)
        var now = Date(timeIntervalSince1970: 1_790_000_000)
        store.clock = { now }
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let old = finishedRecording(store)
        queue.confirmed([WatchTransfer.partKey(old, 0)])
        now = now.addingTimeInterval(8 * 86_400)
        let fresh = finishedRecording(store, parts: 2)
        store.prune()
        XCTAssertNil(store.entry(old), "delivered rows older than a week are dropped")
        XCTAssertNotNil(store.entry(fresh))
        XCTAssertTrue(store.entry(fresh).map(store.partURLs)?.allSatisfy(exists) ?? false)
    }

    func testMetadataRoundTripThroughPropertyList() throws {
        let meta = WatchRecordingMetadata(recordingID: UUID(), startedAt: Date(timeIntervalSince1970: 1_790_000_000),
                                          endedAt: Date(timeIntervalSince1970: 1_790_003_600), duration: 3500,
                                          appVersion: "1.0.0 (4)", pauses: [.init(at: 600, length: 100)], part: 1, partCount: 2)
        // WatchConnectivity requires property-list types
        let data = try PropertyListSerialization.data(fromPropertyList: meta.dictionary, format: .binary, options: 0)
        let back = try XCTUnwrap(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
        XCTAssertEqual(WatchRecordingMetadata(dictionary: back), meta)
    }
}
