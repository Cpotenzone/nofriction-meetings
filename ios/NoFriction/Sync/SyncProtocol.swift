import CryptoKit
import Foundation

/// Wire protocol for Sync with your Mac (docs/SYNC.md). Mirrors
/// `src-tauri/src/sync/protocol.rs`; the golden fixtures in
/// `NoFrictionTests/SyncFixtures` (written by the Rust tests) hold the two
/// to the same bytes.
///
/// Frames: 4-byte big-endian length + canonical JSON (sorted keys, no
/// whitespace, `/` not escaped, absent optionals omitted, integers only).
enum SyncWire {
    static let version = 1
    static let maxFrame = 16 << 20
    static let batchMax = 500
    static let serviceType = "_nofriction._tcp"
}

// MARK: - JSON values with a canonical writer

indirect enum JSONValue: Equatable, Sendable {
    case string(String)
    case int(Int64)
    case bool(Bool)
    case array([JSONValue])
    case object([String: JSONValue])

    /// Compact, keys sorted by UTF-8 bytes, escaping exactly like serde_json.
    var canonical: String {
        var out = ""
        write(&out)
        return out
    }

    private func write(_ out: inout String) {
        switch self {
        case .string(let s): JSONValue.writeString(s, &out)
        case .int(let n): out += String(n)
        case .bool(let b): out += b ? "true" : "false"
        case .array(let items):
            out += "["
            for (i, item) in items.enumerated() {
                if i > 0 { out += "," }
                item.write(&out)
            }
            out += "]"
        case .object(let map):
            out += "{"
            let keys = map.keys.sorted { Array($0.utf8).lexicographicallyPrecedes(Array($1.utf8)) }
            for (i, k) in keys.enumerated() {
                if i > 0 { out += "," }
                JSONValue.writeString(k, &out)
                out += ":"
                map[k]!.write(&out)
            }
            out += "}"
        }
    }

    private static func writeString(_ s: String, _ out: inout String) {
        out += "\""
        for scalar in s.unicodeScalars {
            switch scalar {
            case "\"": out += "\\\""
            case "\\": out += "\\\\"
            case "\n": out += "\\n"
            case "\r": out += "\\r"
            case "\t": out += "\\t"
            case "\u{08}": out += "\\b"
            case "\u{0C}": out += "\\f"
            default:
                if scalar.value < 0x20 {
                    out += String(format: "\\u%04x", scalar.value)
                } else {
                    out.unicodeScalars.append(scalar)
                }
            }
        }
        out += "\""
    }

    static func parse(_ data: Data) throws -> JSONValue {
        let any = try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed])
        return try from(any)
    }

    private static func from(_ any: Any) throws -> JSONValue {
        switch any {
        case let s as String: return .string(s)
        case let n as NSNumber:
            if CFGetTypeID(n) == CFBooleanGetTypeID() { return .bool(n.boolValue) }
            return .int(n.int64Value)
        case let a as [Any]: return .array(try a.map(from))
        case let d as [String: Any]: return .object(try d.mapValues(from))
        case is NSNull: throw SyncWireError.malformed("null")
        default: throw SyncWireError.malformed("unknown value")
        }
    }

    // Accessors
    subscript(_ key: String) -> JSONValue? {
        if case .object(let m) = self { return m[key] }
        return nil
    }
    var string: String? { if case .string(let s) = self { return s }; return nil }
    var int: Int64? { if case .int(let n) = self { return n }; return nil }
    var bool: Bool? { if case .bool(let b) = self { return b }; return nil }
    var array: [JSONValue]? { if case .array(let a) = self { return a }; return nil }
}

enum SyncWireError: Error, Equatable, LocalizedError {
    case version(Int64)
    case malformed(String)
    case frameTooLarge

    var errorDescription: String? {
        switch self {
        case .version(let v): "This Mac speaks sync version \(v). Update noFriction on both devices."
        case .malformed(let m): "The Mac sent something unexpected (\(m))."
        case .frameTooLarge: "The Mac sent a message that is too large."
        }
    }
}

