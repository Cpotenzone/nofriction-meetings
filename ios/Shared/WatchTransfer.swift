import Foundation

// Compiled into both the iPhone app and the Apple Watch app: the contract
// for a recording made on the watch and imported on the phone
// (docs/WATCH_APP.md). Pure Foundation; nothing here touches audio,
// transcript text or the network.

/// How the watch records, and the keys of the transfer metadata.
enum WatchTransfer {
    /// Bump only for an incompatible change. The phone imports any version
    /// whose required fields decode, so additive changes keep `1`.
    static let metadataVersion = 1

    /// AAC, mono, 16 kHz: the rate speech recognition works at, about
    /// 14 MB per hour at 32 kbit/s, so an hour-long meeting transfers in minutes.
    static let sampleRate: Double = 16_000
    static let channels = 1
    static let bitRate = 32_000
    static let fileExtension = "m4a"

    /// The watch app's bundle id (embedded in the iPhone app under Watch/).
    static let watchBundleID = "com.nofriction.meetings.watchkitapp"

    enum Key {
        static let recordingID = "recordingId"
        static let startedAt = "startedAt"
        static let endedAt = "endedAt"
        static let duration = "duration"
        static let appVersion = "appVersion"
        static let pauses = "pauses"
        static let part = "part"
        static let partCount = "parts"
        static let version = "v"
        /// iPhone → watch user info: `["ack": ["<recordingId>#<part>", …]]`,
        /// sent once a part is stored on the iPhone. Only then does the
        /// watch delete its copy.
        static let ack = "ack"

        // Optional, added with recording types (all additive: an iPhone app
        // without them ignores them, and a watch app without them still
        // imports, as a meeting with no notebook, limit or markers).
        /// "What is it?": `meeting` / `class` / `personal`
        static let kind = "kind"
        /// The notebook picked on the watch (one of the iPhone's recent notebooks)
        static let notebook = "notebook"
        /// "How long?" in minutes when the recording ended (after any +15 min);
        /// absent = no limit
        static let plannedMinutes = "plannedMinutes"
        /// Moments marked on the watch: `[["id": uuid, "kind": "important" |
        /// "question" | "test", "at": Date, "offset": seconds]]`. Every part
        /// carries the list; the iPhone merges them by id.
        static let markers = "markers"
        /// iPhone → watch application context (latest wins):
        /// `["recentNotebooks": ["Acme project", "BIO 101", …]]`. Names only.
        static let recentNotebooks = "recentNotebooks"
    }

    /// Markers per recording (each one is a few dozen bytes of metadata)
    static let maxMarkers = 500
    /// Notebook names the iPhone sends the watch
    static let maxRecentNotebooks = 8

    /// The application context the iPhone sends: the recent notebook names
    /// (cleaned, one per name ignoring case, at most `maxRecentNotebooks`)
    /// and nothing else.
    static func notebookContext(_ names: [String]) -> [String: Any] {
        [Key.recentNotebooks: cleanNotebooks(names)]
    }

    /// The names in a received application context (cleaned again: the
    /// watch never trusts the shape of what arrives).
    static func notebooks(fromContext context: [String: Any]) -> [String] {
        cleanNotebooks((context[Key.recentNotebooks] as? [Any])?.compactMap { $0 as? String } ?? [])
    }

    private static func cleanNotebooks(_ names: [String]) -> [String] {
        var seen = Set<String>()
        var out: [String] = []
        for case let name? in names.map(Notebook.normalize) where seen.insert(name.lowercased()).inserted {
            out.append(name)
            if out.count == maxRecentNotebooks { break }
        }
        return out
    }

    /// Identifies one part of one recording ("<uuid>#<part>"), in acks and transfer bookkeeping.
    static func partKey(_ id: UUID, _ part: Int) -> String { "\(id.uuidString)#\(part)" }

    static func parsePartKey(_ key: String) -> (id: UUID, part: Int)? {
        let pieces = key.split(separator: "#")
        guard pieces.count == 2, let id = UUID(uuidString: String(pieces[0])), let part = Int(pieces[1]) else { return nil }
        return (id, part)
    }

    /// Parts a recording may be split into (one per pause, plus one)
    static let maxParts = 500
}

