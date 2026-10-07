import Foundation

/// One recording on the watch and where it is on its way to the iPhone.
///
/// A recording is one or more audio files ("parts"): the watch closes the
/// file at every pause, so everything recorded so far is a complete,
/// readable file even if watchOS ends the app during a long pause, and
/// continues in a new file on Resume. The iPhone joins the parts in order.
///
/// A part's file is deleted only when the iPhone app confirms it stored it
/// (an acknowledgment over WatchConnectivity), not merely when the system
/// reports the transfer finished.
struct WatchRecordingEntry: Codable, Equatable, Identifiable, Sendable {
    enum Status: String, Codable, Sendable {
        /// The microphone is on (or the app died while it was)
        case recording
        /// Finished and waiting for the iPhone
        case saved
        /// Handed to WatchConnectivity; waiting for the iPhone to confirm
        case sending
        /// The iPhone confirmed every part. The watch copies are deleted.
        case delivered
        /// The last attempt failed. The files are kept and sent again.
        case failed
    }

    var id: UUID
    var startedAt: Date
    /// Audio files in order (part 0, 1, …)
    var parts: [String]
    /// Parts the iPhone confirmed it stored (their files are deleted)
    var confirmedParts: [Int] = []
    /// When each unconfirmed part was handed to the system (sent again if
    /// no confirmation follows)
    var handedOff: [Int: Date] = [:]
    var status: Status
    /// Set once the recording is finished
    var metadata: WatchRecordingMetadata?
    /// Pauses so far, kept while recording so a recording recovered after a
    /// crash still maps file time to clock time
    var pauses: [WatchRecordingMetadata.Pause] = []
    /// What it is, its notebook, its planned length and the moments marked,
    /// kept while recording so a recording recovered after a crash keeps
    /// them. The notebook name and the markers are dropped once the iPhone
    /// has every part (the watch keeps no more than it needs).
    var kind: RecordingKind?
    var notebook: String?
    var plannedMinutes: Int?
    var markers: [WatchMarker] = []
    /// Why the last transfer failed (system error text; never content)
    var lastError: String?
    var updatedAt: Date

    var duration: TimeInterval { metadata?.duration ?? 0 }
    var hasAudioOnWatch: Bool { status != .delivered }
    var unconfirmedParts: [Int] { parts.indices.filter { !confirmedParts.contains($0) } }
    /// The user may delete it from the watch: nothing in flight, and the
    /// iPhone holds none of it (or all of it)
    var canDelete: Bool {
        status == .delivered || (status != .recording && status != .sending && confirmedParts.isEmpty)
    }

    init(id: UUID, startedAt: Date, parts: [String], status: Status, updatedAt: Date) {
        self.id = id
        self.startedAt = startedAt
        self.parts = parts
        self.status = status
        self.updatedAt = updatedAt
    }

    enum CodingKeys: String, CodingKey {
        case id, startedAt, parts, confirmedParts, handedOff, status, metadata, pauses, lastError, updatedAt
        case kind, notebook, plannedMinutes, markers
        case fileName   // first builds: one file per recording
    }

