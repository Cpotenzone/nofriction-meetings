import AVFoundation
import SQLite3
import SwiftData
import XCTest
@testable import noFriction

/// docs/REDACTION.md "Tests (both platforms)", iOS side.
final class RedactionTextTests: XCTestCase {
    private func words(_ text: String) -> [String] { RedactionText.tokens(text).map(\.text) }

    func testTokensKeepPunctuationOnWords() {
        XCTAssertEqual(words("Yes,  honestly, it works."), ["Yes,", "honestly,", "it", "works."])
    }

    func testSpliceMiddleClosesUpWhitespace() {
        let text = "We need the Azure credits by Friday."
        let toks = RedactionText.tokens(text)
        let out = RedactionText.splice(text, removing: RedactionText.span(toks, 2...3))
        XCTAssertEqual(out.text, "We need credits by Friday.")
    }

    func testSpliceAtBoundaries() {
        let text = "We need the credits."
        let toks = RedactionText.tokens(text)
        XCTAssertEqual(RedactionText.splice(text, removing: RedactionText.span(toks, 0...0)).text, "need the credits.")
        XCTAssertEqual(RedactionText.splice(text, removing: RedactionText.span(toks, 3...3)).text, "We need the")
        XCTAssertEqual(RedactionText.splice(text, removing: RedactionText.span(toks, 0...3)).text, "")
    }

    func testSpliceIrregularWhitespaceAndPunctuation() {
        let text = "Hello   world,\tagain  and again."
        let toks = RedactionText.tokens(text)
        XCTAssertEqual(RedactionText.splice(text, removing: RedactionText.span(toks, 1...1)).text, "Hello again  and again.", "only the seam is closed up")
        let t2 = "Yes, honestly, it works."
        XCTAssertEqual(RedactionText.splice(t2, removing: RedactionText.span(RedactionText.tokens(t2), 1...1)).text, "Yes, it works.")
    }

    func testSpliceShiftsAndDropsWordTimings() {
        let text = "alpha beta gamma delta"
        let toks = RedactionText.tokens(text)
        let timings = toks.enumerated().map { i, t in
            WordTiming(location: t.range.location, length: t.range.length, start: Double(i), end: Double(i) + 0.8)
        }
        let out = RedactionText.splice(text, timings: timings, removing: RedactionText.span(toks, 1...2))
        XCTAssertEqual(out.text, "alpha delta")
        XCTAssertEqual(out.removedTimings.map(\.start), [1, 2])
        let ns = out.text as NSString
        XCTAssertEqual(out.timings.map { ns.substring(with: $0.range) }, ["alpha", "delta"])
        XCTAssertEqual(out.timings.map(\.start), [0, 3])
    }

    func testStrikeMarkerInsertionAndRendering() {
        let id = UUID()
        let text = "We need the Azure credits by Friday."
        let toks = RedactionText.tokens(text)
        let out = RedactionText.splice(text, removing: RedactionText.span(toks, 3...4), inserting: RedactionText.markerToken(id))
        XCTAssertEqual(out.text, "We need the \(RedactionText.markerToken(id)) by Friday.")
        XCTAssertFalse(out.text.contains("Azure"))
        XCTAssertEqual(RedactionText.pieces(out.text), [.text("We need the"), .marker(id), .text("by Friday.")])
        XCTAssertEqual(RedactionText.plain(out.text), "We need the [stricken from the record] by Friday.")

        // Markers can't be selected or swallowed by a word range
        let after = RedactionText.tokens(out.text)
        XCTAssertEqual(after[3].kind, .marker(id))
        XCTAssertFalse(RedactionText.isWordsOnly(after, 2...4))
        XCTAssertEqual(RedactionText.wordRuns(after), [0...2, 4...5])
        // A whole-line strike keeps an existing marker and replaces the word runs around it
        let id2 = UUID()
        let line = RedactionText.splice(out.text, timings: [], removingTokenRanges: RedactionText.wordRuns(after),
                                        inserting: RedactionText.markerToken(id2)).text
        XCTAssertEqual(RedactionText.pieces(line), [.marker(id2), .marker(id), .marker(id2)])
    }

    func testOnlyMarker() {
        let id = UUID()
        XCTAssertEqual(RedactionText.onlyMarker(RedactionText.markerToken(id)), id)
        XCTAssertNil(RedactionText.onlyMarker("x " + RedactionText.markerToken(id)))
    }

