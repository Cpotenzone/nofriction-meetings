import AVFoundation
import SwiftData
import XCTest
@testable import noFriction

// Screen capture on iPhone and iPad (docs/SCREEN_CAPTURE_IOS.md). ReplayKit
// broadcasts don't run in the Simulator, so the extension's decisions are
// tested as pure functions over synthetic brightness grids, and the import
// over folders laid out exactly as the extension writes them.

// MARK: - Change detector, throttle, hidden video

final class FrameAnalysisTests: XCTestCase {
    /// A 10×10 grid, every cell `value`
    private func flat(_ value: UInt8, columns: Int = 10, rows: Int = 10) -> LumaGrid {
        LumaGrid(columns: columns, rows: rows, cells: Array(repeating: value, count: columns * rows))
    }

    /// `flat(base)` with the first `count` cells set to `value`
    private func with(_ base: UInt8, _ count: Int, at value: UInt8, columns: Int = 10, rows: Int = 10) -> LumaGrid {
        var g = flat(base, columns: columns, rows: rows)
        for i in 0..<count { g.cells[i] = value }
        return g
    }

    func testGridSizeFollowsTheFramesShape() {
        XCTAssertEqual(LumaGrid.size(width: 1179, height: 2556).columns, 30)
        XCTAssertEqual(LumaGrid.size(width: 1179, height: 2556).rows, 64)
        XCTAssertEqual(LumaGrid.size(width: 2556, height: 1179).columns, 64)
        XCTAssertEqual(LumaGrid.size(width: 2556, height: 1179).rows, 30)
        XCTAssertEqual(LumaGrid.size(width: 8, height: 4).columns, 8, "never more cells than pixels")
        XCTAssertEqual(LumaGrid.size(width: 0, height: 10).columns, 0)
    }

    func testGridAveragesEachBlockOfTheLumaPlane() {
        // 8×4 plane, row padding 4 bytes: left half 200, right half 40
        let width = 8, height = 4, bytesPerRow = 12
        var plane = [UInt8](repeating: 99, count: bytesPerRow * height)
        for y in 0..<height { for x in 0..<width { plane[y * bytesPerRow + x] = x < 4 ? 200 : 40 } }
        let grid = plane.withUnsafeBufferPointer {
            LumaGrid.fromLuma($0.baseAddress!, width: width, height: height, bytesPerRow: bytesPerRow, longSide: 2, step: 1)
        }
        XCTAssertEqual(grid.columns, 2)
        XCTAssertEqual(grid.rows, 1)
        XCTAssertEqual(grid.cells, [200, 40], "padding bytes are never read")
    }

    func testGridFromBGRAUsesLuma() {
        // 2×1 BGRA: pure white, pure black
        let pixels: [UInt8] = [255, 255, 255, 255, 0, 0, 0, 255]
        let grid = pixels.withUnsafeBufferPointer {
            LumaGrid.fromBGRA($0.baseAddress!, width: 2, height: 1, bytesPerRow: 8, longSide: 2, step: 1)
        }
        XCTAssertEqual(grid.cells.count, 2)
        XCTAssertGreaterThanOrEqual(grid.cells[0], 250)
        XCTAssertEqual(grid.cells[1], 0)
    }

    func testFirstFrameIsKeptAndAnIdenticalOneIsNot() {
        var d = ScreenChangeDetector()
        XCTAssertEqual(d.consider(flat(120), at: 0), .keep)
        XCTAssertEqual(d.consider(flat(120), at: 10), .unchanged)
        XCTAssertEqual(d.consider(with(120, 0, at: 0), at: 20), .unchanged)
    }

    func testABlinkingCursorIsNotAChange() {
        var d = ScreenChangeDetector(config: .init(smallChange: 0.02))
        XCTAssertEqual(d.consider(flat(120), at: 0), .keep)
        // 1 of 100 cells moved: under the 2% "something changed" share
        XCTAssertEqual(d.consider(with(120, 1, at: 255), at: 30), .unchanged)
        // Small brightness drift in every cell isn't a change either
        XCTAssertEqual(d.consider(flat(125), at: 30), .unchanged)
    }

    func testANewScreenWaitsOneSecondThenIsKept() {
        var d = ScreenChangeDetector()
        XCTAssertEqual(d.consider(flat(120), at: 0), .keep)
        let slide = with(120, 60, at: 250)                       // 60% of the screen changed
        XCTAssertEqual(d.consider(slide, at: 0.25), .tooSoon)
        XCTAssertEqual(d.consider(slide, at: 0.75), .tooSoon)
        XCTAssertEqual(d.consider(slide, at: 1.0), .keep, "kept once a second has passed")
        XCTAssertEqual(d.consider(slide, at: 1.25), .unchanged, "compared with the screen kept, not the previous frame")
    }