    /// Tolerant of rows written by other builds: missing fields take their
    /// defaults, so a newer or older index never reads as empty.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        startedAt = try c.decodeIfPresent(Date.self, forKey: .startedAt) ?? Date()
        if let parts = try c.decodeIfPresent([String].self, forKey: .parts) {
            self.parts = parts
        } else {
            self.parts = try c.decodeIfPresent(String.self, forKey: .fileName).map { [$0] } ?? []
        }
        confirmedParts = try c.decodeIfPresent([Int].self, forKey: .confirmedParts) ?? []
        handedOff = try c.decodeIfPresent([Int: Date].self, forKey: .handedOff) ?? [:]
        let status: Status? = try? c.decodeIfPresent(Status.self, forKey: .status)
        let metadata: WatchRecordingMetadata? = try? c.decodeIfPresent(WatchRecordingMetadata.self, forKey: .metadata)
        self.metadata = metadata
        // Unknown status, or a finished row whose metadata can't be read:
        // let launch recovery rebuild it from the files
        if let status, status == .delivered || status == .recording || metadata != nil {
            self.status = status
        } else {
            self.status = .recording
        }
        let pauses: [WatchRecordingMetadata.Pause]? = try? c.decodeIfPresent([WatchRecordingMetadata.Pause].self, forKey: .pauses)
        self.pauses = pauses ?? []
        let lastError: String? = try? c.decodeIfPresent(String.self, forKey: .lastError)
        self.lastError = lastError
        let updatedAt: Date? = try? c.decodeIfPresent(Date.self, forKey: .updatedAt)
        self.updatedAt = updatedAt ?? Date()
        let kind: String? = try? c.decodeIfPresent(String.self, forKey: .kind)
        self.kind = kind.flatMap(RecordingKind.init(rawValue:))
        let notebook: String? = try? c.decodeIfPresent(String.self, forKey: .notebook)
        self.notebook = Notebook.normalize(notebook)
        let planned: Int? = try? c.decodeIfPresent(Int.self, forKey: .plannedMinutes)
        self.plannedMinutes = planned
        let markers: [WatchMarker]? = try? c.decodeIfPresent([WatchMarker].self, forKey: .markers)
        self.markers = markers ?? []
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(startedAt, forKey: .startedAt)
        try c.encode(parts, forKey: .parts)
        try c.encode(confirmedParts, forKey: .confirmedParts)
        try c.encode(handedOff, forKey: .handedOff)
        try c.encode(status, forKey: .status)
        try c.encodeIfPresent(metadata, forKey: .metadata)
        try c.encode(pauses, forKey: .pauses)
        try c.encodeIfPresent(lastError, forKey: .lastError)
        try c.encode(updatedAt, forKey: .updatedAt)
        try c.encodeIfPresent(kind, forKey: .kind)
        try c.encodeIfPresent(notebook, forKey: .notebook)
        try c.encodeIfPresent(plannedMinutes, forKey: .plannedMinutes)
        if !markers.isEmpty { try c.encode(markers, forKey: .markers) }
    }
}

/// Which parts of a recording carry audio, and the pauses and start that
/// go with them. Parts that are empty or unreadable (a Resume immediately
/// followed by a Pause; the file being written when the app died) are
/// dropped and the pauses around them merge, so the phone never gets a
/// file it can't join and file time still maps to clock time.
enum PartLayout {
    struct Result: Equatable {
        /// Indices of the parts kept, in order
        var keep: [Int]
        /// Pauses at the boundaries of the kept parts (seconds of kept audio)
        var pauses: [WatchRecordingMetadata.Pause]
        /// Clock time before the first kept audio (pauses and empty parts before it)
        var startShift: Double
        /// Seconds of audio kept
        var duration: Double
    }

    /// Shorter parts count as empty
    static let minimumPart = 0.05

    /// `lengths[i]` is part i's audio (nil = unreadable); `pauses[i]` the
    /// pause after part i, where known.
    static func normalize(lengths: [Double?], pauses: [WatchRecordingMetadata.Pause]) -> Result {
        var keep: [Int] = [], out: [WatchRecordingMetadata.Pause] = []
        var offset = 0.0, carry = 0.0, shift = 0.0
        for (i, length) in lengths.enumerated() {
            if i > 0, i - 1 < pauses.count { carry += max(0, pauses[i - 1].length) }
            guard let length, length >= minimumPart else {
                carry += max(0, length ?? 0)
                continue
            }
            if keep.isEmpty {
                shift = carry
            } else if carry > 0 {
                out.append(.init(at: offset, length: carry))
            }
            carry = 0
            keep.append(i)
            offset += length
        }
        return Result(keep: keep, pauses: out, startShift: shift, duration: offset)
    }
}