    func testAINotesRedactionExactAndCaseInsensitive() {
        let notes = "## Summary\nAzure credits are due. AZURE   Credits again; Azurecredits stays; the other word."
        let struck = RedactionText.redact(notes, phrases: ["Azure credits,"], replacement: RedactionText.placeholder)
        XCTAssertEqual(struck, "## Summary\n[stricken from the record] are due. [stricken from the record] again; Azurecredits stays; the other word.")
        // Whole words only: removing "Zenith" doesn't touch "Zeniths"
        let deleted = RedactionText.redact("Ask Zenith, the Zeniths team, Zenith end.", phrases: ["Zenith"], replacement: "")
        XCTAssertEqual(deleted, "Ask, the Zeniths team, end.")
    }

    func testCommonWordsAreNotRewrittenInNotes() {
        // A common/short word would rewrite unrelated notes: left alone
        let notes = "The plan is approved. The team owns the plan."
        XCTAssertEqual(RedactionText.redact(notes, phrases: ["plan"], replacement: RedactionText.placeholder), notes)
        XCTAssertEqual(RedactionText.redact(notes, phrases: ["the"], replacement: ""), notes)
        XCTAssertEqual(RedactionText.redact(notes, phrases: ["because"], replacement: ""), notes)
        // Two words, or one long uncommon word, are rewritten
        XCTAssertEqual(RedactionText.redact(notes, phrases: ["the plan"], replacement: "[x]"), "[x] is approved. The team owns [x].")
        XCTAssertTrue(RedactionText.isDistinctive("Marguerite"))
        XCTAssertFalse(RedactionText.isDistinctive("Plan."))
        XCTAssertTrue(RedactionText.isDistinctive("the plan"))
    }
}

