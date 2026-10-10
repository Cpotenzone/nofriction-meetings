import SwiftData
import XCTest
@testable import noFriction

/// docs/SYNC.md, iOS side: wire compatibility with the Mac (golden fixtures
/// written by the Rust tests), auth vectors, the shared merge cases, and the
/// engine against an in-memory store, including removals through the purge.
private func fixture(_ name: String) throws -> Data {
    let bundle = Bundle(for: SyncWireTests.self)
    let url = try XCTUnwrap(bundle.url(forResource: name, withExtension: "json")
        ?? bundle.url(forResource: name, withExtension: "json", subdirectory: "SyncFixtures"),
        "\(name).json must be in the test bundle (NoFrictionTests/SyncFixtures, written by the Rust tests)")
    return try Data(contentsOf: url)
}

final class SyncWireTests: XCTestCase {
    static let messages = ["pair", "paired", "hello", "challenge", "auth", "welcome", "pull", "applied", "applied_retry",
                           "done", "error", "batch_removals", "batch_changes"]

    func testGoldenMessagesDecodeAndReencodeByteForByte() throws {
        for name in Self.messages {
            let data = try fixture(name)
            let msg = try SyncMessage.decode(data)
            XCTAssertEqual(String(decoding: msg.encode(), as: UTF8.self), String(decoding: data, as: UTF8.self), name)
        }
    }

    func testGoldenBatchFieldsAreReadRight() throws {
        guard case .batch(let phase, let items, let last, let upto) = try SyncMessage.decode(try fixture("batch_changes")) else {
            return XCTFail("not a batch")
        }
        XCTAssertEqual(phase, .changes)
        XCTAssertTrue(last)
        XCTAssertEqual(upto, 77)
        guard case .recording(let r) = items[0], case .line(let l) = items[2], case .topic(let t) = items[6] else { return XCTFail("order") }
        XCTAssertEqual(r.title, "Lecture — \"Cells\" / 1")
        XCTAssertEqual(r.cal?.location, "Room 4")
        XCTAssertEqual(r.people.first?.role, "organizer")
        XCTAssertEqual(l.text, "Café ünïcödé 日本語 \"quoted\"\ttab")
        XCTAssertEqual(l.src, "screen")
        XCTAssertEqual(t.conf, 875)
    }