/// Sent with every file in `WCSession.transferFile(_:metadata:)`. Times,
/// ids, the recording's type, the notebook name the user picked, the
/// planned length and marker times; never transcript or audio content.
struct WatchRecordingMetadata: Codable, Equatable, Sendable {
    /// One per recording, made on the watch. The phone imports each id once.
    var recordingID: UUID
    /// Wall-clock start and end of the recording (pauses included)
    var startedAt: Date
    var endedAt: Date
    /// Seconds of audio in the file (pauses excluded)
    var duration: TimeInterval
    /// Watch app version that made the recording ("1.0.0 (4)")
    var appVersion: String
    /// Where the recording was paused, so file time maps back to wall-clock time.
    var pauses: [Pause] = []
    /// The watch closes the audio file at every pause (so a paused recording
    /// survives the app being ended) and continues in a new one: the
    /// recording arrives as `partCount` files, this one being `part`
    /// (0-based). The phone joins them in order. Times above are for the
    /// whole recording.
    var part: Int = 0
    var partCount: Int = 1
    var version: Int = WatchTransfer.metadataVersion
    /// What the recording is; nil from a watch app before recording types
    /// (the iPhone then treats it as a meeting).
    var kind: RecordingKind?
    /// The notebook picked on the watch; nil = none.
    var notebook: String?
    /// "How long?" in minutes when it ended; nil = no limit (or an older watch app).
    var plannedMinutes: Int?
    /// Moments marked on the watch, in time order.
    var markers: [WatchMarker] = []

    struct Pause: Codable, Equatable, Sendable {
        /// Seconds into the audio file where the pause happened
        var at: Double
        /// How long it lasted (wall-clock seconds)
        var length: Double
    }

    enum CodingKeys: String, CodingKey {
        case recordingID, startedAt, endedAt, duration, appVersion, pauses, part, partCount, version
        case kind, notebook, plannedMinutes, markers
    }

