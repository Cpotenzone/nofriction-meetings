import Foundation

/// One recording on the watch and where it is on its way to the iPhone.
struct WatchRecordingEntry: Codable, Equatable, Identifiable, Sendable {
    enum Status: String, Codable, Sendable {
        /// The microphone is on (or the app died while it was)
        case recording
        /// Finished and waiting for the iPhone
        case saved
        /// Handed to WatchConnectivity; the system delivers it when it can
        case sending
        /// The iPhone has it. The watch copy is deleted.
        case delivered
        /// The last attempt failed. The file is kept and sent again.
        case failed
    }

    var id: UUID
    var startedAt: Date
    var fileName: String
    var status: Status
    /// Set once the recording is finished
    var metadata: WatchRecordingMetadata?
    /// Why the last transfer failed (system error text; never content)
    var lastError: String?
    var updatedAt: Date

    var duration: TimeInterval { metadata?.duration ?? 0 }
    var hasAudioOnWatch: Bool { status != .delivered }
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

    func fileURL(for entry: WatchRecordingEntry) -> URL { directory.appending(path: entry.fileName) }
    func fileURL(for id: UUID) -> URL { directory.appending(path: Self.fileName(for: id)) }
    static func fileName(for id: UUID) -> String { "\(id.uuidString).\(WatchTransfer.fileExtension)" }

    func entry(_ id: UUID) -> WatchRecordingEntry? { entries.first { $0.id == id } }

    /// Newest first
    var recent: [WatchRecordingEntry] { entries.sorted { $0.startedAt > $1.startedAt } }

    /// Recordings still on the watch (not yet delivered)
    var waitingCount: Int { entries.filter { $0.status != .delivered && $0.status != .recording }.count }

    // MARK: Lifecycle

    /// A new recording is about to start: returns where to write it.
    @discardableResult
    func beginRecording(id: UUID, startedAt: Date) -> URL {
        let entry = WatchRecordingEntry(id: id, startedAt: startedAt, fileName: Self.fileName(for: id),
                                        status: .recording, updatedAt: clock())
        entries.removeAll { $0.id == id }
        entries.append(entry)
        persist()
        return fileURL(for: entry)
    }

    /// Recording stopped: ready to send.
    func finish(_ metadata: WatchRecordingMetadata) {
        update(metadata.recordingID) {
            $0.metadata = metadata
            $0.status = .saved
            $0.lastError = nil
        }
    }

    /// Removes a recording that never became one (failed start), or one the
    /// user deleted before it was sent. Deletes the audio.
    func discard(_ id: UUID) {
        try? fileManager.removeItem(at: fileURL(for: id))
        entries.removeAll { $0.id == id }
        persist()
    }

    func markSending(_ id: UUID) {
        update(id) {
            $0.status = .sending
            $0.lastError = nil
        }
    }

    /// The iPhone has the file: delete the watch copy (privacy: the audio
    /// lives on one device), keep a small row so the list can say "Delivered".
    func markDelivered(_ id: UUID) {
        guard let entry = entry(id) else { return }
        try? fileManager.removeItem(at: fileURL(for: entry))
        update(id) {
            $0.status = .delivered
            $0.lastError = nil
        }
        prune()
    }

    /// The transfer failed: keep the file, try again later.
    func markFailed(_ id: UUID, message: String) {
        update(id) {
            $0.status = .failed
            $0.lastError = message
        }
    }

    /// Recordings that should be (re)sent: finished but not yet delivered,
    /// minus those WatchConnectivity is still working on.
    func needingTransfer(outstanding: Set<UUID>) -> [WatchRecordingEntry] {
        entries
            .filter { e in
                guard e.metadata != nil, fileManager.fileExists(atPath: fileURL(for: e).path(percentEncoded: false)) else { return false }
                switch e.status {
                case .saved, .failed: return !outstanding.contains(e.id)
                case .sending: return !outstanding.contains(e.id)   // lost across a relaunch
                case .recording, .delivered: return false
                }
            }
            .sorted { $0.startedAt < $1.startedAt }
    }

    /// At launch: a row still marked `recording` means the app was killed
    /// mid-recording. Keep the audio if the file can be read (`audioLength`
    /// returns its seconds), otherwise delete it. Returns (kept, lost).
    @discardableResult
    func recoverInterrupted(appVersion: String, audioLength: (URL) -> TimeInterval?) -> (kept: Int, lost: Int) {
        var kept = 0, lost = 0
        for e in entries where e.status == .recording {
            let url = fileURL(for: e)
            if let seconds = audioLength(url), seconds > 0 {
                finish(WatchRecordingMetadata(recordingID: e.id, startedAt: e.startedAt,
                                              endedAt: e.startedAt.addingTimeInterval(seconds),
                                              duration: seconds, appVersion: appVersion))
                kept += 1
            } else {
                discard(e.id)
                lost += 1
            }
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
        let known = Set(entries.filter(\.hasAudioOnWatch).map(\.fileName))
        for url in (try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        where url.pathExtension == WatchTransfer.fileExtension && !known.contains(url.lastPathComponent) {
            try? fileManager.removeItem(at: url)
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
    /// Recording ids WatchConnectivity is still delivering
    var outstandingRecordingIDs: Set<UUID> { get }
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

    /// Queue everything that still has to go. Safe to call often: recordings
    /// already being delivered aren't queued twice, and the phone imports
    /// each recording id once anyway.
    @discardableResult
    func sendPending() -> Int {
        guard let transport, transport.canTransfer else { return 0 }
        let todo = store.needingTransfer(outstanding: transport.outstandingRecordingIDs)
        for entry in todo {
            guard let metadata = entry.metadata else { continue }
            store.markSending(entry.id)
            transport.transferFile(store.fileURL(for: entry), metadata: metadata.dictionary)
        }
        return todo.count
    }

    /// WatchConnectivity finished a transfer (`session(_:didFinish:error:)`).
    /// `errorMessage` is nil on success.
    func didFinish(recordingID: UUID, errorMessage: String?) {
        if let errorMessage {
            store.markFailed(recordingID, message: errorMessage)
        } else {
            store.markDelivered(recordingID)
        }
    }
}