/// The watch's list of recordings, kept as a small JSON index next to the
/// audio files (Application Support/Recordings). Audio stays on the watch
/// only until the iPhone confirms it has it.
@MainActor
final class WatchRecordingStore {
    let directory: URL
    private let fileManager: FileManager
    private var indexURL: URL { directory.appending(path: "index.json") }
    private(set) var entries: [WatchRecordingEntry] = []
    /// Called after every change (the UI refreshes from it)
    var onChange: (() -> Void)?
    /// Injectable for tests
    var clock: () -> Date = Date.init

    /// Delivered rows kept for the "Delivered" list, newest first
    static let deliveredKept = 20
    static let deliveredMaxAge: TimeInterval = 7 * 86_400
    /// A part handed to the system but not confirmed by the iPhone is sent
    /// again after this long (the iPhone ignores duplicates)
    static let resendAfter: TimeInterval = 3600

    nonisolated static var defaultDirectory: URL {
        URL.applicationSupportDirectory.appending(path: "Recordings", directoryHint: .isDirectory)
    }

    init(directory: URL = WatchRecordingStore.defaultDirectory, fileManager: FileManager = .default) {
        self.directory = directory
        self.fileManager = fileManager
        try? fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        if let data = try? Data(contentsOf: indexURL) {
            if let saved = try? JSONDecoder().decode([WatchRecordingEntry].self, from: data) {
                entries = saved
            } else {
                // Never treat an unreadable index as "no recordings": keep it
                // aside, and rebuild the rows from the audio files
                let aside = directory.appending(path: "index-unreadable-\(Int(Date().timeIntervalSince1970)).json")
                try? fileManager.moveItem(at: indexURL, to: aside)
            }
        }
        adoptUnlistedFiles()
    }

    static func partName(_ id: UUID, _ part: Int) -> String { "\(id.uuidString)-p\(part).\(WatchTransfer.fileExtension)" }
    func url(_ name: String) -> URL { directory.appending(path: name) }
    func partURLs(_ entry: WatchRecordingEntry) -> [URL] { entry.parts.map(url) }

    func entry(_ id: UUID) -> WatchRecordingEntry? { entries.first { $0.id == id } }

    /// Newest first
    var recent: [WatchRecordingEntry] { entries.sorted { $0.startedAt > $1.startedAt } }

    /// Finished recordings still on the watch (not yet confirmed by the iPhone)
    var waitingCount: Int { entries.filter { $0.status != .delivered && $0.status != .recording }.count }

    // MARK: Lifecycle

    /// A new recording is about to start: returns where to write part 0.
    @discardableResult
    func beginRecording(id: UUID, startedAt: Date, kind: RecordingKind? = nil, notebook: String? = nil,
                        plannedMinutes: Int? = nil) -> URL {
        var entry = WatchRecordingEntry(id: id, startedAt: startedAt, parts: [Self.partName(id, 0)],
                                        status: .recording, updatedAt: clock())
        entry.kind = kind
        entry.notebook = Notebook.normalize(notebook)
        entry.plannedMinutes = plannedMinutes
        entries.removeAll { $0.id == id }
        entries.append(entry)
        persist()
        return url(entry.parts[0])
    }

    /// Resume after a pause: the next part's file.
    @discardableResult
    func beginPart(_ id: UUID) -> URL? {
        guard let i = entries.firstIndex(where: { $0.id == id }) else { return nil }
        let name = Self.partName(id, entries[i].parts.count)
        entries[i].parts.append(name)
        entries[i].updatedAt = clock()
        persist()
        return url(name)
    }

    /// The last part never got audio (resume failed): forget it.
    func dropLastPart(_ id: UUID) {
        guard let i = entries.firstIndex(where: { $0.id == id }), entries[i].parts.count > 1 else { return }
        try? fileManager.removeItem(at: url(entries[i].parts.removeLast()))
        persist()
    }