    func testASmallChangeWaitsForTheCalmInterval() {
        var d = ScreenChangeDetector()
        XCTAssertEqual(d.consider(flat(120), at: 0), .keep)
        let line = with(120, 5, at: 250)                          // 5%: a line of text
        XCTAssertEqual(d.consider(line, at: 1.5), .tooSoon)
        XCTAssertEqual(d.consider(line, at: 4.9), .tooSoon)
        XCTAssertEqual(d.consider(line, at: 5.0), .keep)
    }

    func testNeverMoreThanOneScreenPerSecond() {
        // Video: every frame differs completely, 30 frames a second for 10 s
        var d = ScreenChangeDetector()
        var kept: [Double] = []
        for i in 0..<300 {
            let t = Double(i) / 30
            if d.consider(flat(i % 2 == 0 ? 40 : 220), at: t) == .keep { kept.append(t) }
        }
        XCTAssertEqual(kept.count, 10)
        for (a, b) in zip(kept, kept.dropFirst()) { XCTAssertGreaterThanOrEqual(b - a, 1 - 1e-9) }
    }

    func testRotationCountsAsANewScreen() {
        var d = ScreenChangeDetector()
        XCTAssertEqual(d.consider(flat(120, columns: 10, rows: 20), at: 0), .keep)
        XCTAssertEqual(d.consider(flat(120, columns: 20, rows: 10), at: 0.5), .tooSoon)
        XCTAssertEqual(d.consider(flat(120, columns: 20, rows: 10), at: 1.0), .keep)
    }

    func testNearBlackDetector() {
        XCTAssertTrue(HiddenFrame.isNearBlack(flat(0)), "full-range black")
        XCTAssertTrue(HiddenFrame.isNearBlack(flat(16)), "video-range black")
        XCTAssertTrue(HiddenFrame.isNearBlack(with(16, 1, at: 235)), "a subtitle on black still counts as hidden")
        XCTAssertFalse(HiddenFrame.isNearBlack(with(16, 10, at: 200)), "dark mode with text is a screen")
        XCTAssertFalse(HiddenFrame.isNearBlack(flat(30)), "a dark scene isn't black")
        XCTAssertFalse(HiddenFrame.isNearBlack(LumaGrid(columns: 0, rows: 0, cells: [])))
    }

    func testHiddenFramesAreNeverKeptAndDontReplaceTheLastScreen() {
        var d = ScreenChangeDetector()
        XCTAssertEqual(d.consider(flat(120), at: 0), .keep)
        XCTAssertEqual(d.consider(flat(16), at: 2), .hidden)
        XCTAssertEqual(d.lastKept, flat(120))
        XCTAssertEqual(d.consider(flat(120), at: 4), .unchanged, "back to the same screen after the black")
    }

    func testHiddenTimeAddsUpAndGapsDontCount() {
        var t = HiddenVideoTracker()
        for i in 0...8 { t.note(hidden: true, at: Double(i) * 0.25) }      // 2 s hidden
        XCTAssertEqual(t.seconds, 2, accuracy: 1e-9)
        XCTAssertFalse(t.shouldNotify)
        t.note(hidden: false, at: 2.25)
        t.note(hidden: true, at: 10)
        t.note(hidden: true, at: 15)                                          // a 5 s gap: not counted
        XCTAssertEqual(t.seconds, 2, accuracy: 1e-9)
        t.note(hidden: true, at: 16.5)
        XCTAssertEqual(t.seconds, 3.5, accuracy: 1e-9)
        XCTAssertTrue(t.shouldNotify)
    }
}

// MARK: - Contract

final class ScreenCaptureContractTests: XCTestCase {
    func testFrameNamesCarryTheirTime() {
        let at = Date(timeIntervalSince1970: 1_790_000_000.123)
        let name = ScreenCaptureContract.frameName(at: at)
        XCTAssertEqual(name, "f-1790000000123.jpg")
        XCTAssertEqual(ScreenCaptureContract.frameDate(fromName: name)!.timeIntervalSince1970, 1_790_000_000.123, accuracy: 0.001)
        XCTAssertNil(ScreenCaptureContract.frameDate(fromName: "manifest.json"))
        XCTAssertNil(ScreenCaptureContract.frameDate(fromName: "a-1.m4a"))
        XCTAssertNil(ScreenCaptureContract.frameDate(fromName: "f-12x4.jpg"))
        XCTAssertNil(ScreenCaptureContract.frameDate(fromName: "f-.jpg"))
    }