@MainActor
final class RedactionEngineTests: XCTestCase {
    private var container: ModelContainer!
    private var context: ModelContext!
    private var created: [URL] = []

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        context = container.mainContext
        Storage.prepare()
    }

    override func tearDown() async throws {
        for url in created { try? FileManager.default.removeItem(at: url) }
        container = nil
        context = nil
    }

    private func makeMeeting(_ lines: [String], notes: String? = nil) -> Meeting {
        let start = Date(timeIntervalSince1970: 1_790_000_000)
        let m = Meeting(title: "Test", startedAt: start)
        m.endedAt = start.addingTimeInterval(120)
        m.aiNotes = notes
        context.insert(m)
        for (i, line) in lines.enumerated() {
            let toks = RedactionText.tokens(line)
            let base = Double(i) * 10
            let timings = toks.enumerated().map { j, t in
                WordTiming(location: t.range.location, length: t.range.length, start: base + Double(j) * 0.5, end: base + Double(j) * 0.5 + 0.4)
            }
            let s = Segment(text: line, start: start.addingTimeInterval(base), duration: 8, audioOffset: base, wordTimings: timings)
            context.insert(s)
            s.meeting = m
        }
        try? context.save()
        return m
    }

    private func makePhoto(_ m: Meeting, at offset: Double) throws -> Snapshot {
        let name = "test-\(UUID().uuidString).jpg"
        let url = Storage.snapshots.appending(path: name)
        try Data([0xFF, 0xD8, 0xFF]).write(to: url)
        created.append(url)
        let s = Snapshot(fileName: name, takenAt: m.startedAt.addingTimeInterval(offset))
        context.insert(s)
        s.meeting = m
        try context.save()
        return s
    }

    func testStrikeWordsLeavesMarkerRedactsNotesAndExports() async throws {
        let m = makeMeeting(["We need the Azure credits by Friday.", "Marcus owns it."],
                            notes: "## Summary\nDecision on azure credits by Friday.")
        let seg = m.orderedSegments[0]
        try await RedactionEngine.strike(.words(seg, 3...4), reason: "privileged", meeting: m, context: context)

        XCTAssertFalse(seg.text.contains("Azure"))
        XCTAssertFalse(seg.text.contains("credits"))
        XCTAssertEqual(RedactionText.plain(seg.text), "We need the [stricken from the record] by Friday.")
        // Timings of struck words are gone; the rest still point at their words
        let ns = seg.text as NSString
        XCTAssertEqual(seg.wordTimings.map { ns.substring(with: $0.range) }, ["We", "need", "the", "by", "Friday."])

        let r = try XCTUnwrap(m.redactions.first)
        XCTAssertEqual(m.redactions.count, 1)
        XCTAssertEqual(r.action, "strike")
        XCTAssertEqual(r.kind, "words")
        XCTAssertEqual(r.reason, "privileged")
        XCTAssertEqual(r.mediaStart, 1.5 - RedactionEngine.padding, accuracy: 0.001)
        XCTAssertEqual(r.mediaEnd, 2.4 + RedactionEngine.padding, accuracy: 0.001)

        XCTAssertEqual(m.aiNotes, "## Summary\nDecision on [stricken from the record] by Friday.")
        XCTAssertTrue(m.aiNotesStale)

        let export = MeetingExport.markdown(m)
        XCTAssertTrue(export.contains("We need the [stricken from the record] by Friday."))
        XCTAssertFalse(export.localizedCaseInsensitiveContains("azure"))
        XCTAssertFalse(export.contains("⟦"))
        let prompt = MeetingAI.context(m)
        XCTAssertTrue(prompt.contains("[stricken from the record]"))
        XCTAssertFalse(prompt.localizedCaseInsensitiveContains("azure"))
        XCTAssertFalse(m.transcriptText.localizedCaseInsensitiveContains("azure"), "search text still has it")
    }

    func testStrikeCantSwallowAMarker() async throws {
        let m = makeMeeting(["one two three four"])
        let seg = m.orderedSegments[0]
        try await RedactionEngine.strike(.words(seg, 1...1), reason: nil, meeting: m, context: context)
        // tokens: one ⟦m⟧ three four — a range across the marker is refused
        XCTAssertThrowsError(try RedactionEngine.plan(.words(seg, 0...2), in: m))
        XCTAssertThrowsError(try RedactionEngine.delete(.words(seg, 0...2), meeting: m, context: context))
        XCTAssertEqual(m.redactions.count, 1)
    }

    func testStrikeLinesCollapsesIntoOneMarkerInExport() async throws {
        let m = makeMeeting(["first secret line", "second secret line", "public line"])
        let segs = Array(m.orderedSegments.prefix(2))
        try await RedactionEngine.strike(.lines(segs), reason: nil, meeting: m, context: context)
        XCTAssertEqual(m.redactions.count, 1)
        XCTAssertNotNil(RedactionText.onlyMarker(segs[0].text))
        XCTAssertEqual(RedactionText.onlyMarker(segs[0].text), RedactionText.onlyMarker(segs[1].text))
        let entries = RedactionText.entries(m).map(\.text)
        XCTAssertEqual(entries, ["[stricken from the record]", "public line"])
        XCTAssertFalse(MeetingExport.markdown(m).contains("secret"))
    }

    func testStrikeScreenDeletesFileAndLeavesPlaceholder() async throws {
        let m = makeMeeting(["hello there"])
        let photo = try makePhoto(m, at: 5)
        let url = photo.fileURL
        try await RedactionEngine.strike(.screens([photo]), reason: "personal", meeting: m, context: context)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path(percentEncoded: false)))
        XCTAssertTrue(m.snapshots.isEmpty)
        XCTAssertEqual(m.screenStrikes.count, 1)
        XCTAssertEqual(m.screenStrikes.first?.coveredFrom, m.startedAt.addingTimeInterval(5))
        XCTAssertTrue(MeetingExport.markdown(m).contains("[screen stricken from the record]"))
        XCTAssertTrue(MeetingAI.context(m).contains("[screen stricken from the record]"))
    }

    func testPhotoDeleteRemovesFileOnCommitAndLeavesNoTrace() async throws {
        let m = makeMeeting(["hello there"])
        let photo = try makePhoto(m, at: 5)
        let url = photo.fileURL
        let pending = try RedactionEngine.delete(.screens([photo]), meeting: m, context: context)
        XCTAssertTrue(m.snapshots.isEmpty, "row hidden immediately")
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path(percentEncoded: false)), "file kept for the undo window")
        try await RedactionEngine.commit(pending, context: context)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path(percentEncoded: false)))
        XCTAssertTrue(m.redactions.isEmpty, "Delete leaves no record")
        XCTAssertFalse(MeetingExport.markdown(m).contains("stricken"))
    }

    func testDeleteWordsUndoRestoresExactly() throws {
        let m = makeMeeting(["keep these words please"], notes: "These words matter.")
        let seg = m.orderedSegments[0]
        let before = (seg.text, seg.wordTimingsJSON)
        let pending = try RedactionEngine.delete(.words(seg, 1...2), meeting: m, context: context)
        XCTAssertEqual(seg.text, "keep please")
        XCTAssertEqual(pending.label, "2 words deleted")
        try RedactionEngine.undo(pending, context: context)
        XCTAssertEqual(seg.text, before.0)
        XCTAssertEqual(seg.wordTimingsJSON, before.1)
        XCTAssertTrue(m.redactions.isEmpty)
        XCTAssertEqual(m.aiNotes, "These words matter.")
        XCTAssertFalse(m.aiNotesStale)
    }

    func testDeletingACommonWordOnlyMarksNotesStale() async throws {
        let m = makeMeeting(["the plan is late", "we need a plan"], notes: "The plan for hiring is approved.")
        let pending = try RedactionEngine.delete(.words(m.orderedSegments[0], 1...1), meeting: m, context: context)
        try await RedactionEngine.commit(pending, context: context)
        XCTAssertEqual(m.orderedSegments[0].text, "the is late")
        XCTAssertEqual(m.aiNotes, "The plan for hiring is approved.")
        XCTAssertTrue(m.aiNotesStale)
    }

    func testDeleteWholeLineRemovesSegmentAndCommitRedactsNotes() async throws {
        let m = makeMeeting(["junk false start", "real content"], notes: "Summary: junk false start then real content.")
        let pending = try RedactionEngine.delete(.lines([m.orderedSegments[0]]), meeting: m, context: context)
        XCTAssertEqual(m.orderedSegments.map(\.text), ["real content"])
        try await RedactionEngine.commit(pending, context: context)
        XCTAssertEqual(m.aiNotes, "Summary: then real content.")
        XCTAssertTrue(m.aiNotesStale)
        XCTAssertTrue(m.redactions.isEmpty)
        // Undo a deleted line after the fact isn't possible: there's no record left
        XCTAssertFalse(m.transcriptText.contains("junk"))
    }

    func testUndoReinsertsDeletedLine() throws {
        let m = makeMeeting(["only line"])
        let pending = try RedactionEngine.delete(.lines([m.orderedSegments[0]]), meeting: m, context: context)
        XCTAssertTrue(m.segments.isEmpty)
        try RedactionEngine.undo(pending, context: context)
        XCTAssertEqual(m.orderedSegments.map(\.text), ["only line"])
        XCTAssertEqual(m.orderedSegments.first?.wordTimings.count, 2)
    }

    func testRecoverFinishesInterruptedDelete() async throws {
        let m = makeMeeting(["hello"], notes: "hello notes")
        let photo = try makePhoto(m, at: 1)
        let url = photo.fileURL
        _ = try RedactionEngine.delete(.screens([photo]), meeting: m, context: context)
        // App killed here: the in-memory undo state is gone, the record isn't
        XCTAssertEqual(m.redactions.count, 1)
        await RedactionEngine.recover(context: context)
        XCTAssertFalse(FileManager.default.fileExists(atPath: url.path(percentEncoded: false)))
        XCTAssertTrue(m.redactions.isEmpty)
        XCTAssertTrue(m.aiNotesStale)
    }

    func testStrikeSilencesTheWordsInTheRecording() async throws {
        let m = makeMeeting(["alpha beta gamma delta"])
        let name = "test-\(UUID().uuidString).m4a"
        let url = Storage.audio.appending(path: name)
        created.append(url)
        try AudioFixtures.writeTone(url, seconds: 4)
        m.audioFileName = name
        let frames = try AVAudioFile(forReading: url).length
        // "gamma" is at 1.0–1.4 s → silenced 0.85–1.55 s
        try await RedactionEngine.strike(.words(m.orderedSegments[0], 2...2), reason: nil, meeting: m, context: context)
        let (samples, rate, length) = try AudioFixtures.decode(url)
        XCTAssertEqual(length, frames, "duration changed")
        XCTAssertEqual(AudioFixtures.peak(samples, rate, 0.92...1.48), 0)
        XCTAssertGreaterThan(AudioFixtures.peak(samples, rate, 0.2...0.7), 0.2)
        XCTAssertGreaterThan(AudioFixtures.peak(samples, rate, 1.8...3.5), 0.2)
    }
}