/// Builds an object, leaving out nil optionals.
struct JSONObject {
    private(set) var map: [String: JSONValue] = [:]
    mutating func set(_ k: String, _ v: String?) { if let v { map[k] = .string(v) } }
    mutating func set(_ k: String, _ v: Int64?) { if let v { map[k] = .int(v) } }
    mutating func set(_ k: String, _ v: Bool) { map[k] = .bool(v) }
    mutating func set(_ k: String, _ v: JSONValue?) { if let v { map[k] = v } }
    var value: JSONValue { .object(map) }
}

private func req<T>(_ v: T?, _ name: String) throws -> T {
    guard let v else { throw SyncWireError.malformed("missing \(name)") }
    return v
}

// MARK: - Items

struct SyncCalendar: Equatable, Sendable {
    var event: String?
    var start: Int64?
    var end: Int64?
    var location: String?
    var url: String?
    var notes: String?

    var isEmpty: Bool { self == SyncCalendar() }

    var json: JSONValue {
        var o = JSONObject()
        o.set("event", event); o.set("start", start); o.set("end", end)
        o.set("location", location); o.set("url", url); o.set("notes", notes)
        return o.value
    }

    init(event: String? = nil, start: Int64? = nil, end: Int64? = nil, location: String? = nil, url: String? = nil, notes: String? = nil) {
        self.event = event; self.start = start; self.end = end; self.location = location; self.url = url; self.notes = notes
    }

    init(json j: JSONValue) {
        self.init(event: j["event"]?.string, start: j["start"]?.int, end: j["end"]?.int,
                  location: j["location"]?.string, url: j["url"]?.string, notes: j["notes"]?.string)
    }
}

struct SyncPerson: Equatable, Sendable {
    var email: String
    var name: String?
    var role: String
}

struct RecordingItem: Equatable, Sendable {
    var id: String
    var title: String
    var started: Int64
    var ended: Int64?
    var kind: String
    var notebook: String?
    var planned: Int64?
    var cal: SyncCalendar?
    var people: [SyncPerson]
    var modified: Int64
}

struct LineItem: Equatable, Sendable {
    var id: String
    var rec: String
    var text: String
    var at: Int64
    var dur: Int64?
    var speaker: String?
    var src: String?
}

struct EditItem: Equatable, Sendable {
    var id: String
    var rec: String
    var keep: [String]
}

struct StrikeItem: Equatable, Sendable {
    var id: String
    var rec: String
    var target: String
    var from: Int64?
    var to: Int64?
    var created: Int64
    var reason: String?
    var line: String?
}

struct NotesItem: Equatable, Sendable {
    var rec: String
    var md: String
    var made: Int64
    var stale: Bool
    var modified: Int64
}

struct MarkItem: Equatable, Sendable {
    var id: String
    var rec: String
    var at: Int64
    var kind: String
    var note: String?
    var created: Int64
    var modified: Int64
}

struct RefItem: Equatable, Sendable {
    var id: String
    var rec: String
    var url: String
    var title: String?
    var note: String?
    var created: Int64
    var modified: Int64
}

struct TopicItem: Equatable, Sendable {
    var id: String
    var rec: String
    var label: String
    var key: String
    var conf: Int64
    var source: String
    var created: Int64
}

struct GoneItem: Equatable, Sendable {
    var entity: String
    var id: String
    var rec: String?
}

enum SyncItem: Equatable, Sendable {
    case recording(RecordingItem)
    case line(LineItem)
    case edit(EditItem)
    case strike(StrikeItem)
    case notes(NotesItem)
    case mark(MarkItem)
    case ref(RefItem)
    case topic(TopicItem)
    case gone(GoneItem)

    var isRemoval: Bool {
        switch self {
        case .edit, .strike, .gone: true
        default: false
        }
    }

    /// Application order: strikes, edits, gones, then recordings before what hangs off them
    var order: Int {
        switch self {
        case .strike: 0
        case .edit: 1
        case .gone: 2
        case .recording: 3
        case .line: 4
        case .notes: 5
        case .mark: 6
        case .ref: 7
        case .topic: 8
        }
    }

    /// Order for applying a peer's items: edits and gones, then recordings,
    /// then strike records (their recording is there by then), then the rest
    var applyOrder: Int {
        switch self {
        case .edit: 0
        case .gone: 1
        case .recording: 2
        case .strike: 3
        default: order
        }
    }

