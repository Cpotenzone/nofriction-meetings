import Foundation

// Compiled into both the iPhone app and its screen-capture broadcast
// extension (docs/SCREEN_CAPTURE_IOS.md): where the extension leaves its
// screens and app audio, and the flags the app passes to it. Pure
// Foundation. Nothing here touches the network.

/// The app ↔ extension contract.
enum ScreenCaptureContract {
    /// Shared container of the app and the extension (App Group)
    static let appGroup = "group.com.nofriction.meetings"
    /// The ReplayKit broadcast upload extension, embedded in the app's PlugIns/
    static let extensionBundleID = "com.nofriction.meetings.broadcast"
    /// Folder inside the group container: one sub-folder per broadcast
    static let folderName = "ScreenCapture"

    /// Shared `UserDefaults(suiteName: appGroup)` keys
    enum Key {
        /// App → extension: write the app audio ("Transcribe what's playing").
        /// True only for a noFriction Pro user who left the option on; any
        /// other value (missing, false) means the extension never writes app audio.
        static let appAudio = "screenCapture.appAudio"
        /// App → extension: the recording is paused; keep nothing
        static let paused = "screenCapture.paused"
        /// App → extension: the recording stopped; end the broadcast with this
        /// broadcast id (or "*" for whichever is running)
        static let stopRequest = "screenCapture.stopRequest"
    }

    /// Darwin notifications (names only, no payload) between the two processes
    enum Signal {
        /// Extension → app: a broadcast started
        static let started = "com.nofriction.meetings.screen.started"
        /// Extension → app: a broadcast ended and its files are complete
        static let finished = "com.nofriction.meetings.screen.finished"
        /// App → extension: read `Key.stopRequest` / `Key.paused` now
        static let control = "com.nofriction.meetings.screen.control"
    }

    /// A broadcast whose manifest hasn't been touched for this long is over
    /// (the extension was killed without `broadcastFinished`).
    static let staleAfter: TimeInterval = 15
    /// The extension refreshes the manifest at least this often while running
    static let heartbeat: TimeInterval = 2

    static let frameExtension = "jpg"
    static let audioExtension = "m4a"

    /// The shared folder, or nil when the App Group isn't provisioned.
    static func root(fileManager: FileManager = .default) -> URL? {
        fileManager.containerURL(forSecurityApplicationGroupIdentifier: appGroup)?
            .appending(path: folderName, directoryHint: .isDirectory)
    }

    static func defaults() -> UserDefaults? { UserDefaults(suiteName: appGroup) }

    /// `f-<milliseconds since 1970>.jpg`: a screen kept at that moment
    static func frameName(at date: Date) -> String {
        "f-\(Int64((date.timeIntervalSince1970 * 1000).rounded())).\(frameExtension)"
    }

    /// The capture time encoded in a frame's file name; nil for anything else.
    static func frameDate(fromName name: String) -> Date? {
        guard name.hasPrefix("f-"), name.hasSuffix("." + frameExtension) else { return nil }
        let digits = name.dropFirst(2).dropLast(frameExtension.count + 1)
        guard !digits.isEmpty, digits.allSatisfy(\.isNumber), let ms = Int64(digits) else { return nil }
        return Date(timeIntervalSince1970: Double(ms) / 1000)
    }

    /// `a-<n>.m4a`: the n-th stretch of app audio (a new one after each pause)
    static func audioPartName(_ index: Int) -> String { "a-\(index).\(audioExtension)" }
}

/// What a broadcast left in its folder, written by the extension (atomically)
/// and read by the app. Times and file names only, never content.
struct ScreenCaptureManifest: Codable, Equatable, Sendable {
    struct AudioPart: Codable, Equatable, Sendable {
        var file: String
        /// Wall-clock time of the part's first sample
        var start: Date
        /// Set once the file is complete (closed after a pause or at the end)
        var end: Date?
    }

    static let fileName = "manifest.json"

    var id: UUID
    var startedAt: Date
    var endedAt: Date?
    /// The extension was allowed to write app audio for this broadcast
    var appAudio: Bool
    var parts: [AudioPart] = []
    /// Screens kept so far (some may already be imported and deleted)
    var framesKept: Int = 0
    /// Seconds of near-black frames skipped: video an app hides from capture
    var hiddenSeconds: Double = 0
    /// Last time the extension wrote this file
    var heartbeat: Date

    init(id: UUID = UUID(), startedAt: Date, appAudio: Bool) {
        self.id = id
        self.startedAt = startedAt
        self.appAudio = appAudio
        self.heartbeat = startedAt
    }

    /// Running: not ended, and the extension wrote recently.
    func isLive(at now: Date) -> Bool {
        endedAt == nil && now.timeIntervalSince(heartbeat) < ScreenCaptureContract.staleAfter
    }

    static func load(from folder: URL) -> ScreenCaptureManifest? {
        guard let data = try? Data(contentsOf: folder.appending(path: fileName)) else { return nil }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        return try? decoder.decode(ScreenCaptureManifest.self, from: data)
    }

    func write(to folder: URL) throws {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .secondsSince1970
        try encoder.encode(self).write(to: folder.appending(path: Self.fileName), options: .atomic)
    }
}