    /// Fields added after the first version decode with their defaults.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        recordingID = try c.decode(UUID.self, forKey: .recordingID)
        startedAt = try c.decode(Date.self, forKey: .startedAt)
        endedAt = try c.decode(Date.self, forKey: .endedAt)
        duration = try c.decode(Double.self, forKey: .duration)
        appVersion = try c.decodeIfPresent(String.self, forKey: .appVersion) ?? "unknown"
        pauses = try c.decodeIfPresent([Pause].self, forKey: .pauses) ?? []
        part = try c.decodeIfPresent(Int.self, forKey: .part) ?? 0
        partCount = try c.decodeIfPresent(Int.self, forKey: .partCount) ?? 1
        version = try c.decodeIfPresent(Int.self, forKey: .version) ?? WatchTransfer.metadataVersion
        // Never fail a recording over these: a bad value is dropped
        let kind: String? = try? c.decodeIfPresent(String.self, forKey: .kind)
        self.kind = kind.flatMap(RecordingKind.init(rawValue:))
        let notebook: String? = try? c.decodeIfPresent(String.self, forKey: .notebook)
        self.notebook = Notebook.normalize(notebook)
        let planned: Int? = try? c.decodeIfPresent(Int.self, forKey: .plannedMinutes)
        plannedMinutes = Self.validMinutes(planned.map(Double.init))
        let markers: [WatchMarker]? = try? c.decodeIfPresent([WatchMarker].self, forKey: .markers)
        self.markers = WatchMarker.merged([markers ?? []])
    }

    /// Wall-clock time of a position in the audio file: the start, plus the
    /// audio before it, plus every pause that happened before it.
    func wallClock(atFileOffset t: Double) -> Date {
        let paused = pauses.filter { $0.at <= t }.reduce(0) { $0 + max(0, $1.length) }
        return startedAt.addingTimeInterval(max(0, t) + paused)
    }

    // MARK: Property-list form (WCSession metadata allows only plist types)

    /// The first-version keys keep their names and types. The optional keys
    /// are added only when set (a property list has no null).
    var dictionary: [String: Any] {
        var d: [String: Any] = [
            WatchTransfer.Key.recordingID: recordingID.uuidString,
            WatchTransfer.Key.startedAt: startedAt,
            WatchTransfer.Key.endedAt: endedAt,
            WatchTransfer.Key.duration: duration,
            WatchTransfer.Key.appVersion: appVersion,
            WatchTransfer.Key.pauses: pauses.map { [$0.at, $0.length] },
            WatchTransfer.Key.part: part,
            WatchTransfer.Key.partCount: partCount,
            WatchTransfer.Key.version: version,
        ]
        if let kind { d[WatchTransfer.Key.kind] = kind.rawValue }
        if let notebook { d[WatchTransfer.Key.notebook] = notebook }
        if let plannedMinutes { d[WatchTransfer.Key.plannedMinutes] = plannedMinutes }
        if !markers.isEmpty { d[WatchTransfer.Key.markers] = markers.map(\.dictionary) }
        return d
    }

    init(recordingID: UUID, startedAt: Date, endedAt: Date, duration: TimeInterval,
         appVersion: String, pauses: [Pause] = [], part: Int = 0, partCount: Int = 1,
         version: Int = WatchTransfer.metadataVersion, kind: RecordingKind? = nil,
         notebook: String? = nil, plannedMinutes: Int? = nil, markers: [WatchMarker] = []) {
        self.recordingID = recordingID
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.duration = duration
        self.appVersion = appVersion
        self.pauses = pauses
        self.part = part
        self.partCount = partCount
        self.version = version
        self.kind = kind
        self.notebook = notebook
        self.plannedMinutes = plannedMinutes
        self.markers = markers
    }

    /// The same recording, labeled as one of its parts.
    func forPart(_ index: Int, of count: Int) -> WatchRecordingMetadata {
        var copy = self
        copy.part = index
        copy.partCount = count
        return copy
    }

    /// nil when a required field is missing or out of range. Unknown keys are ignored.
    init?(dictionary d: [String: Any]) {
        guard let idString = d[WatchTransfer.Key.recordingID] as? String,
              let id = UUID(uuidString: idString),
              let start = Self.date(d[WatchTransfer.Key.startedAt]),
              let end = Self.date(d[WatchTransfer.Key.endedAt]),
              let duration = Self.number(d[WatchTransfer.Key.duration])
        else { return nil }
        let pauses = ((d[WatchTransfer.Key.pauses] as? [Any]) ?? []).compactMap { item -> Pause? in
            guard let pair = item as? [Any], pair.count == 2,
                  let at = Self.number(pair[0]), let length = Self.number(pair[1]) else { return nil }
            return Pause(at: at, length: length)
        }
        self.init(recordingID: id, startedAt: start, endedAt: end, duration: duration,
                  appVersion: (d[WatchTransfer.Key.appVersion] as? String) ?? "unknown",
                  pauses: pauses,
                  part: Self.number(d[WatchTransfer.Key.part]).map { Int($0) } ?? 0,
                  partCount: Self.number(d[WatchTransfer.Key.partCount]).map { Int($0) } ?? 1,
                  version: (d[WatchTransfer.Key.version] as? Int) ?? WatchTransfer.metadataVersion,
                  // Optional keys: a missing or invalid value is dropped, never the recording
                  kind: (d[WatchTransfer.Key.kind] as? String).flatMap(RecordingKind.init(rawValue:)),
                  notebook: Notebook.normalize(d[WatchTransfer.Key.notebook] as? String),
                  plannedMinutes: Self.validMinutes(Self.number(d[WatchTransfer.Key.plannedMinutes])),
                  markers: WatchMarker.merged([((d[WatchTransfer.Key.markers] as? [Any]) ?? []).compactMap {
                      ($0 as? [String: Any]).flatMap(WatchMarker.init(dictionary:))
                  }]))
        guard isValid else { return nil }
    }

    /// 1 minute … 12 hours, whole minutes; anything else reads as no limit.
    private static func validMinutes(_ value: Double?) -> Int? {
        guard let value, value.isFinite, value >= 1, value <= Double(RecordingLimit.maxMinutes) else { return nil }
        return Int(value.rounded())
    }

    /// Sane values: ends no earlier than it starts, finite non-negative audio
    /// length, pauses with finite non-negative times, a real part number.
    /// (No upper limit on length: a recording paused overnight is still valid.)
    var isValid: Bool {
        guard duration.isFinite, duration >= 0, endedAt >= startedAt,
              partCount >= 1, partCount <= WatchTransfer.maxParts, part >= 0, part < partCount else { return false }
        return pauses.allSatisfy { $0.at.isFinite && $0.length.isFinite && $0.at >= 0 && $0.length >= 0 }
    }

    fileprivate static func date(_ value: Any?) -> Date? {
        if let d = value as? Date { return d }
        if let n = value as? Double, n.isFinite { return Date(timeIntervalSince1970: n) }
        return nil
    }

    fileprivate static func number(_ value: Any?) -> Double? {
        if let n = value as? Double { return n.isFinite ? n : nil }
        if let n = value as? Int { return Double(n) }
        if let n = value as? NSNumber { return n.doubleValue.isFinite ? n.doubleValue : nil }
        return nil
    }
}

