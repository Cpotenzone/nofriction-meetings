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

/// Watch-side list + transfer queue: deliver → delete, fail → keep and retry.
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
        var outstandingRecordingIDs: Set<UUID> = []
        var sent: [(URL, [String: Any])] = []
        func transferFile(_ url: URL, metadata: [String: Any]) {
            sent.append((url, metadata))
            if let id = (metadata[WatchTransfer.Key.recordingID] as? String).flatMap(UUID.init(uuidString:)) {
                outstandingRecordingIDs.insert(id)
            }
        }
    }

    private func finishedRecording(_ store: WatchRecordingStore, seconds: TimeInterval = 42) -> UUID {
        let id = UUID()
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        let url = store.beginRecording(id: id, startedAt: start)
        FileManager.default.createFile(atPath: url.path(percentEncoded: false), contents: Data(repeating: 1, count: 128))
        store.finish(WatchRecordingMetadata(recordingID: id, startedAt: start, endedAt: start.addingTimeInterval(seconds),
                                            duration: seconds, appVersion: "test"))
        return id
    }

    func testSendsOnceAndDeletesOnDelivery() throws {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store)

        XCTAssertEqual(queue.sendPending(), 1)
        XCTAssertEqual(store.entry(id)?.status, .sending)
        XCTAssertEqual(queue.sendPending(), 0, "already outstanding: not queued twice")
        let meta = try XCTUnwrap(WatchRecordingMetadata(dictionary: transport.sent[0].1))
        XCTAssertEqual(meta.recordingID, id)
        XCTAssertEqual(meta.duration, 42)

        let file = store.fileURL(for: id)
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path(percentEncoded: false)))
        queue.didFinish(recordingID: id, errorMessage: nil)
        XCTAssertEqual(store.entry(id)?.status, .delivered)
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.path(percentEncoded: false)), "watch copy deleted once delivered")
    }

    func testFailureKeepsFileAndRetries() {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store)
        queue.sendPending()
        transport.outstandingRecordingIDs = []          // the system gave up on it
        queue.didFinish(recordingID: id, errorMessage: "The companion is not reachable.")
        XCTAssertEqual(store.entry(id)?.status, .failed)
        XCTAssertTrue(FileManager.default.fileExists(atPath: store.fileURL(for: id).path(percentEncoded: false)))
        XCTAssertEqual(queue.sendPending(), 1, "retried")
        XCTAssertEqual(transport.sent.count, 2)
    }

    func testNothingSentWhileUnavailable() {
        let store = WatchRecordingStore(directory: dir)
        let transport = FakeTransport()
        transport.canTransfer = false
        let queue = WatchTransferQueue(store: store, transport: transport)
        let id = finishedRecording(store)
        XCTAssertEqual(queue.sendPending(), 0)
        XCTAssertEqual(store.entry(id)?.status, .saved, "kept on the watch until the iPhone is available")
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

    func testRecoverInterruptedRecording() {
        let store = WatchRecordingStore(directory: dir)
        let keep = UUID(), lose = UUID()
        let url = store.beginRecording(id: keep, startedAt: .now)
        FileManager.default.createFile(atPath: url.path(percentEncoded: false), contents: Data([1]))
        store.beginRecording(id: lose, startedAt: .now)
        let result = store.recoverInterrupted(appVersion: "x") { $0 == url ? 12 : nil }
        XCTAssertEqual(result.kept, 1)
        XCTAssertEqual(result.lost, 1)
        XCTAssertEqual(store.entry(keep)?.status, .saved)
        XCTAssertEqual(store.entry(keep)?.duration, 12)
        XCTAssertNil(store.entry(lose))
    }

    func testPruneKeepsRecentDeliveredRowsAndRemovesStrayFiles() {
        let store = WatchRecordingStore(directory: dir)
        var now = Date(timeIntervalSince1970: 1_790_000_000)
        store.clock = { now }
        let transport = FakeTransport()
        let queue = WatchTransferQueue(store: store, transport: transport)
        let old = finishedRecording(store)
        queue.didFinish(recordingID: old, errorMessage: nil)
        now = now.addingTimeInterval(8 * 86_400)
        let stray = dir.appending(path: "\(UUID().uuidString).m4a")
        FileManager.default.createFile(atPath: stray.path(percentEncoded: false), contents: Data([1]))
        let fresh = finishedRecording(store)
        store.prune()
        XCTAssertNil(store.entry(old), "delivered rows older than a week are dropped")
        XCTAssertNotNil(store.entry(fresh))
        XCTAssertFalse(FileManager.default.fileExists(atPath: stray.path(percentEncoded: false)))
        XCTAssertTrue(FileManager.default.fileExists(atPath: store.fileURL(for: fresh).path(percentEncoded: false)))
    }

    func testMetadataRoundTripThroughPropertyList() throws {
        let meta = WatchRecordingMetadata(recordingID: UUID(), startedAt: Date(timeIntervalSince1970: 1_790_000_000),
                                          endedAt: Date(timeIntervalSince1970: 1_790_003_600), duration: 3500,
                                          appVersion: "1.0.0 (4)", pauses: [.init(at: 600, length: 100)])
        // WatchConnectivity requires property-list types
        let data = try PropertyListSerialization.data(fromPropertyList: meta.dictionary, format: .binary, options: 0)
        let back = try XCTUnwrap(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
        XCTAssertEqual(WatchRecordingMetadata(dictionary: back), meta)
    }
}