    func recordPauses(_ id: UUID, _ pauses: [WatchRecordingMetadata.Pause]) {
        update(id) { $0.pauses = pauses }
    }

    /// The moments marked so far (saved at once, so a crash keeps them).
    func recordMarkers(_ id: UUID, _ markers: [WatchMarker]) {
        update(id) { $0.markers = markers }
    }

    /// The planned length changed (+15 min, No limit).
    func recordPlan(_ id: UUID, plannedMinutes: Int?) {
        update(id) { $0.plannedMinutes = plannedMinutes }
    }

    /// Recording stopped: keep the parts with audio (deleting the rest),
    /// line the pauses up with them, and mark it ready to send. Returns
    /// false (and discards the recording) when no part has audio.
    @discardableResult
    func finish(_ metadata: WatchRecordingMetadata, partLengths: [Double?]? = nil) -> Bool {
        guard let entry = entry(metadata.recordingID) else { return false }
        var final = metadata
        var parts = entry.parts
        if let partLengths, partLengths.count == parts.count {
            let layout = PartLayout.normalize(lengths: partLengths, pauses: metadata.pauses)
            guard !layout.keep.isEmpty else {
                discard(entry.id)
                return false
            }
            for i in parts.indices where !layout.keep.contains(i) { try? fileManager.removeItem(at: url(parts[i])) }
            parts = layout.keep.map { parts[$0] }
            final.startedAt = metadata.startedAt.addingTimeInterval(layout.startShift)
            final.endedAt = max(final.endedAt, final.startedAt)
            final.pauses = layout.pauses
            final.duration = layout.duration
        }
        update(entry.id) {
            $0.parts = parts
            $0.startedAt = final.startedAt
            $0.metadata = final.forPart(0, of: parts.count)
            $0.pauses = final.pauses
            $0.kind = final.kind
            $0.notebook = final.notebook
            $0.plannedMinutes = final.plannedMinutes
            $0.markers = final.markers
            $0.status = .saved
            $0.lastError = nil
        }
        return true
    }

    /// Removes a recording that never became one (failed start), or one the
    /// user deleted before it was sent. Deletes its audio.
    func discard(_ id: UUID) {
        if let entry = entry(id) {
            for u in partURLs(entry) { try? fileManager.removeItem(at: u) }
        }
        entries.removeAll { $0.id == id }
        persist()
    }

    func markSending(_ id: UUID) {
        update(id) {
            $0.status = .sending
            $0.lastError = nil
        }
    }

    /// The system finished transferring a part. The file stays until the
    /// iPhone app confirms it stored it.
    func markHandedOff(_ id: UUID, part: Int) {
        update(id) {
            guard !$0.confirmedParts.contains(part) else { return }
            $0.handedOff[part] = self.clock()
        }
    }

    /// The iPhone confirmed it stored this part: delete the watch copy
    /// (privacy: the audio lives on one device). When every part is there
    /// the row says "Delivered" and keeps no audio.
    func markConfirmed(_ id: UUID, part: Int) {
        guard let entry = entry(id), entry.parts.indices.contains(part) else { return }
        try? fileManager.removeItem(at: url(entry.parts[part]))
        update(id) {
            if !$0.confirmedParts.contains(part) { $0.confirmedParts.append(part) }
            $0.handedOff[part] = nil
            if $0.unconfirmedParts.isEmpty, $0.status != .recording {
                $0.status = .delivered
                $0.lastError = nil
                // The iPhone has it all: keep only times and the type here
                $0.notebook = nil
                $0.markers = []
                $0.metadata?.notebook = nil
                $0.metadata?.markers = []
            }
        }
        prune()
    }

    /// A transfer failed: keep the file, try again later. A failure for a
    /// part the iPhone already confirmed (a duplicate send) changes nothing.
    func markFailed(_ id: UUID, part: Int, message: String) {
        update(id) {
            guard !$0.confirmedParts.contains(part), $0.status != .delivered else { return }
            $0.handedOff[part] = nil
            $0.status = .failed
            $0.lastError = message
        }
    }