    func testVersionAndGarbageAreRefused() {
        XCTAssertThrowsError(try SyncMessage.decode(Data(#"{"t":"done","v":2}"#.utf8))) { XCTAssertEqual($0 as? SyncWireError, .version(2)) }
        XCTAssertThrowsError(try SyncMessage.decode(Data(#"{"t":"nope","v":1}"#.utf8)))
        XCTAssertThrowsError(try SyncMessage.decode(Data("not json".utf8)))
    }

    func testFramingAndBatches() throws {
        let f = SyncMessage.done.frame()
        XCTAssertEqual(Array(f.prefix(4)), [0, 0, 0, UInt8(f.count - 4)])
        XCTAssertEqual(try SyncMessage.decode(f.dropFirst(4)), .done)
        XCTAssertEqual(SyncMessage.batches([]).count, 1)
        let items = (0..<1001).map { SyncItem.gone(GoneItem(entity: "mark", id: "\($0)", rec: nil)) }
        XCTAssertEqual(SyncMessage.batches(items).map(\.count), [500, 500, 1])
    }

    func testVectorsMatchTheMac() throws {
        let v = try JSONValue.parse(try fixture("vectors"))
        let secret = try XCTUnwrap(Data(base64Encoded: try XCTUnwrap(v["secret"]?.string)))
        let key = SyncCrypto.tokenKey(secret: secret)
        XCTAssertEqual(key.map { String(format: "%02x", $0) }.joined(), v["token_key"]?.string)
        for w in try XCTUnwrap(v["words"]?.array) {
            XCTAssertEqual(SyncCrypto.wordHash(tokenKey: key, try XCTUnwrap(w["word"]?.string)), w["hash"]?.string)
        }
        let np = try XCTUnwrap(Data(base64Encoded: v["nonce_phone"]!.string!)), nm = try XCTUnwrap(Data(base64Encoded: v["nonce_mac"]!.string!))
        XCTAssertEqual(SyncCrypto.macProof(secret: secret, noncePhone: np, nonceMac: nm).base64EncodedString(), v["mac_proof"]?.string)
        XCTAssertEqual(SyncCrypto.phoneProof(secret: secret, nonceMac: nm, noncePhone: np).base64EncodedString(), v["phone_proof"]?.string)
        XCTAssertFalse(SyncCrypto.equal(SyncCrypto.macProof(secret: Data(repeating: 9, count: 32), noncePhone: np, nonceMac: nm),
                                        SyncCrypto.macProof(secret: secret, noncePhone: np, nonceMac: nm)), "a wrong secret fails")
    }

    func testSharedMergeCases() throws {
        let v = try JSONValue.parse(try fixture("vectors"))
        let key = SyncCrypto.tokenKey(secret: Data(base64Encoded: v["secret"]!.string!)!)
        let cases = try XCTUnwrap(JSONValue.parse(try fixture("merge_cases")).array)
        XCTAssertGreaterThan(cases.count, 5)
        for c in cases {
            let name = c["name"]!.string!
            let local = c["local"]!.string!
            let keep = try XCTUnwrap(SyncMerge.parseKeep(c["keep"]!.array!.map { $0.string! }), name)
            let plan = SyncMerge.plan(local: SyncText.tokens(local), keep: keep, tokenKey: key)
            XCTAssertEqual(SyncMerge.apply(plan, toWire: local), c["expected"]?.string, name)
        }
    }

    func testIDsMarkersAndTokens() {
        XCTAssertEqual(SyncIDs.wire("0123456789ABCDEF0123456789abcdef"), "01234567-89ab-cdef-0123-456789abcdef")
        XCTAssertNil(SyncIDs.wire("nope"))
        let id = UUID()
        let local = "one \(RedactionText.markerToken(id)) two"
        let wire = SyncText.toWire(local)
        XCTAssertEqual(wire, "one ⟦stricken:\(id.uuidString.lowercased())⟧ two")
        XCTAssertEqual(SyncText.fromWire(wire), local)
        XCTAssertEqual((local as NSString).length, (wire as NSString).length, "offsets are the same in both forms")
        let toks = SyncText.tokens("a\u{00A0}b  \(wire)")
        XCTAssertEqual(toks.count, 5)
        XCTAssertEqual(SyncCrypto.keepList(tokenKey: Data([1]), wireText: "Zenith Labs").allSatisfy { $0.hasPrefix("w:") && !$0.contains("Zenith") }, true)
    }

    func testPairingLink() throws {
        let fp = String(repeating: "ab", count: 32)
        let link = try XCTUnwrap(PairingLink("nfsync:1?id=7c1e1b2a-3f4d-4e5f-8a9b-0c1d2e3f4a5b&n=Casey%27s%20Mac&fp=\(fp)&h=192.168.1.5%2C10.0.0.2&p=5000&c=ABCDEFGHJK"))
        XCTAssertEqual(link.name, "Casey's Mac")
        XCTAssertEqual(link.hosts, ["192.168.1.5", "10.0.0.2"])
        XCTAssertEqual(link.port, 5000)
        XCTAssertNil(PairingLink("https://example.com"))
        XCTAssertNil(PairingLink("nfsync:1?id=x&fp=\(fp)&p=1&c=A"), "bad id")
        XCTAssertNil(PairingLink("nfsync:1?id=7c1e1b2a-3f4d-4e5f-8a9b-0c1d2e3f4a5b&fp=short&p=1&c=A"), "bad fingerprint")
    }
}

@MainActor
final class SyncEngineTests: XCTestCase {
    private var container: ModelContainer!
    private var context: ModelContext { container.mainContext }
    private var dir: URL!
    private let key = SyncCrypto.tokenKey(secret: Data((0..<32).map { UInt8($0) }))
    private let macID = "00000000-0000-4000-8000-0000000000ee"

    override func setUp() async throws {
        container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        dir = FileManager.default.temporaryDirectory.appending(path: "nf-sync-\(UUID().uuidString)")
        SyncLedger.directory = dir
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        container = nil
    }

    private var engine: SyncEngine { SyncEngine(context: context, tokenKey: key) }
    private func newState() -> SyncMacState {
        SyncMacState(macID: macID, name: "Mac", fingerprint: String(repeating: "0", count: 64), hosts: [], port: 1)
    }

    private func recording(_ title: String = "Board", lines: [String] = []) -> Meeting {
        let m = Meeting(title: title, startedAt: Date(timeIntervalSince1970: 1_760_000_000))
        context.insert(m)
        for (i, t) in lines.enumerated() {
            let s = Segment(text: t, start: m.startedAt.addingTimeInterval(Double(i) * 5), duration: 3)
            context.insert(s)
            s.meeting = m
        }
        try? context.save()
        return m
    }

    private func sent(_ state: inout SyncMacState) {
        let e = engine
        e.assignLineIDs()
        let all = e.removals(for: state) + e.changes(for: state)
        e.confirmSent(all, retry: [], state: &state)
    }

    func testNewThingsGoOnceAndATitleChangeGoesAgain() {
        var state = newState()
        let m = recording(lines: ["We acquire Zenith Labs next quarter"])
        engine.assignLineIDs()
        let first = engine.changes(for: state)
        XCTAssertEqual(first.map(\.order), [3, 4])
        engine.confirmSent(first, retry: [], state: &state)
        XCTAssertTrue(engine.changes(for: state).isEmpty)
        XCTAssertTrue(engine.removals(for: state).isEmpty)
        m.title = "Board, renamed"
        let again = engine.changes(for: state)
        XCTAssertEqual(again.count, 1)
        guard case .recording(let r) = again[0] else { return XCTFail() }
        XCTAssertEqual(r.title, "Board, renamed")
        // A retried id stays unsent
        engine.confirmSent(again, retry: [r.id], state: &state)
        XCTAssertEqual(engine.changes(for: state).count, 1)
    }

    func testALocalStrikeTravelsAsMarkerAndKeepListNeverTheWords() async throws {
        var state = newState()
        let m = recording(lines: ["We acquire Zenith Labs next quarter"])
        sent(&state)
        let seg = m.segments[0]
        try await RedactionEngine.strike(.words(seg, 2...3), reason: "privileged", meeting: m, context: context)
        let removals = engine.removals(for: state)
        XCTAssertEqual(removals.map(\.order), [0, 1])
        guard case .strike(let s) = removals[0], case .edit(let e) = removals[1] else { return XCTFail() }
        XCTAssertEqual(s.reason, "privileged")
        XCTAssertEqual(s.line, seg.syncID.map(SyncIDs.wire))
        XCTAssertEqual(e.keep.count, 5)
        XCTAssertEqual(e.keep[2], "m:\(s.id)")
        let bytes = String(decoding: SyncMessage.batch(phase: .removals, items: removals, last: true, upto: nil).encode(), as: UTF8.self)
        XCTAssertFalse(bytes.contains("Zenith"), "struck words never travel")
        engine.confirmSent(removals, retry: [], state: &state)
        XCTAssertTrue(engine.removals(for: state).isEmpty)
    }

    func testAMacStrikeIsAppliedThroughThePurge() async throws {
        var state = newState()
        let m = recording(lines: ["We acquire Zenith Labs next quarter"])
        m.aiNotes = "**Summary**\nPlan: acquire Zenith Labs."
        let ai = MeetingTopic(label: "Zenith deal", confidence: 0.8, source: .ai)
        let mine = MeetingTopic(label: "Acquisitions", confidence: 1, source: .user)
        context.insert(ai); ai.meeting = m
        context.insert(mine); mine.meeting = m
        try context.save()
        sent(&state)
        let seg = m.segments[0]
        let lineID = SyncIDs.wire(seg.syncID!)
        let marker = "00000000-0000-4000-8000-0000000000a1"
        let macText = "We acquire ⟦stricken:\(marker)⟧ next quarter"
        let items: [SyncItem] = [
            .strike(StrikeItem(id: marker, rec: SyncIDs.wire(m.id), target: "words", from: nil, to: nil, created: 1, reason: "legal", line: lineID)),
            .edit(EditItem(id: lineID, rec: SyncIDs.wire(m.id), keep: SyncCrypto.keepList(tokenKey: key, wireText: macText))),
        ]
        let report = await engine.apply(items, state: &state)
        XCTAssertEqual(report.applied, 2, "\(report)")
        XCTAssertEqual(SyncText.toWire(seg.text), macText)
        XCTAssertFalse(m.aiNotes!.contains("Zenith"))
        XCTAssertTrue(m.aiNotesStale)
        XCTAssertEqual(m.topics.map(\.label), ["Acquisitions"], "AI topics go, the user's stay")
        XCTAssertEqual(m.redaction(id: UUID(uuidString: marker)!)?.reason, "legal")
        // Not sent back (only the AI topic the purge removed here, which the
        // Mac's own purge removed too), and the notes count as in step
        let back = engine.removals(for: state)
        XCTAssertEqual(back, [.gone(GoneItem(entity: "topic", id: SyncIDs.wire(ai.id), rec: SyncIDs.wire(m.id)))])
        XCTAssertFalse(engine.changes(for: state).contains { if case .notes = $0 { return true }; return false })
        // Applying it again changes nothing
        let again = await engine.apply([items[1]], state: &state)
        XCTAssertEqual(again.applied, 0)
    }

    func testGonesRemoveForGoodAndLateCopiesAreRefused() async throws {
        var state = newState()
        let m = recording(lines: ["first line", "second line"])
        sent(&state)
        let rec = SyncIDs.wire(m.id)
        let second = SyncIDs.wire(m.orderedSegments[1].syncID!)
        var r = await engine.apply([.gone(GoneItem(entity: "line", id: second, rec: rec))], state: &state)
        XCTAssertEqual(r.applied, 1)
        XCTAssertEqual(m.segments.map(\.text), ["first line"])
        r = await engine.apply([.line(LineItem(id: second, rec: rec, text: "second line", at: 1, dur: nil, speaker: nil, src: nil))], state: &state)
        XCTAssertEqual(m.segments.count, 1, "a deleted line is never re-imported")

        r = await engine.apply([.gone(GoneItem(entity: "recording", id: rec, rec: nil))], state: &state)
        XCTAssertEqual(r.applied, 1)
        XCTAssertTrue(try context.fetch(FetchDescriptor<Meeting>()).isEmpty)
        let late = RecordingItem(id: rec, title: "Back?", started: 1, ended: nil, kind: "meeting", notebook: nil, planned: nil, cal: nil, people: [], modified: .max)
        _ = await engine.apply([.recording(late)], state: &state)
        XCTAssertTrue(try context.fetch(FetchDescriptor<Meeting>()).isEmpty, "deletions win")
    }

    func testLocalDeletionsBecomeGones() throws {
        var state = newState()
        let m = recording(lines: ["keep me", "drop me"])
        let k = MomentMarker(at: m.startedAt, kind: .question)
        context.insert(k); k.meeting = m
        try context.save()
        sent(&state)
        let dropped = SyncIDs.wire(m.orderedSegments[1].syncID!)
        context.delete(m.orderedSegments[1])
        context.delete(k)
        try context.save()
        let gones = engine.removals(for: state).compactMap { item -> String? in
            if case .gone(let g) = item { return "\(g.entity):\(g.id)" }
            return nil
        }
        XCTAssertEqual(Set(gones), ["line:\(dropped)", "mark:\(SyncIDs.wire(k.id))"])
        let all = engine.removals(for: state)
        engine.confirmSent(all, retry: [], state: &state)
        context.delete(m)
        try context.save()
        XCTAssertEqual(engine.removals(for: state), [.gone(GoneItem(entity: "recording", id: SyncIDs.wire(m.id), rec: nil))])
    }

    func testMacRecordsArriveAndLastWriterRules() async throws {
        var state = newState()
        let rec = "7c1e1b2a-3f4d-4e5f-8a9b-0c1d2e3f4a5b"
        let item = RecordingItem(id: rec, title: "Lecture 1", started: 1_760_000_000_000, ended: 1_760_000_600_000, kind: "class",
                                 notebook: "BIO 101", planned: 50, cal: SyncCalendar(event: "evt", location: "Room 4"),
                                 people: [SyncPerson(email: "ana@example.com", name: "Ana", role: "organizer")], modified: 10)
        let r = await engine.apply([
            .recording(item),
            .line(LineItem(id: "00000000-0000-4000-8000-000000000014", rec: rec, text: "Cells divide", at: 1_760_000_001_000, dur: nil, speaker: nil, src: "screen")),
            .notes(NotesItem(rec: rec, md: "**Summary**\nCells.", made: 1_760_000_700_000, stale: false, modified: 11)),
            .mark(MarkItem(id: "00000000-0000-4000-8000-000000000015", rec: rec, at: 1_760_000_100_000, kind: "test", note: "exam", created: 1, modified: 2)),
            .ref(RefItem(id: "00000000-0000-4000-8000-000000000016", rec: rec, url: "https://example.com", title: "Syllabus", note: nil, created: 1, modified: 2)),
            .ref(RefItem(id: "00000000-0000-4000-8000-000000000017", rec: rec, url: "javascript:x", title: nil, note: nil, created: 1, modified: 2)),
            .topic(TopicItem(id: "00000000-0000-4000-8000-000000000018", rec: rec, label: "Mitosis", key: "mitosis", conf: 900, source: "user", created: 1)),
        ], state: &state)
        XCTAssertEqual(r.applied, 6, "\(r)")
        let m = try XCTUnwrap(try context.fetch(FetchDescriptor<Meeting>()).first)
        XCTAssertEqual(m.kind, .class)
        XCTAssertEqual(m.courseName, "BIO 101")
        XCTAssertEqual(m.location, "Room 4")
        XCTAssertEqual(m.people.first?.person.email, "ana@example.com")
        XCTAssertEqual(m.segments.first?.source, Snapshot.Source.screen)
        XCTAssertEqual(m.aiNotes, "**Summary**\nCells.")
        XCTAssertEqual(m.markers.first?.note, "exam")
        XCTAssertEqual(m.references.count, 1)
        XCTAssertTrue(engine.changes(for: state).isEmpty, "nothing echoes back")

        // Unchanged here: the Mac's newer title applies
        var newer = item; newer.title = "Lecture 1 (Mac)"; newer.modified = 20
        _ = await engine.apply([.recording(newer)], state: &state)
        XCTAssertEqual(m.title, "Lecture 1 (Mac)")
        // Changed here too since the last sync: the iPhone's version wins
        m.title = "Mine"
        var other = item; other.title = "Theirs"; other.modified = 30
        _ = await engine.apply([.recording(other)], state: &state)
        XCTAssertEqual(m.title, "Mine")
    }
}