    func testManifestRoundTripAndLiveness() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "nf-manifest-\(UUID().uuidString)", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        var m = ScreenCaptureManifest(startedAt: start, appAudio: true)
        m.parts = [.init(file: "a-1.m4a", start: start.addingTimeInterval(1), end: nil)]
        m.framesKept = 3
        m.hiddenSeconds = 4.5
        try m.write(to: dir)
        XCTAssertEqual(ScreenCaptureManifest.load(from: dir), m)
        XCTAssertTrue(m.isLive(at: start.addingTimeInterval(10)))
        XCTAssertFalse(m.isLive(at: start.addingTimeInterval(ScreenCaptureContract.staleAfter + 1)), "the extension stopped writing: over")
        m.endedAt = start.addingTimeInterval(5)
        XCTAssertFalse(m.isLive(at: start.addingTimeInterval(6)))
    }
}

// MARK: - Pro gate

final class ScreenAudioGateTests: XCTestCase {
    func testAppAudioOnlyForProWithTheSwitchOn() {
        XCTAssertTrue(ScreenAudioPolicy.allowsAppAudio(isPro: true, wantsIt: true))
        XCTAssertFalse(ScreenAudioPolicy.allowsAppAudio(isPro: true, wantsIt: false))
        XCTAssertFalse(ScreenAudioPolicy.allowsAppAudio(isPro: false, wantsIt: true))
        XCTAssertFalse(ScreenAudioPolicy.allowsAppAudio(isPro: false, wantsIt: false))
    }

    func testFreeUserSeesTheSwitchOffAndThePaywallOnTurningItOn() {
        XCTAssertFalse(ScreenAudioPolicy.switchShowsOn(isPro: false, wantsIt: true))
        XCTAssertTrue(ScreenAudioPolicy.switchShowsOn(isPro: true, wantsIt: true))
        XCTAssertTrue(ScreenAudioPolicy.needsPaywall(turningOn: true, isPro: false))
        XCTAssertFalse(ScreenAudioPolicy.needsPaywall(turningOn: true, isPro: true))
        XCTAssertFalse(ScreenAudioPolicy.needsPaywall(turningOn: false, isPro: false))
    }

    func testDefaultOnForProAndPublishedToTheExtension() throws {
        let suite = "nf-screen-gate-\(UUID().uuidString)"
        let d = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { d.removePersistentDomain(forName: suite) }
        XCTAssertTrue(ScreenCapturePrefs.wantsAppAudio(d), "on unless turned off")
        ScreenAudioPolicy.publish(isPro: false, wantsIt: true, to: d)
        XCTAssertFalse(d.bool(forKey: ScreenCaptureContract.Key.appAudio), "free: the extension never writes app audio")
        ScreenAudioPolicy.publish(isPro: true, wantsIt: ScreenCapturePrefs.wantsAppAudio(d), to: d)
        XCTAssertTrue(d.bool(forKey: ScreenCaptureContract.Key.appAudio))
        ScreenCapturePrefs.setWantsAppAudio(false, d)
        ScreenAudioPolicy.publish(isPro: true, wantsIt: ScreenCapturePrefs.wantsAppAudio(d), to: d)
        XCTAssertFalse(d.bool(forKey: ScreenCaptureContract.Key.appAudio))
    }

    func testPaywallFeatureKey() {
        XCTAssertEqual(ProFeature.transcribeWhatsPlaying.rawValue, "transcribe-whats-playing")
        XCTAssertEqual(ProFeature.transcribeWhatsPlaying.paywallLine, "Transcribe what's playing is part of noFriction Pro.")
    }

    func testRememberedNotebookIsNormalized() throws {
        let suite = "nf-screen-notebook-\(UUID().uuidString)"
        let d = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { d.removePersistentDomain(forName: suite) }
        ScreenCapturePrefs.rememberNotebook("  BIO   101 ", d)
        XCTAssertEqual(ScreenCapturePrefs.lastNotebook(d), "BIO 101")
        ScreenCapturePrefs.rememberNotebook(nil, d)
        XCTAssertNil(ScreenCapturePrefs.lastNotebook(d))
    }
}

