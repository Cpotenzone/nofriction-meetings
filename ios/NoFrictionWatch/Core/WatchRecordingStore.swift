import Foundation

/// One recording on the watch and where it is on its way to the iPhone.
///
/// A recording is one or more audio files ("parts"): the watch closes the
/// file at every pause, so everything recorded so far is a complete,
/// readable file even if watchOS ends the app during a long pause, and
/// continues in a new file on Resume. The iPhone joins the parts in order.
struct WatchRecordingEntry: Codable, Equatable, Identifiable, Sendable {
    enum Status: String, Codable, Sendable {
        /// The microphone is on (or the app died while it was)
        case recording
        /// Finished and waiting for the iPhone
        case saved
        /// Handed to WatchConnectivity; the system delivers it when it can
        case sending
        /// The iPhone has every part. The watch copies are deleted.
        case delivered
        /// The last attempt failed. The files are kept and sent again.
        case failed
    }

    var id: UUID
    var startedAt: Date
    /// Audio files in order (part 0, 1, …)
    var parts: [String]
    /// Parts the iPhone has (their files are deleted)
    var deliveredParts: [Int] = []
    var status: Status
    /// Set once the recording is finished
    var metadata: WatchRecordingMetadata?
    /// Pauses so far, kept while recording so a recording recovered after a
    /// crash still maps file time to clock time
    var pauses: [WatchRecordingMetadata.Pause] = []
    /// Why the last transfer failed (system error text; never content)
    var lastError: String?
    var updatedAt: Date

    var duration: TimeInterval { metadata?.duration ?? 0 }
    var hasAudioOnWatch: Bool { status != .delivered }
    var undeliveredParts: [Int] { parts.indices.filter { !deliveredParts.contains($0) } }
}