    /// The id the peer asks for again (`applied.retry`)
    var retryID: String {
        switch self {
        case .recording(let x): x.id
        case .line(let x): x.id
        case .edit(let x): x.id
        case .strike(let x): x.id
        case .notes(let x): x.rec
        case .mark(let x): x.id
        case .ref(let x): x.id
        case .topic(let x): x.id
        case .gone(let x): x.id
        }
    }

    var json: JSONValue {
        var o = JSONObject()
        switch self {
        case .recording(let r):
            o.set("k", "recording"); o.set("id", r.id); o.set("title", r.title); o.set("started", r.started)
            o.set("ended", r.ended); o.set("kind", r.kind); o.set("notebook", r.notebook); o.set("planned", r.planned)
            o.set("cal", r.cal.map(\.json))
            o.set("people", .array(r.people.map { p in
                var po = JSONObject()
                po.set("email", p.email); po.set("name", p.name); po.set("role", p.role)
                return po.value
            }))
            o.set("mod", r.modified)
        case .line(let l):
            o.set("k", "line"); o.set("id", l.id); o.set("rec", l.rec); o.set("text", l.text); o.set("at", l.at)
            o.set("dur", l.dur); o.set("speaker", l.speaker); o.set("src", l.src)
        case .edit(let e):
            o.set("k", "edit"); o.set("id", e.id); o.set("rec", e.rec); o.set("keep", .array(e.keep.map { .string($0) }))
        case .strike(let s):
            o.set("k", "strike"); o.set("id", s.id); o.set("rec", s.rec); o.set("target", s.target)
            o.set("from", s.from); o.set("to", s.to); o.set("created", s.created); o.set("reason", s.reason); o.set("line", s.line)
        case .notes(let n):
            o.set("k", "notes"); o.set("rec", n.rec); o.set("md", n.md); o.set("made", n.made); o.set("stale", n.stale); o.set("mod", n.modified)
        case .mark(let m):
            o.set("k", "mark"); o.set("id", m.id); o.set("rec", m.rec); o.set("at", m.at); o.set("kind", m.kind)
            o.set("note", m.note); o.set("created", m.created); o.set("mod", m.modified)
        case .ref(let r):
            o.set("k", "ref"); o.set("id", r.id); o.set("rec", r.rec); o.set("url", r.url); o.set("title", r.title)
            o.set("note", r.note); o.set("created", r.created); o.set("mod", r.modified)
        case .topic(let t):
            o.set("k", "topic"); o.set("id", t.id); o.set("rec", t.rec); o.set("label", t.label); o.set("key", t.key)
            o.set("conf", t.conf); o.set("source", t.source); o.set("created", t.created)
        case .gone(let g):
            o.set("k", "gone"); o.set("entity", g.entity); o.set("id", g.id); o.set("rec", g.rec)
        }
        return o.value
    }