    /// One file to (re)send.
    struct PendingPart: Equatable {
        var id: UUID
        var part: Int
        var url: URL
        var metadata: WatchRecordingMetadata
    }

    /// Parts that should be (re)sent: finished recordings with parts the
    /// iPhone hasn't confirmed, minus those WatchConnectivity is still
    /// working on and those handed off recently (waiting for confirmation).
    func needingTransfer(outstanding: Set<String>) -> [PendingPart] {
        let now = clock()
        var out: [PendingPart] = []
        for e in entries.sorted(by: { $0.startedAt < $1.startedAt }) {
            guard let metadata = e.metadata, e.status != .recording, e.status != .delivered else { continue }
            for part in e.unconfirmedParts {
                let u = url(e.parts[part])
                if let at = e.handedOff[part], now.timeIntervalSince(at) < Self.resendAfter { continue }
                guard !outstanding.contains(WatchTransfer.partKey(e.id, part)),
                      fileManager.fileExists(atPath: u.path(percentEncoded: false)) else { continue }
                out.append(PendingPart(id: e.id, part: part, url: u, metadata: metadata.forPart(part, of: e.parts.count)))
            }
        }
        return out
    }

    /// At launch: a row still marked `recording` means the app was ended
    /// mid-recording. Parts that can be read (`audioLength` returns their
    /// seconds) are kept and sent; unreadable ones (normally only the part
    /// being written) are deleted. Returns (kept recordings, lost recordings).
    @discardableResult
    func recoverInterrupted(appVersion: String, audioLength: (URL) -> TimeInterval?) -> (kept: Int, lost: Int) {
        var kept = 0, lost = 0
        for e in entries where e.status == .recording {
            let lengths = e.parts.map { audioLength(url($0)) }
            let layout = PartLayout.normalize(lengths: lengths, pauses: e.pauses)
            guard !layout.keep.isEmpty else {
                discard(e.id)
                lost += 1
                continue
            }
            let paused = layout.pauses.reduce(0) { $0 + $1.length }
            let start = e.startedAt.addingTimeInterval(layout.startShift)
            let metadata = WatchRecordingMetadata(recordingID: e.id, startedAt: e.startedAt,
                                                  endedAt: start.addingTimeInterval(layout.duration + paused),
                                                  duration: layout.duration, appVersion: appVersion, pauses: e.pauses,
                                                  kind: e.kind, notebook: e.notebook, plannedMinutes: e.plannedMinutes,
                                                  markers: e.markers)
            finish(metadata, partLengths: lengths)
            kept += 1
        }
        return (kept, lost)
    }