// MARK: - Audio session

final class ScreenCaptureAudioSessionTests: XCTestCase {
    func testOtherAppsKeepPlayingAndAreNeverDucked() {
        for capturing in [false, true] {
            let o = AudioCapture.categoryOptions(keepsPlaybackQuality: capturing)
            XCTAssertTrue(o.contains(.mixWithOthers), "another app's video keeps playing")
            XCTAssertFalse(o.contains(.duckOthers), "and isn't made quieter")
            XCTAssertTrue(o.contains(.defaultToSpeaker), "its sound stays on the speaker, not the earpiece")
        }
    }

    func testCapturingKeepsBluetoothHeadphonesInHighQuality() {
        let o = AudioCapture.categoryOptions(keepsPlaybackQuality: true)
        XCTAssertTrue(o.contains(.allowBluetoothA2DP))
        XCTAssertFalse(o.contains(.allowBluetooth), "no hands-free profile while capturing the screen")
    }
}

// MARK: - Echo filter

final class ScreenTranscriptMergeTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_790_000_000)

    func testALineTheMicrophoneAlsoHeardIsAnEcho() {
        let mic = [(text: "We need to leave before the storm hits the harbor.", start: t0.addingTimeInterval(1), end: t0.addingTimeInterval(4))]
        XCTAssertTrue(ScreenTranscriptMerge.isEcho("we need to leave before the storm hits", start: t0.addingTimeInterval(2), end: t0.addingTimeInterval(5), microphone: mic))
        XCTAssertFalse(ScreenTranscriptMerge.isEcho("A completely different sentence about lunch", start: t0.addingTimeInterval(2), end: t0.addingTimeInterval(5), microphone: mic))
        XCTAssertFalse(ScreenTranscriptMerge.isEcho("we need to leave before the storm hits", start: t0.addingTimeInterval(60), end: t0.addingTimeInterval(63), microphone: mic),
                       "same words a minute later aren't an echo")
        XCTAssertFalse(ScreenTranscriptMerge.isEcho("the storm", start: t0.addingTimeInterval(2), end: t0.addingTimeInterval(3), microphone: mic),
                       "too short to judge")
    }
}

// MARK: - Import, transcription, purge

