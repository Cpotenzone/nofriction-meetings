import Foundation

/// Apple Watch recordings received but not imported yet:
/// `<recordingId>.json` (metadata) + `<recordingId>.m4a` (audio).
///
/// WatchConnectivity deletes a received file as soon as its delegate method
/// returns, and may call it while the app is in the background, so the file
/// is moved here first, synchronously; the import into the store follows.
struct WatchInbox: Sendable {
    let directory: URL

    static let shared = WatchInbox(directory: Storage.watchInbox)

    /// Staging runs on WatchConnectivity's queue, listing on the main actor:
    /// one at a time, so a half-staged recording is never seen (or cleared).
    private static let lock = NSLock()

    struct Item: Sendable {
        var metadata: WatchRecordingMetadata
        var audioURL: URL
    }

    func audioURL(_ id: UUID) -> URL { directory.appending(path: "\(id.uuidString).\(WatchTransfer.fileExtension)") }
    func metadataURL(_ id: UUID) -> URL { directory.appending(path: "\(id.uuidString).json") }

    /// Keep a received file. Metadata is written first, so a crash in between
    /// leaves a JSON with no audio (dropped), never audio with no times.
    func stage(_ file: URL, metadata: WatchRecordingMetadata) throws {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let id = metadata.recordingID
        let data = try JSONEncoder().encode(metadata)
        try data.write(to: metadataURL(id), options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        let target = audioURL(id)
        try? FileManager.default.removeItem(at: target)   // a re-delivery replaces the earlier copy
        do {
            try FileManager.default.moveItem(at: file, to: target)
        } catch {
            try? FileManager.default.removeItem(at: metadataURL(id))
            throw error
        }
    }

    /// Staged recordings, oldest first. A JSON without audio is cleared.
    func pending() -> [Item] {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        var items: [Item] = []
        for url in files where url.pathExtension == "json" {
            guard let data = try? Data(contentsOf: url),
                  let metadata = try? JSONDecoder().decode(WatchRecordingMetadata.self, from: data),
                  metadata.isValid else {
                try? FileManager.default.removeItem(at: url)
                continue
            }
            let audio = audioURL(metadata.recordingID)
            guard FileManager.default.fileExists(atPath: audio.path(percentEncoded: false)) else {
                try? FileManager.default.removeItem(at: url)
                continue
            }
            items.append(Item(metadata: metadata, audioURL: audio))
        }
        return items.sorted { $0.metadata.startedAt < $1.metadata.startedAt }
    }

    var count: Int { pending().count }

    func remove(_ id: UUID) {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        try? FileManager.default.removeItem(at: audioURL(id))
        try? FileManager.default.removeItem(at: metadataURL(id))
    }

    /// Audio with no metadata (interrupted staging): nothing can be imported from it.
    func removeOrphans() {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        for url in files where url.pathExtension == WatchTransfer.fileExtension {
            let json = url.deletingPathExtension().appendingPathExtension("json")
            if !FileManager.default.fileExists(atPath: json.path(percentEncoded: false)) {
                try? FileManager.default.removeItem(at: url)
            }
        }
    }
}