    /// Audio files no row points at (an unreadable index, a crash between
    /// writing a file and the index) become rows again, never deleted:
    /// recovery then keeps whatever can be read. A file of a part the iPhone
    /// already confirmed is removed.
    func adoptUnlistedFiles() {
        let files = (try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: [.creationDateKey])) ?? []
        var changed = false
        for u in files where u.pathExtension == WatchTransfer.fileExtension {
            let name = u.lastPathComponent
            if let i = entries.firstIndex(where: { $0.parts.contains(name) }) {
                // Listed. A part the iPhone already confirmed is a leftover copy.
                if let p = entries[i].parts.firstIndex(of: name), entries[i].confirmedParts.contains(p) {
                    try? fileManager.removeItem(at: u)
                }
                continue
            }
            guard let (id, part) = Self.parse(name) else { continue }
            let created = (try? u.resourceValues(forKeys: [.creationDateKey]).creationDate) ?? clock()
            if let i = entries.firstIndex(where: { $0.id == id }) {
                var parts = entries[i].parts
                while parts.count <= part { parts.append(Self.partName(id, parts.count)) }
                parts[part] = name
                entries[i].parts = parts
                if entries[i].status == .delivered { entries[i].status = .failed }
            } else {
                var entry = WatchRecordingEntry(id: id, startedAt: created, parts: [], status: .recording, updatedAt: clock())
                while entry.parts.count <= part { entry.parts.append(Self.partName(id, entry.parts.count)) }
                entry.parts[part] = name
                entries.append(entry)
            }
            changed = true
        }
        if changed { persist() }
    }

    /// "<uuid>-p<N>.m4a" (or "<uuid>.m4a" from the first builds)
    static func parse(_ name: String) -> (UUID, Int)? {
        let base = (name as NSString).deletingPathExtension
        if let id = UUID(uuidString: base) { return (id, 0) }
        guard let r = base.range(of: "-p", options: .backwards),
              let id = UUID(uuidString: String(base[..<r.lowerBound])),
              let part = Int(base[r.upperBound...]) else { return nil }
        return (id, part)
    }

    /// Drop old "Delivered" rows (they hold no audio).
    func prune() {
        let now = clock()
        let delivered = entries.filter { $0.status == .delivered }.sorted { $0.updatedAt > $1.updatedAt }
        let drop = Set(delivered.enumerated()
            .filter { i, e in i >= Self.deliveredKept || now.timeIntervalSince(e.updatedAt) > Self.deliveredMaxAge }
            .map { $0.element.id })
        if !drop.isEmpty {
            entries.removeAll { drop.contains($0.id) }
            persist()
        }
    }

    // MARK: Internals

    private func update(_ id: UUID, _ change: (inout WatchRecordingEntry) -> Void) {
        guard let i = entries.firstIndex(where: { $0.id == id }) else { return }
        change(&entries[i])
        entries[i].updatedAt = clock()
        persist()
    }

    private func persist() {
        if let data = try? JSONEncoder().encode(entries) {
            try? data.write(to: indexURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        }
        onChange?()
    }
}

/// What the transfer queue needs from WatchConnectivity (a fake in tests).
@MainActor
protocol RecordingTransport: AnyObject {
    /// The session is activated and the iPhone app is installed
    var canTransfer: Bool { get }
    /// `WatchTransfer.partKey`s of parts WatchConnectivity is still delivering
    var outstandingTransfers: Set<String> { get }
    func transferFile(_ url: URL, metadata: [String: Any])
}

/// Sends finished recordings to the iPhone and settles the result: confirmed
/// by the iPhone → the watch copy is deleted; failed → kept and retried.
@MainActor
final class WatchTransferQueue {
    let store: WatchRecordingStore
    weak var transport: RecordingTransport?

    init(store: WatchRecordingStore, transport: RecordingTransport?) {
        self.store = store
        self.transport = transport
    }

    /// Queue every part that still has to go. Safe to call often: parts
    /// in flight or waiting for confirmation aren't queued again, and the
    /// iPhone imports each recording id once anyway. Returns files queued.
    @discardableResult
    func sendPending() -> Int {
        guard let transport, transport.canTransfer else { return 0 }
        let todo = store.needingTransfer(outstanding: transport.outstandingTransfers)
        for item in todo {
            store.markSending(item.id)
            transport.transferFile(item.url, metadata: item.metadata.dictionary)
        }
        return todo.count
    }

    /// WatchConnectivity finished a transfer (`session(_:didFinish:error:)`).
    /// `errorMessage` is nil on success; success only means the system has
    /// it — the file stays until the iPhone confirms.
    func didFinish(recordingID: UUID, part: Int, errorMessage: String?) {
        if let errorMessage {
            store.markFailed(recordingID, part: part, message: errorMessage)
        } else {
            store.markHandedOff(recordingID, part: part)
        }
    }

    /// The iPhone confirmed it stored these parts (`WatchTransfer.Key.ack`).
    func confirmed(_ keys: [String]) {
        for key in keys {
            guard let (id, part) = WatchTransfer.parsePartKey(key) else { continue }
            store.markConfirmed(id, part: part)
        }
    }
}