/// A moment marked on the watch (★ Important, ? Question, ✎ the third kind).
/// `at` is wall-clock time on the watch's clock, the clock of the
/// recording's `startedAt`, so it lands on the right transcript line however
/// the parts are joined. `offset` is the seconds of audio before it (pauses
/// excluded); `WatchRecordingMetadata.wallClock(atFileOffset:)` maps it back
/// to `at`. Times and a kind only: there are no notes on the watch.
struct WatchMarker: Codable, Equatable, Sendable, Identifiable {
    var id: UUID
    var kind: MarkerKind
    var at: Date
    var offset: Double?

    enum Key {
        static let id = "id"
        static let kind = "kind"
        static let at = "at"
        static let offset = "offset"
    }

    init(id: UUID = UUID(), kind: MarkerKind = .default, at: Date, offset: Double? = nil) {
        self.id = id
        self.kind = kind
        self.at = at
        self.offset = offset
    }

    enum CodingKeys: String, CodingKey { case id, kind, at, offset }

    /// A kind from a newer build reads as ★ Important (the moment is kept).
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        at = try c.decode(Date.self, forKey: .at)
        let kind: String? = try? c.decodeIfPresent(String.self, forKey: .kind)
        self.kind = kind.flatMap(MarkerKind.init(rawValue:)) ?? .default
        let offset: Double? = try? c.decodeIfPresent(Double.self, forKey: .offset)
        self.offset = offset.flatMap { $0.isFinite && $0 >= 0 ? $0 : nil }
    }

    var dictionary: [String: Any] {
        var d: [String: Any] = [Key.id: id.uuidString, Key.kind: kind.rawValue, Key.at: at]
        if let offset { d[Key.offset] = offset }
        return d
    }

    /// nil without a valid id and time.
    init?(dictionary d: [String: Any]) {
        guard let id = (d[Key.id] as? String).flatMap(UUID.init(uuidString:)),
              let at = WatchRecordingMetadata.date(d[Key.at]) else { return nil }
        let offset = WatchRecordingMetadata.number(d[Key.offset]).flatMap { $0 >= 0 ? $0 : nil }
        self.init(id: id, kind: (d[Key.kind] as? String).flatMap(MarkerKind.init(rawValue:)) ?? .default,
                  at: at, offset: offset)
    }

    /// The markers of every part as one list: one per id (the first seen),
    /// in time order, at most `WatchTransfer.maxMarkers`.
    static func merged(_ lists: [[WatchMarker]]) -> [WatchMarker] {
        var seen = Set<UUID>()
        var out: [WatchMarker] = []
        for m in lists.joined() where seen.insert(m.id).inserted { out.append(m) }
        return Array(out.sorted { ($0.at, $0.id.uuidString) < ($1.at, $1.id.uuidString) }.prefix(WatchTransfer.maxMarkers))
    }
}

/// Text shown before the first recording, on the phone and on the watch.
enum RecordingNotice {
    static let text = "Recording laws differ — in many places everyone in the conversation must agree to be recorded. Tell participants you're recording."
}

/// "1:15", "1:02:05"
enum ClockText {
    static func format(_ seconds: TimeInterval) -> String {
        let s = Int(max(0, seconds).rounded(.down))
        return s >= 3600
            ? String(format: "%d:%02d:%02d", s / 3600, (s % 3600) / 60, s % 60)
            : String(format: "%d:%02d", s / 60, s % 60)
    }
}