    init(json j: JSONValue) throws {
        func s(_ k: String) throws -> String { try req(j[k]?.string, k) }
        func i(_ k: String) throws -> Int64 { try req(j[k]?.int, k) }
        switch try s("k") {
        case "recording":
            let people = try (j["people"]?.array ?? []).map { p in
                SyncPerson(email: try req(p["email"]?.string, "email"), name: p["name"]?.string, role: try req(p["role"]?.string, "role"))
            }
            self = .recording(RecordingItem(id: try s("id"), title: try s("title"), started: try i("started"), ended: j["ended"]?.int,
                                            kind: try s("kind"), notebook: j["notebook"]?.string, planned: j["planned"]?.int,
                                            cal: j["cal"].map(SyncCalendar.init(json:)), people: people, modified: try i("mod")))
        case "line":
            self = .line(LineItem(id: try s("id"), rec: try s("rec"), text: try s("text"), at: try i("at"), dur: j["dur"]?.int,
                                  speaker: j["speaker"]?.string, src: j["src"]?.string))
        case "edit":
            self = .edit(EditItem(id: try s("id"), rec: try s("rec"), keep: try req(j["keep"]?.array, "keep").map { try req($0.string, "keep") }))
        case "strike":
            self = .strike(StrikeItem(id: try s("id"), rec: try s("rec"), target: try s("target"), from: j["from"]?.int, to: j["to"]?.int,
                                      created: try i("created"), reason: j["reason"]?.string, line: j["line"]?.string))
        case "notes":
            self = .notes(NotesItem(rec: try s("rec"), md: try s("md"), made: try i("made"), stale: try req(j["stale"]?.bool, "stale"), modified: try i("mod")))
        case "mark":
            self = .mark(MarkItem(id: try s("id"), rec: try s("rec"), at: try i("at"), kind: try s("kind"), note: j["note"]?.string,
                                  created: try i("created"), modified: try i("mod")))
        case "ref":
            self = .ref(RefItem(id: try s("id"), rec: try s("rec"), url: try s("url"), title: j["title"]?.string, note: j["note"]?.string,
                                created: try i("created"), modified: try i("mod")))
        case "topic":
            self = .topic(TopicItem(id: try s("id"), rec: try s("rec"), label: try s("label"), key: try s("key"), conf: try i("conf"),
                                    source: try s("source"), created: try i("created")))
        case "gone":
            self = .gone(GoneItem(entity: try s("entity"), id: try s("id"), rec: j["rec"]?.string))
        case let other:
            throw SyncWireError.malformed("item \(other)")
        }
    }
}

// MARK: - Messages

enum SyncPhase: String, Sendable { case removals, changes }

enum SyncMessage: Equatable, Sendable {
    case pair(code: String, deviceID: String, name: String)
    case paired(deviceID: String, name: String, secret: String)
    case hello(deviceID: String, nonce: String)
    case challenge(nonce: String, proof: String)
    case auth(proof: String)
    case welcome(deviceID: String, name: String)
    case batch(phase: SyncPhase, items: [SyncItem], last: Bool, upto: Int64?)
    case pull(since: Int64)
    case applied(retry: [String])
    case done
    case error(code: String, message: String)

    var json: JSONValue {
        var o = JSONObject()
        o.set("v", Int64(SyncWire.version))
        switch self {
        case .pair(let code, let id, let name):
            o.set("t", "pair"); o.set("code", code); o.set("device_id", id); o.set("name", name)
        case .paired(let id, let name, let secret):
            o.set("t", "paired"); o.set("device_id", id); o.set("name", name); o.set("secret", secret)
        case .hello(let id, let nonce):
            o.set("t", "hello"); o.set("device_id", id); o.set("nonce", nonce)
        case .challenge(let nonce, let proof):
            o.set("t", "challenge"); o.set("nonce", nonce); o.set("proof", proof)
        case .auth(let proof):
            o.set("t", "auth"); o.set("proof", proof)
        case .welcome(let id, let name):
            o.set("t", "welcome"); o.set("device_id", id); o.set("name", name)
        case .batch(let phase, let items, let last, let upto):
            o.set("t", "batch"); o.set("phase", phase.rawValue); o.set("items", .array(items.map(\.json)))
            o.set("last", last); o.set("upto", upto)
        case .pull(let since):
            o.set("t", "pull"); o.set("since", since)
        case .applied(let retry):
            o.set("t", "applied")
            if !retry.isEmpty { o.set("retry", .array(retry.map { .string($0) })) }
        case .done:
            o.set("t", "done")
        case .error(let code, let message):
            o.set("t", "error"); o.set("code", code); o.set("message", message)
        }
        return o.value
    }

    func encode() -> Data { Data(json.canonical.utf8) }

    func frame() -> Data {
        let body = encode()
        var len = UInt32(body.count).bigEndian
        var out = Data(bytes: &len, count: 4)
        out.append(body)
        return out
    }