@MainActor
final class ScreenCaptureImportTests: XCTestCase {
    private var container: ModelContainer!
    private var context: ModelContext!
    private var root: URL!
    private var group: URL!
    private var snapshotsDir: URL!
    private var audioDir: URL!
    private var appAudioAllowed = true
    private var now = Date(timeIntervalSince1970: 1_790_000_100)
    private var linksSuite = ""
    private var links: ScreenCaptureLinks!
    private let start = Date(timeIntervalSince1970: 1_790_000_000)

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        context = container.mainContext
        root = FileManager.default.temporaryDirectory.appending(path: "nf-screen-\(UUID().uuidString)", directoryHint: .isDirectory)
        group = root.appending(path: "group/ScreenCapture", directoryHint: .isDirectory)
        snapshotsDir = root.appending(path: "Snapshots", directoryHint: .isDirectory)
        audioDir = root.appending(path: "Audio", directoryHint: .isDirectory)
        for d in [group!, snapshotsDir!, audioDir!] { try FileManager.default.createDirectory(at: d, withIntermediateDirectories: true) }
        appAudioAllowed = true
        now = start.addingTimeInterval(100)
        linksSuite = "nf-screen-links-\(UUID().uuidString)"
        links = ScreenCaptureLinks(defaults: UserDefaults(suiteName: linksSuite)!)
        Storage.prepare()
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: root)
        UserDefaults().removePersistentDomain(forName: linksSuite)
        container = nil
        context = nil
    }

    private func makeImporter(_ transcriber: FileTranscriber? = nil, snapshots: URL? = nil) -> ScreenCaptureImporter {
        let t = transcriber ?? ScriptedTranscriber { _, _ in [] }
        return ScreenCaptureImporter(context: context, env: .init(
            root: group, snapshotsDirectory: snapshots ?? snapshotsDir, audioDirectory: audioDir,
            makeTranscriber: { t }, isForeground: { true },
            appAudioAllowed: { [unowned self] in self.appAudioAllowed },
            speechLevel: 0, now: { [unowned self] in self.now }))
    }

    /// A broadcast folder as the extension leaves it: manifest, frames, audio parts.
    @discardableResult
    private func makeBroadcast(frames: [Double], audio: [(at: Double, seconds: Double, complete: Bool)] = [],
                               ended: Bool = true, appAudio: Bool = true) throws -> (URL, ScreenCaptureManifest) {
        var m = ScreenCaptureManifest(startedAt: start, appAudio: appAudio)
        let folder = group.appending(path: m.id.uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        for offset in frames {
            try Data([0xFF, 0xD8, 0xFF, UInt8(Int(offset) % 256)]).write(to: folder.appending(path: ScreenCaptureContract.frameName(at: start.addingTimeInterval(offset))))
        }
        for (i, part) in audio.enumerated() {
            let name = ScreenCaptureContract.audioPartName(i + 1)
            try WatchAudioFixture.write(folder.appending(path: name), seconds: part.seconds)
            m.parts.append(.init(file: name, start: start.addingTimeInterval(part.at),
                                 end: part.complete ? start.addingTimeInterval(part.at + part.seconds) : nil))
        }
        m.framesKept = frames.count
        m.heartbeat = ended ? start.addingTimeInterval(60) : now
        if ended { m.endedAt = start.addingTimeInterval(60) }
        try m.write(to: folder)
        return (folder, m)
    }

    private func makeMeeting(micLines: [(String, Double)] = []) -> Meeting {
        let m = Meeting(title: "Movie night", startedAt: start)
        m.endedAt = start.addingTimeInterval(60)
        context.insert(m)
        for (text, at) in micLines {
            let s = Segment(text: text, start: start.addingTimeInterval(at), duration: 3, audioOffset: at)
            context.insert(s)
            s.meeting = m
        }
        try? context.save()
        return m
    }

    func testScreensBecomeSnapshotsInTimeOrderAndLeaveTheFolder() throws {
        let (folder, _) = try makeBroadcast(frames: [12, 3, 7.5])
        let importer = makeImporter()
        let m = makeMeeting()
        let session = try XCTUnwrap(importer.sessions().first)
        XCTAssertTrue(importer.hasContent(session))
        XCTAssertEqual(importer.importFiles(of: session, into: m), 3)

        let shots = m.orderedSnapshots
        XCTAssertEqual(shots.map { $0.takenAt.timeIntervalSince(start) }, [3, 7.5, 12])
        XCTAssertTrue(shots.allSatisfy(\.isScreen))
        XCTAssertEqual(shots.map(\.source), Array(repeating: "screen", count: 3))
        for s in shots {
            XCTAssertTrue(FileManager.default.fileExists(atPath: snapshotsDir.appending(path: s.fileName).path(percentEncoded: false)))
        }
        XCTAssertTrue(importer.frames(in: session).isEmpty, "moved out of the shared container")
        // Running it again adds nothing
        XCTAssertEqual(importer.importFiles(of: ScreenCaptureImporter.Session(folder: folder, manifest: session.manifest), into: m), 0)
        XCTAssertEqual(m.snapshots.count, 3)
        // A photo stays a photo
        let photo = Snapshot(fileName: "p.jpg")
        context.insert(photo)
        photo.meeting = m
        XCTAssertFalse(photo.isScreen)
        XCTAssertEqual(m.screens.count, 3)
    }

    func testScreensAfterTheRecordingStoppedAreDeletedNotImported() throws {
        try makeBroadcast(frames: [5, 30, 50])
        let importer = makeImporter()
        let m = makeMeeting()
        let session = try XCTUnwrap(importer.sessions().first)
        importer.importFiles(of: session, into: m, keepUntil: start.addingTimeInterval(40))
        XCTAssertEqual(m.orderedSnapshots.map { $0.takenAt.timeIntervalSince(start) }, [5, 30])
        XCTAssertTrue(importer.frames(in: session).isEmpty, "the late one is deleted")
    }

    func testAppAudioWaitsForTranscriptionOnlyWhenAllowed() throws {
        try makeBroadcast(frames: [], audio: [(at: 2, seconds: 3, complete: true)])
        let importer = makeImporter()
        let m = makeMeeting()
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        let pending = m.pendingScreenAudio
        XCTAssertEqual(pending.count, 1)
        XCTAssertEqual(pending.first?.start, start.addingTimeInterval(2))
        XCTAssertTrue(FileManager.default.fileExists(atPath: audioDir.appending(path: pending[0].file).path(percentEncoded: false)))
    }

    func testFreeUserAppAudioIsDeletedNotKept() throws {
        let (folder, _) = try makeBroadcast(frames: [1], audio: [(at: 2, seconds: 2, complete: true)])
        appAudioAllowed = false
        let importer = makeImporter()
        let m = makeMeeting()
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        XCTAssertTrue(m.pendingScreenAudio.isEmpty)
        XCTAssertNil(m.screenAudioJSON)
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.appending(path: "a-1.m4a").path(percentEncoded: false)))
        XCTAssertEqual(m.snapshots.count, 1, "screens are free")
    }

    func testAppAudioFromABroadcastThatWasntAllowedIsNeverKept() throws {
        try makeBroadcast(frames: [], audio: [(at: 2, seconds: 2, complete: true)], appAudio: false)
        let importer = makeImporter()
        let m = makeMeeting()
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        XCTAssertTrue(m.pendingScreenAudio.isEmpty)
    }

    func testUnfinishedAudioIsLeftWhileLiveAndDeletedOnceOver() throws {
        let (folder, _) = try makeBroadcast(frames: [], audio: [(at: 2, seconds: 2, complete: false)], ended: false)
        let importer = makeImporter()
        let m = makeMeeting()
        let part = folder.appending(path: "a-1.m4a")
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        XCTAssertTrue(FileManager.default.fileExists(atPath: part.path(percentEncoded: false)), "still being written")
        now = now.addingTimeInterval(ScreenCaptureContract.staleAfter + 1)
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        XCTAssertFalse(FileManager.default.fileExists(atPath: part.path(percentEncoded: false)), "cut off: unreadable, deleted")
        XCTAssertTrue(m.pendingScreenAudio.isEmpty)
    }

    func testAppAudioBecomesOnScreenLinesInterleavedWithTheMicrophone() async throws {
        try makeBroadcast(frames: [], audio: [(at: 10, seconds: 12, complete: true)])
        let transcriber = ScriptedTranscriber(chunk: 30) { _, _ in
            [ScriptedTranscriber.line("The ship leaves at dawn tomorrow", at: 1),
             ScriptedTranscriber.line("we should all go and see the ocean", at: 6),
             ScriptedTranscriber.line("Nobody knows where the treasure went", at: 9)]
        }
        let importer = makeImporter(transcriber)
        // The mic heard the third line out loud (an echo), and the people talking
        let m = makeMeeting(micLines: [("Can you pass the popcorn please", 8),
                                       ("Nobody knows where the treasure went", 18.5),
                                       ("I love this part", 30)])
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        let file = try XCTUnwrap(m.pendingScreenAudio.first?.file)

        let done = await importer.transcribePending(meetingID: m.id)
        XCTAssertTrue(done)

        let lines = m.orderedSegments.map { ($0.isFromScreen ? "screen" : "mic", $0.text, $0.start.timeIntervalSince(start)) }
        XCTAssertEqual(lines.map(\.0), ["mic", "screen", "screen", "mic", "mic"])
        XCTAssertEqual(lines.map(\.1), ["Can you pass the popcorn please", "The ship leaves at dawn tomorrow",
                                        "we should all go and see the ocean", "Nobody knows where the treasure went", "I love this part"])
        XCTAssertEqual(lines[1].2, 11, accuracy: 0.01, "wall-clock: the part's start + the line's offset")
        XCTAssertEqual(lines[2].2, 16, accuracy: 0.01)
        let screen = m.orderedSegments.filter(\.isFromScreen)
        XCTAssertTrue(screen.allSatisfy { $0.audioOffset == nil && $0.wordTimingsJSON == nil }, "no audio in the recording's file")
        XCTAssertNil(m.screenAudioJSON, "nothing left to transcribe")
        XCTAssertFalse(FileManager.default.fileExists(atPath: audioDir.appending(path: file).path(percentEncoded: false)), "the app audio isn't kept")
        // Exports and prompts say where the line came from
        XCTAssertTrue(MeetingExport.markdown(m).contains("(On screen) The ship leaves at dawn tomorrow"))
        XCTAssertFalse(MeetingExport.markdown(m).contains("(On screen) Can you pass"))
    }

    func testTranscriptionResumesWhereItStopped() async throws {
        try makeBroadcast(frames: [], audio: [(at: 0, seconds: 20, complete: true)])
        var calls = 0
        let transcriber = ScriptedTranscriber(chunk: 8) { index, _ in
            calls += 1
            return [ScriptedTranscriber.line("Chunk number \(index) speaking clearly now", at: 0.5)]
        }
        let importer = makeImporter(transcriber)
        let m = makeMeeting()
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        var allowed = 1
        let first = await importer.transcribePending(meetingID: m.id) { defer { allowed -= 1 }; return allowed > 0 }
        XCTAssertFalse(first, "stopped after one chunk (this iPhone started recording)")
        XCTAssertEqual(m.segments.count, 1)
        XCTAssertGreaterThan(m.pendingScreenAudio.first?.done ?? 0, 0)
        let second = await importer.transcribePending(meetingID: m.id)
        XCTAssertTrue(second)
        XCTAssertEqual(m.segments.count, calls)
        XCTAssertGreaterThanOrEqual(calls, 3)
        XCTAssertNil(m.screenAudioJSON)
    }

    func testABroadcastWithNoRecordingGetsItsOwn() throws {
        let (_, manifest) = try makeBroadcast(frames: [4])
        let importer = makeImporter()
        let m = importer.makeRecording(for: manifest)
        XCTAssertEqual(m.startedAt, start)
        XCTAssertEqual(m.endedAt, start.addingTimeInterval(60))
        XCTAssertNil(m.audioFileName, "no microphone audio")
        XCTAssertTrue(RecordingSession.isDefaultTitle(m.title) || m.courseName != nil)
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        XCTAssertEqual(m.screens.count, 1)
    }

    // MARK: Purge (docs/REDACTION.md)

    func testDeleteRecordingPurgesWaitingAppAudioAndSharedLeftovers() throws {
        let (folder, manifest) = try makeBroadcast(frames: [1, 2], audio: [(at: 3, seconds: 2, complete: true)], ended: false)
        let importer = makeImporter()
        let m = makeMeeting()
        links.link(manifest.id, to: m.id)
        // Part imported (waiting for transcription); more screens still in the shared folder
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        try Data([1]).write(to: folder.appending(path: ScreenCaptureContract.frameName(at: start.addingTimeInterval(9))))
        let waiting = audioDir.appending(path: try XCTUnwrap(m.pendingScreenAudio.first?.file))

        importer.purge(meeting: m, links: links)
        XCTAssertFalse(FileManager.default.fileExists(atPath: waiting.path(percentEncoded: false)))
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.path(percentEncoded: false)), "shared container leftovers go too")
        XCTAssertNil(m.screenAudioJSON)
        XCTAssertTrue(links.broadcasts(for: m.id).isEmpty)
    }

    func testLeftoversAreRemovedAtLaunch() throws {
        let importer = makeImporter()
        // A broadcast whose recording was deleted, and one still linked to a recording
        let (gone, goneManifest) = try makeBroadcast(frames: [1])
        let (kept, keptManifest) = try makeBroadcast(frames: [2])
        let m = makeMeeting()
        links.link(goneManifest.id, to: UUID())
        links.link(keptManifest.id, to: m.id)
        // App audio no recording waits for, and one that is waited for
        let orphan = audioDir.appending(path: "screen-\(UUID().uuidString)-dead.m4a")
        let wanted = audioDir.appending(path: "screen-\(m.id.uuidString)-live.m4a")
        try Data([1]).write(to: orphan)
        try Data([1]).write(to: wanted)
        m.pendingScreenAudio = [PendingScreenAudio(file: wanted.lastPathComponent, start: start)]
        try context.save()
        let micFile = audioDir.appending(path: "\(m.id.uuidString).m4a")
        try Data([1]).write(to: micFile)

        importer.removeLeftovers(links: links)
        XCTAssertFalse(FileManager.default.fileExists(atPath: gone.path(percentEncoded: false)))
        XCTAssertTrue(FileManager.default.fileExists(atPath: kept.path(percentEncoded: false)))
        XCTAssertFalse(FileManager.default.fileExists(atPath: orphan.path(percentEncoded: false)))
        XCTAssertTrue(FileManager.default.fileExists(atPath: wanted.path(percentEncoded: false)))
        XCTAssertTrue(FileManager.default.fileExists(atPath: micFile.path(percentEncoded: false)), "microphone audio is never touched")
        XCTAssertNil(links.meetingID(for: goneManifest.id))
    }

    func testStrikeAScreenRemovesItsFileAndLeavesAMarker() async throws {
        try makeBroadcast(frames: [5])
        let importer = makeImporter(snapshots: Storage.snapshots)
        let m = makeMeeting()
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        let screen = try XCTUnwrap(m.screens.first)
        let url = screen.fileURL
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path(percentEncoded: false)))
        try await RedactionEngine.strike(.screens([screen]), reason: nil, meeting: m, context: context)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path(percentEncoded: false)))
        XCTAssertTrue(m.snapshots.isEmpty)
        XCTAssertEqual(m.screenStrikes.count, 1)
        XCTAssertEqual(m.screenStrikes.first?.coveredFrom, start.addingTimeInterval(5))
    }

    func testDeleteScreensCommitsLikePhotos() async throws {
        try makeBroadcast(frames: [5, 6])
        let importer = makeImporter(snapshots: Storage.snapshots)
        let m = makeMeeting()
        importer.importFiles(of: try XCTUnwrap(importer.sessions().first), into: m)
        let urls = m.screens.map(\.fileURL)
        let pending = try RedactionEngine.delete(.screens(m.screens), meeting: m, context: context)
        XCTAssertEqual(pending.label, "2 screens deleted")
        try await RedactionEngine.commit(pending, context: context)
        for url in urls { XCTAssertFalse(FileManager.default.fileExists(atPath: url.path(percentEncoded: false))) }
        XCTAssertTrue(m.redactions.isEmpty)
    }

    func testStrikeAnOnScreenLineRemovesTheTextButNotTheMicrophoneAudio() async throws {
        let m = makeMeeting(micLines: [("We talked over the film", 2)])
        let audioName = "screen-strike-\(UUID().uuidString).m4a"
        let audioURL = Storage.audio.appending(path: audioName)
        try WatchAudioFixture.write(audioURL, seconds: 30)
        defer { try? FileManager.default.removeItem(at: audioURL) }
        m.audioFileName = audioName
        let line = Segment(text: "The secret code is swordfish", start: start.addingTimeInterval(10), duration: 3)
        line.source = Snapshot.Source.screen
        context.insert(line)
        line.meeting = m
        try context.save()
        let before = try Data(contentsOf: audioURL)

        let plan = try RedactionEngine.plan(.lines([line]), in: m)
        XCTAssertTrue(plan.audioRanges.isEmpty, "an On screen line has no audio in the recording's file")
        try await RedactionEngine.strike(.lines([line]), reason: nil, meeting: m, context: context)
        XCTAssertFalse(RedactionText.plainTranscript(m).contains("swordfish"))
        XCTAssertEqual(try Data(contentsOf: audioURL), before, "the microphone recording is untouched")
    }

    // MARK: Migration

    /// A store from before screen capture (no Snapshot/Segment source,
    /// no screenAudioJSON) opens with the current model.
    func testStoreFromBeforeScreenCaptureMigrates() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "nf-migrate-screen-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appending(path: "old.store")
        try autoreleasepool {
            let old = try ModelContainer(for: Schema(PreKindSchema.models), configurations: ModelConfiguration(url: url))
            let ctx = ModelContext(old)
            let m = PreKindSchema.Meeting(title: "Before screens", startedAt: start)
            ctx.insert(m)
            let photo = PreKindSchema.Snapshot(fileName: "photo.jpg")
            ctx.insert(photo)
            photo.meeting = m
            let s = PreKindSchema.Segment(text: "Hello from before", start: start, duration: 2)
            ctx.insert(s)
            s.meeting = m
            try ctx.save()
        }
        let container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(url: url))
        let context = ModelContext(container)
        let m = try XCTUnwrap(try context.fetch(FetchDescriptor<Meeting>()).first)
        XCTAssertEqual(m.snapshots.map(\.isScreen), [false])
        XCTAssertEqual(m.segments.map(\.isFromScreen), [false])
        XCTAssertNil(m.screenAudioJSON)
        let screen = Snapshot(fileName: "screen.jpg", takenAt: start)
        screen.source = Snapshot.Source.screen
        context.insert(screen)
        screen.meeting = m
        try context.save()
        let again = try XCTUnwrap(try ModelContext(container).fetch(FetchDescriptor<Meeting>()).first)
        XCTAssertEqual(again.screens.count, 1)
    }

    // MARK: Links

    func testLinksMapBroadcastsToRecordings() {
        let b1 = UUID(), b2 = UUID(), m = UUID()
        links.link(b1, to: m)
        links.link(b2, to: m)
        XCTAssertEqual(links.meetingID(for: b1), m)
        XCTAssertEqual(Set(links.broadcasts(for: m)), [b1, b2])
        links.unlink(b1)
        XCTAssertNil(links.meetingID(for: b1))
        XCTAssertEqual(links.broadcasts(for: m), [b2])
    }
}