final class AudioSilencerTests: XCTestCase {
    func testSilencedRangeIsAllZerosAndDurationUnchanged() throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "silence-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appending(path: "tone.m4a")
        try AudioFixtures.writeTone(url, seconds: 3)
        let before = try AVAudioFile(forReading: url)
        let frames = before.length
        let seconds = Double(frames) / before.processingFormat.sampleRate

        try AudioSilencer.silence(url, ranges: [1.0...2.0])

        let (samples, rate, length) = try AudioFixtures.decode(url)
        XCTAssertEqual(length, frames)
        XCTAssertEqual(Double(length) / rate, seconds, accuracy: 0.001)
        // The whole range decodes to digital zero, bar AAC's one-frame overlap at the edges
        let edge = 2048.0 / rate
        XCTAssertEqual(AudioFixtures.peak(samples, rate, (1.0 + edge)...(2.0 - edge)), 0)
        XCTAssertGreaterThan(AudioFixtures.peak(samples, rate, 0.1...0.9), 0.2)
        XCTAssertGreaterThan(AudioFixtures.peak(samples, rate, 2.1...2.9), 0.2)
        // No temp files left behind
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: dir.path(percentEncoded: false)), ["tone.m4a"])
    }

    func testNormalizedMergesOverlaps() {
        XCTAssertEqual(AudioSilencer.normalized([2...3, -1...0.5, 0.4...1]), [0...1, 2...3])
    }
}