    static func decode(_ data: Data) throws -> SyncMessage {
        let j = try JSONValue.parse(data)
        let v = j["v"]?.int ?? 0
        guard v == Int64(SyncWire.version) else { throw SyncWireError.version(v) }
        func s(_ k: String) throws -> String { try req(j[k]?.string, k) }
        switch try s("t") {
        case "pair": return .pair(code: try s("code"), deviceID: try s("device_id"), name: try s("name"))
        case "paired": return .paired(deviceID: try s("device_id"), name: try s("name"), secret: try s("secret"))
        case "hello": return .hello(deviceID: try s("device_id"), nonce: try s("nonce"))
        case "challenge": return .challenge(nonce: try s("nonce"), proof: try s("proof"))
        case "auth": return .auth(proof: try s("proof"))
        case "welcome": return .welcome(deviceID: try s("device_id"), name: try s("name"))
        case "batch":
            guard let phase = SyncPhase(rawValue: try s("phase")) else { throw SyncWireError.malformed("phase") }
            return .batch(phase: phase, items: try req(j["items"]?.array, "items").map(SyncItem.init(json:)),
                          last: try req(j["last"]?.bool, "last"), upto: j["upto"]?.int)
        case "pull": return .pull(since: try req(j["since"]?.int, "since"))
        case "applied": return .applied(retry: (j["retry"]?.array ?? []).compactMap(\.string))
        case "done": return .done
        case "error": return .error(code: try s("code"), message: try s("message"))
        case let t: throw SyncWireError.malformed("message \(t)")
        }
    }

    /// Items split into batches of at most 500; always at least one.
    static func batches(_ items: [SyncItem]) -> [[SyncItem]] {
        guard !items.isEmpty else { return [[]] }
        return stride(from: 0, to: items.count, by: SyncWire.batchMax).map {
            Array(items[$0..<min($0 + SyncWire.batchMax, items.count)])
        }
    }
}

// MARK: - Ids, markers, tokens, proofs

enum SyncIDs {
    /// Any UUID spelling → lowercase hyphenated (the wire form)
    static func wire(_ s: String) -> String? {
        let t = s.trimmingCharacters(in: .whitespaces)
        if let u = UUID(uuidString: t) { return u.uuidString.lowercased() }
        let hex = t.lowercased()
        guard hex.count == 32, hex.allSatisfy(\.isHexDigit) else { return nil }
        let a = Array(hex)
        let parts = [a[0..<8], a[8..<12], a[12..<16], a[16..<20], a[20..<32]].map { String($0) }
        return UUID(uuidString: parts.joined(separator: "-")).map { $0.uuidString.lowercased() }
    }

    static func wire(_ u: UUID) -> String { u.uuidString.lowercased() }
    static func uuid(_ wire: String) -> UUID? { self.wire(wire).flatMap(UUID.init(uuidString:)) }

    static func ms(_ d: Date) -> Int64 { Int64((d.timeIntervalSince1970 * 1000).rounded()) }
    static func date(_ ms: Int64) -> Date { Date(timeIntervalSince1970: Double(ms) / 1000) }
}

enum SyncText {
    private static let localMarker = try! NSRegularExpression(pattern: "⟦stricken:([0-9A-Fa-f-]{36})⟧")

    static func wireMarker(_ id: String) -> String { "⟦stricken:\(id)⟧" }

    /// iPhone line text → wire (markers lowercase)
    static func toWire(_ text: String) -> String {
        let ns = text as NSString
        var out = text
        for m in localMarker.matches(in: text, range: NSRange(location: 0, length: ns.length)).reversed() {
            guard let id = SyncIDs.wire(ns.substring(with: m.range(at: 1))),
                  let r = Range(m.range, in: out) else { continue }
            out.replaceSubrange(r, with: wireMarker(id))
        }
        return out
    }

    /// Wire line text → iPhone (`RedactionText.markerToken` form)
    static func fromWire(_ text: String) -> String {
        let ns = text as NSString
        var out = text
        for m in localMarker.matches(in: text, range: NSRange(location: 0, length: ns.length)).reversed() {
            guard let u = SyncIDs.uuid(ns.substring(with: m.range(at: 1))), let r = Range(m.range, in: out) else { continue }
            out.replaceSubrange(r, with: RedactionText.markerToken(u))
        }
        return out
    }

    /// The wire id if this whitespace token is exactly one marker
    static func markerID(_ token: String) -> String? {
        let ns = token as NSString
        guard let m = localMarker.firstMatch(in: token, range: NSRange(location: 0, length: ns.length)),
              m.range.location == 0, m.range.length == ns.length else { return nil }
        return SyncIDs.wire(ns.substring(with: m.range(at: 1)))
    }

