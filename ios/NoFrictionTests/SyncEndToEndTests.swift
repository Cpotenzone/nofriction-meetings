import SwiftData
import XCTest
@testable import noFriction

/// End to end against a real Mac server over the real network stack:
/// `cargo run --example sync_test_server -- <dir>` in src-tauri (temp
/// directory, in-memory secrets), then run this test with
/// `TEST_RUNNER_NF_SYNC_LINK_FILE=<dir>/link.txt`. Skipped otherwise.
@MainActor
final class SyncEndToEndTests: XCTestCase {
    func testPairAndTwoSessionsAgainstTheMacServer() async throws {
        guard let path = ProcessInfo.processInfo.environment["NF_SYNC_LINK_FILE"] else {
            throw XCTSkip("No Mac test server (set TEST_RUNNER_NF_SYNC_LINK_FILE)")
        }
        var raw: String?
        for _ in 0..<100 {
            raw = try? String(contentsOfFile: path, encoding: .utf8)
            if raw != nil { break }
            try await Task.sleep(for: .milliseconds(200))
        }
        let link = try XCTUnwrap(PairingLink(try XCTUnwrap(raw)))
        let dir = FileManager.default.temporaryDirectory.appending(path: "nf-sync-e2e-\(UUID().uuidString)")
        SyncLedger.directory = dir
        defer {
            SyncStore.forget(link.macID)
            try? FileManager.default.removeItem(at: dir)
        }

        let container = try ModelContainer(for: Schema(Storage.modelTypes), configurations: ModelConfiguration(isStoredInMemoryOnly: true))
        let context = container.mainContext
        let m = Meeting(title: "iPhone lecture", startedAt: Date(timeIntervalSince1970: 1_760_100_000))
        context.insert(m)
        for (i, t) in ["The exam is on Tuesday at nine", "Please delete me later"].enumerated() {
            let s = Segment(text: t, start: m.startedAt.addingTimeInterval(Double(i) * 4), duration: 3)
            context.insert(s)
            s.meeting = m
        }
        let mark = MomentMarker(at: m.startedAt, kind: .test, note: "exam")
        context.insert(mark)
        mark.meeting = m
        m.aiNotes = "**Summary**\nExam on Tuesday."
        try context.save()

        let connect: (SyncMacState) async throws -> SyncConnection = { state in
            let c = SyncConnection(endpoint: .hostPort(host: "127.0.0.1", port: .init(rawValue: state.port)!), fingerprint: state.fingerprint)
            try await c.start()
            return c
        }
        // A wrong pin never gets past TLS
        let wrong = SyncConnection(endpoint: .hostPort(host: "127.0.0.1", port: .init(rawValue: link.port)!),
                                   fingerprint: String(repeating: "0", count: 64))
        do {
            try await wrong.start(timeout: 4)
            XCTFail("connected with the wrong pin")
        } catch {}
        wrong.cancel()

        var state = try await SyncSession.pair(link, deviceName: "Simulator", connect: connect)
        XCTAssertNotNil(SyncStore.secret(for: link.macID))
        let engine = { (key: Data) in SyncEngine(context: context, tokenKey: key) }

        // Session 1: each side gets the other's recording
        let s1 = try await SyncSession.run(&state, engine: engine, connect: connect)
        XCTAssertTrue(s1.errors.isEmpty, "\(s1.errors)")
        let mac = try XCTUnwrap(try context.fetch(FetchDescriptor<Meeting>()).first { $0.title == "Mac planning meeting" })
        XCTAssertEqual(mac.segments.count, 2)
        XCTAssertEqual(mac.aiNotes, "**Summary**\nPlan: acquire Zenith Labs.")

        // Here: strike "Zenith Labs" in the Mac's line, delete our second line
        try await Task.sleep(for: .seconds(2))   // the server deletes "Tuesday" after session 1
        let zenith = try XCTUnwrap(mac.segments.first { $0.text.contains("Zenith") })
        try await RedactionEngine.strike(.words(zenith, 2...3), reason: "privileged", meeting: mac, context: context)
        let gone = try XCTUnwrap(m.segments.first { $0.text.contains("delete me") })
        let pending = try RedactionEngine.delete(.lines([gone]), meeting: m, context: context)
        try await RedactionEngine.commit(pending, context: context)

        // Session 2: removals cross both ways (state as the app reloads it)
        state = try XCTUnwrap(SyncStore.load(link.macID))
        let s2 = try await SyncSession.run(&state, engine: engine, connect: connect)
        XCTAssertTrue(s2.errors.isEmpty, "\(s2.errors)")
        XCTAssertEqual(m.segments.map(\.text), ["The exam is on at nine"], "the Mac's Delete reached the iPhone")
        XCTAssertFalse(m.aiNotes?.contains("Tuesday") ?? true, "and was purged from the notes here")
        XCTAssertFalse(mac.aiNotes?.contains("Zenith") ?? true)

        // What the Mac ended with (the server writes it after session 2)
        let resultURL = URL(filePath: path).deletingLastPathComponent().appending(path: "result.json")
        var result: JSONValue?
        for _ in 0..<50 {
            if let d = try? Data(contentsOf: resultURL) { result = try JSONValue.parse(d); break }
            try await Task.sleep(for: .milliseconds(200))
        }
        let r = try XCTUnwrap(result, "the server's result")
        XCTAssertEqual(r["zenith_in_search"]?.int, 0, "struck on the iPhone: gone from the Mac's search index")
        XCTAssertEqual(r["zenith_in_notes"]?.int, 0, "and from its notes")
        XCTAssertEqual(r["strikes"]?.int, 1)
        XCTAssertEqual(r["deleted_phone_line_present"]?.int, 0, "deleted on the iPhone: gone on the Mac")
        XCTAssertEqual(r["phone_notes"]?.int, 1, "the Mac kept its own notes for its recording")
        XCTAssertEqual(r["marks"]?.int, 1)
    }
}
