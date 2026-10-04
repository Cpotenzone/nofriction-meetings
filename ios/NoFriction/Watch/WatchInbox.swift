import Foundation

/// Apple Watch recordings received but not imported yet. Each part of a
/// recording is `<recordingId>-p<part>.m4a` plus its metadata
/// `<recordingId>-p<part>.json`; a recording is ready once every part is here.
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

    /// A complete recording: metadata (part 0's, for the whole recording) and its parts in order.
    struct Item: Sendable {
        var metadata: WatchRecordingMetadata
        var audioURLs: [URL]
    }

    func audioURL(_ id: UUID, part: Int) -> URL { directory.appending(path: "\(id.uuidString)-p\(part).\(WatchTransfer.fileExtension)") }
    func metadataURL(_ id: UUID, part: Int) -> URL { directory.appending(path: "\(id.uuidString)-p\(part).json") }

    /// Keep a received part. Metadata is written first, so a crash in between
    /// leaves a JSON with no audio (dropped), never audio with no times.
    func stage(_ file: URL, metadata: WatchRecordingMetadata) throws {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let id = metadata.recordingID, part = metadata.part
        let data = try JSONEncoder().encode(metadata)
        try data.write(to: metadataURL(id, part: part), options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        let target = audioURL(id, part: part)
        try? FileManager.default.removeItem(at: target)   // a re-delivery replaces the earlier copy
        do {
            try FileManager.default.moveItem(at: file, to: target)
        } catch {
            try? FileManager.default.removeItem(at: metadataURL(id, part: part))
            throw error
        }
    }

    /// Complete recordings, oldest first. Recordings still missing a part
    /// wait; a JSON without its audio is cleared.
    func pending() -> [Item] {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        var parts: [UUID: [Int: WatchRecordingMetadata]] = [:]
        for url in contents() where url.pathExtension == "json" {
            guard let data = try? Data(contentsOf: url),
                  let metadata = try? JSONDecoder().decode(WatchRecordingMetadata.self, from: data),
                  metadata.isValid,
                  url.lastPathComponent == metadataURL(metadata.recordingID, part: metadata.part).lastPathComponent else {
                try? FileManager.default.removeItem(at: url)
                continue
            }
            guard exists(audioURL(metadata.recordingID, part: metadata.part)) else {
                try? FileManager.default.removeItem(at: url)
                continue
            }
            parts[metadata.recordingID, default: [:]][metadata.part] = metadata
        }
        var items: [Item] = []
        for (id, byPart) in parts {
            guard let first = byPart[0] else { continue }
            let count = first.partCount
            guard (0..<count).allSatisfy({ byPart[$0] != nil }) else { continue }
            items.append(Item(metadata: first, audioURLs: (0..<count).map { audioURL(id, part: $0) }))
        }
        return items.sorted { $0.metadata.startedAt < $1.metadata.startedAt }
    }

    /// Recordings with at least one part here (complete or not)
    var recordingCount: Int {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        return Set(contents().filter { $0.pathExtension == "json" }.map { $0.lastPathComponent.components(separatedBy: "-p").first ?? "" }).count
    }

    /// Remove every staged part of a recording.
    func remove(_ id: UUID) {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        for url in contents() where url.lastPathComponent.hasPrefix("\(id.uuidString)-p") {
            try? FileManager.default.removeItem(at: url)
        }
    }

    /// Audio with no metadata (interrupted staging): nothing can be imported from it.
    func removeOrphans() {
        Self.lock.lock()
        defer { Self.lock.unlock() }
        for url in contents() where url.pathExtension == WatchTransfer.fileExtension {
            let json = url.deletingPathExtension().appendingPathExtension("json")
            if !exists(json) { try? FileManager.default.removeItem(at: url) }
        }
    }

    private func contents() -> [URL] {
        (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
    }

    private func exists(_ url: URL) -> Bool { FileManager.default.fileExists(atPath: url.path(percentEncoded: false)) }
}
