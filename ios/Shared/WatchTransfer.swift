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

/// Sent with every file in `WCSession.transferFile(_:metadata:)`. Times and
/// ids only; never transcript or audio content.
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

    struct Pause: Codable, Equatable, Sendable {
        /// Seconds into the audio file where the pause happened
        var at: Double
        /// How long it lasted (wall-clock seconds)
        var length: Double
    }

    enum CodingKeys: String, CodingKey {
        case recordingID, startedAt, endedAt, duration, appVersion, pauses, part, partCount, version
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
    }

    /// Wall-clock time of a position in the audio file: the start, plus the
    /// audio before it, plus every pause that happened before it.
    func wallClock(atFileOffset t: Double) -> Date {
        let paused = pauses.filter { $0.at <= t }.reduce(0) { $0 + max(0, $1.length) }
        return startedAt.addingTimeInterval(max(0, t) + paused)
    }

    // MARK: Property-list form (WCSession metadata allows only plist types)

    var dictionary: [String: Any] {
        [
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
    }

    init(recordingID: UUID, startedAt: Date, endedAt: Date, duration: TimeInterval,
         appVersion: String, pauses: [Pause] = [], part: Int = 0, partCount: Int = 1,
         version: Int = WatchTransfer.metadataVersion) {
        self.recordingID = recordingID
        self.startedAt = startedAt
        self.endedAt = endedAt
        self.duration = duration
        self.appVersion = appVersion
        self.pauses = pauses
        self.part = part
        self.partCount = partCount
        self.version = version
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
                  version: (d[WatchTransfer.Key.version] as? Int) ?? WatchTransfer.metadataVersion)
        guard isValid else { return nil }
    }

    /// Sane values: ends no earlier than it starts, finite non-negative audio
    /// length, pauses with finite non-negative times, a real part number.
    /// (No upper limit on length: a recording paused overnight is still valid.)
    var isValid: Bool {
        guard duration.isFinite, duration >= 0, endedAt >= startedAt,
              partCount >= 1, partCount <= WatchTransfer.maxParts, part >= 0, part < partCount else { return false }
        return pauses.allSatisfy { $0.at.isFinite && $0.length.isFinite && $0.at >= 0 && $0.length >= 0 }
    }

    private static func date(_ value: Any?) -> Date? {
        if let d = value as? Date { return d }
        if let n = value as? Double, n.isFinite { return Date(timeIntervalSince1970: n) }
        return nil
    }

    private static func number(_ value: Any?) -> Double? {
        if let n = value as? Double { return n.isFinite ? n : nil }
        if let n = value as? Int { return Double(n) }
        if let n = value as? NSNumber { return n.doubleValue.isFinite ? n.doubleValue : nil }
        return nil
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