    enum Tok: Equatable {
        case word(String, NSRange)
        case marker(String, NSRange)

        var range: NSRange {
            switch self {
            case .word(_, let r), .marker(_, let r): r
            }
        }
    }

    /// Whitespace (Unicode White_Space) separated tokens with UTF-16 ranges;
    /// a token that is exactly one marker is a marker. Same as the Mac.
    static func tokens(_ text: String) -> [Tok] {
        var out: [Tok] = []
        var cur = ""
        var start = 0
        var pos = 0
        func flush() {
            guard !cur.isEmpty else { return }
            let r = NSRange(location: start, length: pos - start)
            if let id = markerID(cur) { out.append(.marker(id, r)) } else { out.append(.word(cur, r)) }
            cur = ""
        }
        for scalar in text.unicodeScalars {
            if scalar.properties.isWhitespace {
                flush()
            } else {
                if cur.isEmpty { start = pos }
                cur.unicodeScalars.append(scalar)
            }
            pos += scalar.utf16.count
        }
        flush()
        return out
    }
}

enum SyncCrypto {
    static func hmac(_ key: some ContiguousBytes, _ parts: [Data]) -> Data {
        var h = HMAC<SHA256>(key: SymmetricKey(data: key))
        for p in parts { h.update(data: p) }
        return Data(h.finalize())
    }

    static func tokenKey(secret: Data) -> Data { hmac(secret, [Data("nfsync-v1 tokens".utf8)]) }

    static func wordHash(tokenKey: Data, _ word: String) -> String {
        hmac(tokenKey, [Data(word.utf8)]).prefix(8).map { String(format: "%02x", $0) }.joined()
    }

    /// `keep` for an edit: the line as it now is, words hashed
    static func keepList(tokenKey: Data, wireText: String) -> [String] {
        SyncText.tokens(wireText).map { t in
            switch t {
            case .word(let w, _): "w:" + wordHash(tokenKey: tokenKey, w)
            case .marker(let id, _): "m:" + id
            }
        }
    }

    static func macProof(secret: Data, noncePhone: Data, nonceMac: Data) -> Data {
        hmac(secret, [Data("nfsync-v1 mac".utf8), noncePhone, nonceMac])
    }

    static func phoneProof(secret: Data, nonceMac: Data, noncePhone: Data) -> Data {
        hmac(secret, [Data("nfsync-v1 phone".utf8), nonceMac, noncePhone])
    }

    /// Constant-time comparison
    static func equal(_ a: Data, _ b: Data) -> Bool {
        guard a.count == b.count else { return false }
        return zip(a, b).reduce(UInt8(0)) { $0 | ($1.0 ^ $1.1) } == 0
    }

    static func random(_ n: Int) -> Data {
        var bytes = [UInt8](repeating: 0, count: n)
        _ = SecRandomCopyBytes(kSecRandomDefault, n, &bytes)
        return Data(bytes)
    }

    static func sha256Hex(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

// MARK: - Pairing link

/// `nfsync:1?id=…&n=…&fp=…&h=…&p=…&c=…` from the Mac's QR code
struct PairingLink: Equatable, Sendable {
    var macID: String
    var name: String
    var fingerprint: String
    var hosts: [String]
    var port: UInt16
    var code: String

    init?(_ raw: String) {
        let s = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard s.hasPrefix("nfsync:1?"), let comps = URLComponents(string: "nfsync://x?" + s.dropFirst("nfsync:1?".count)) else { return nil }
        var q: [String: String] = [:]
        for item in comps.queryItems ?? [] { q[item.name] = item.value ?? "" }
        guard let id = q["id"].flatMap(SyncIDs.wire), let fp = q["fp"]?.lowercased(), fp.count == 64, fp.allSatisfy(\.isHexDigit),
              let port = q["p"].flatMap(UInt16.init), let code = q["c"], !code.isEmpty else { return nil }
        macID = id
        name = q["n"].flatMap { $0.isEmpty ? nil : $0 } ?? "Mac"
        fingerprint = fp
        hosts = (q["h"] ?? "").split(separator: ",").map(String.init).filter { !$0.isEmpty }
        self.port = port
        self.code = code
    }
}