/// The watch's list of recordings, kept as a small JSON index next to the
/// audio files (Application Support/Recordings). Audio stays on the watch
/// only until the iPhone has it.
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

    nonisolated static var defaultDirectory: URL {
        URL.applicationSupportDirectory.appending(path: "Recordings", directoryHint: .isDirectory)
    }

    init(directory: URL = WatchRecordingStore.defaultDirectory, fileManager: FileManager = .default) {
        self.directory = directory
        self.fileManager = fileManager
        try? fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        if let data = try? Data(contentsOf: indexURL),
           let saved = try? JSONDecoder().decode([WatchRecordingEntry].self, from: data) {
            entries = saved
        }
    }

    static func partName(_ id: UUID, _ part: Int) -> String { "\(id.uuidString)-p\(part).\(WatchTransfer.fileExtension)" }
    func url(_ name: String) -> URL { directory.appending(path: name) }
    func partURLs(_ entry: WatchRecordingEntry) -> [URL] { entry.parts.map(url) }

    func entry(_ id: UUID) -> WatchRecordingEntry? { entries.first { $0.id == id } }

    /// Newest first
    var recent: [WatchRecordingEntry] { entries.sorted { $0.startedAt > $1.startedAt } }

    /// Finished recordings still on the watch (not yet delivered)
    var waitingCount: Int { entries.filter { $0.status != .delivered && $0.status != .recording }.count }

    // MARK: Lifecycle

    /// A new recording is about to start: returns where to write part 0.
    @discardableResult
    func beginRecording(id: UUID, startedAt: Date) -> URL {
        let entry = WatchRecordingEntry(id: id, startedAt: startedAt, parts: [Self.partName(id, 0)],
                                        status: .recording, updatedAt: clock())
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

    /// Recording stopped: ready to send.
    func finish(_ metadata: WatchRecordingMetadata) {
        update(metadata.recordingID) {
            $0.metadata = metadata.forPart(0, of: $0.parts.count)
            $0.pauses = metadata.pauses
            $0.status = .saved
            $0.lastError = nil
        }
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

    /// The iPhone has this part: delete the watch copy (privacy: the audio
    /// lives on one device). When every part is there the row says
    /// "Delivered" and keeps no audio.
    func markDelivered(_ id: UUID, part: Int) {
        guard let entry = entry(id), entry.parts.indices.contains(part) else { return }
        try? fileManager.removeItem(at: url(entry.parts[part]))
        update(id) {
            if !$0.deliveredParts.contains(part) { $0.deliveredParts.append(part) }
            if $0.undeliveredParts.isEmpty {
                $0.status = .delivered
                $0.lastError = nil
            }
        }
        prune()
    }

    /// A transfer failed: keep the files, try again later.
    func markFailed(_ id: UUID, message: String) {
        update(id) {
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

    /// Parts that should be (re)sent: finished recordings not yet delivered,
    /// minus those WatchConnectivity is still working on.
    func needingTransfer(outstanding: Set<String>) -> [PendingPart] {
        var out: [PendingPart] = []
        for e in entries.sorted(by: { $0.startedAt < $1.startedAt }) {
            guard let metadata = e.metadata, e.status != .recording, e.status != .delivered else { continue }
            for part in e.undeliveredParts {
                let u = url(e.parts[part])
                guard !outstanding.contains(Self.transferKey(e.id, part)),
                      fileManager.fileExists(atPath: u.path(percentEncoded: false)) else { continue }
                out.append(PendingPart(id: e.id, part: part, url: u, metadata: metadata.forPart(part, of: e.parts.count)))
            }
        }
        return out
    }

    /// Identifies one part's transfer ("<uuid>#<part>")
    nonisolated static func transferKey(_ id: UUID, _ part: Int) -> String { "\(id.uuidString)#\(part)" }

    /// At launch: a row still marked `recording` means the app was ended
    /// mid-recording. Parts that can be read (`audioLength` returns their
    /// seconds) are kept and sent; unreadable ones (normally only the part
    /// being written) are deleted. Returns (kept recordings, lost recordings).
    @discardableResult
    func recoverInterrupted(appVersion: String, audioLength: (URL) -> TimeInterval?) -> (kept: Int, lost: Int) {
        var kept = 0, lost = 0
        for e in entries where e.status == .recording {
            var parts: [String] = []
            var lengths: [Double] = []
            for name in e.parts {
                if let seconds = audioLength(url(name)), seconds > 0 {
                    parts.append(name)
                    lengths.append(seconds)
                } else {
                    try? fileManager.removeItem(at: url(name))
                }
            }
            guard !parts.isEmpty else {
                discard(e.id)
                lost += 1
                continue
            }
            let total = lengths.reduce(0, +)
            // Pauses that still have audio after them
            let pauses = e.pauses.filter { $0.at < total }
            let paused = pauses.reduce(0) { $0 + $1.length }
            update(e.id) { $0.parts = parts }
            finish(WatchRecordingMetadata(recordingID: e.id, startedAt: e.startedAt,
                                          endedAt: e.startedAt.addingTimeInterval(total + paused),
                                          duration: total, appVersion: appVersion, pauses: pauses))
            kept += 1
        }
        return (kept, lost)
    }

    /// Drop old "Delivered" rows (they hold no audio) and stray files.
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
        // Audio files with no row (e.g. a crash between create and index write)
        var known = Set<String>()
        for e in entries where e.hasAudioOnWatch {
            for (i, name) in e.parts.enumerated() where !e.deliveredParts.contains(i) { known.insert(name) }
        }
        for u in (try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        where u.pathExtension == WatchTransfer.fileExtension && !known.contains(u.lastPathComponent) {
            try? fileManager.removeItem(at: u)
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
    /// `WatchRecordingStore.transferKey`s of parts WatchConnectivity is still delivering
    var outstandingTransfers: Set<String> { get }
    func transferFile(_ url: URL, metadata: [String: Any])
}

/// Sends finished recordings to the iPhone and settles the result:
/// delivered → the watch copy is deleted; failed → kept and retried.
@MainActor
final class WatchTransferQueue {
    let store: WatchRecordingStore
    weak var transport: RecordingTransport?

    init(store: WatchRecordingStore, transport: RecordingTransport?) {
        self.store = store
        self.transport = transport
    }

    /// Queue every part that still has to go. Safe to call often: parts
    /// already being delivered aren't queued twice, and the phone imports
    /// each recording id once anyway. Returns the number of files queued.
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
    /// `errorMessage` is nil on success.
    func didFinish(recordingID: UUID, part: Int, errorMessage: String?) {
        if let errorMessage {
            store.markFailed(recordingID, message: errorMessage)
        } else {
            store.markDelivered(recordingID, part: part)
        }
    }
}