/// The SQLite store on disk must not keep stricken text in free pages or the WAL.
@MainActor
final class StoreHygieneTests: XCTestCase {
    func testStrickenTextIsNotLeftInTheStoreFiles() async throws {
        let dir = FileManager.default.temporaryDirectory.appending(path: "store-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let storeURL = dir.appending(path: "test.store")
        let container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(url: storeURL))
        let context = container.mainContext

        let secret = "ZEBRAQUARTZ" + String(Int.random(in: 100_000...999_999))
        let m = Meeting(title: "Store test")
        context.insert(m)
        for i in 0..<20 {
            let s = Segment(text: i == 7 ? "the code is \(secret) okay" : "filler line number \(i) with some words", start: m.startedAt.addingTimeInterval(Double(i)), duration: 1)
            context.insert(s)
            s.meeting = m
        }
        try context.save()
        XCTAssertTrue(Self.filesContain(dir, secret), "fixture: secret should be on disk before the strike")

        let line = try XCTUnwrap(m.orderedSegments.first { $0.text.contains(secret) })
        let result = try await RedactionEngine.strike(.words(line, 3...3), reason: nil, meeting: m, context: context)
        XCTAssertTrue(result.storage.compacted)
        XCTAssertFalse(Self.filesContain(dir, secret), "stricken text survives somewhere in the store files")
    }

    func testSecureDeleteDefaultIsReported() throws {
        // Informational: whether the OS SQLite already defaults secure_delete on.
        var db: OpaquePointer?
        XCTAssertEqual(sqlite3_open(":memory:", &db), SQLITE_OK)
        defer { sqlite3_close(db) }
        var stmt: OpaquePointer?
        sqlite3_prepare_v2(db, "PRAGMA secure_delete;", -1, &stmt, nil)
        defer { sqlite3_finalize(stmt) }
        XCTAssertEqual(sqlite3_step(stmt), SQLITE_ROW)
        print("NF: system SQLite secure_delete default =", sqlite3_column_int(stmt, 0))
    }

    static func filesContain(_ dir: URL, _ needle: String) -> Bool {
        let bytes = Data(needle.utf8)
        let files = (try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? []
        return files.contains { url in
            // Read through the file system (what's in the files now)
            guard let data = try? Data(contentsOf: url) else { return false }
            return data.range(of: bytes) != nil
        }
    }
}

enum AudioFixtures {
    static func writeTone(_ url: URL, seconds: Double, rate: Double = 44_100) throws {
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: rate,
            AVNumberOfChannelsKey: 1, AVEncoderBitRateKey: 64_000,
        ]
        let file = try AVAudioFile(forWriting: url, settings: settings, commonFormat: .pcmFormatFloat32, interleaved: false)
        let n = AVAudioFrameCount(seconds * rate)
        let buf = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: n))
        buf.frameLength = n
        let p = buf.floatChannelData![0]
        for i in 0..<Int(n) { p[i] = 0.5 * sinf(2 * .pi * 440 * Float(i) / Float(rate)) }
        try file.write(from: buf)
    }

    static func decode(_ url: URL) throws -> ([Float], Double, AVAudioFramePosition) {
        let file = try AVAudioFile(forReading: url)
        let n = AVAudioFrameCount(file.length)
        let buf = try XCTUnwrap(AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: n))
        try file.read(into: buf)
        let p = buf.floatChannelData![0]
        return (Array(UnsafeBufferPointer(start: p, count: Int(buf.frameLength))), file.processingFormat.sampleRate, file.length)
    }

    static func peak(_ samples: [Float], _ rate: Double, _ range: ClosedRange<Double>) -> Float {
        let a = max(0, Int(range.lowerBound * rate)), b = min(samples.count - 1, Int(range.upperBound * rate))
        guard a <= b else { return 0 }
        return samples[a...b].map(abs).max() ?? 0
    }
}
