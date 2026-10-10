import Foundation
import Security

/// What the iPhone keeps about each paired Mac (docs/SYNC.md): the pin, the
/// Mac's cursor, and what it has sent or received, as ids and hashes of the
/// non-transcript records (ids only for lines). No transcript text, no keys.
/// One JSON file per Mac in Application Support/Sync.
struct SyncMacState: Codable, Equatable {
    var macID: String
    var name: String
    var fingerprint: String
    var hosts: [String]
    var port: UInt16
    var pairedAt: Date = .now
    /// Highest Mac change number applied here
    var since: Int64 = 0
    var lastSyncedAt: Date?
    var lastError: String?
    /// entity → id → hash of the record as last exchanged (recording, notes, mark, ref, topic, strike)
    var known: [String: [String: String]] = [:]
    /// Lines the Mac has (sent or received)
    var lines: Set<String> = []
    /// Line id → the edit counter last sent (see `SyncLedger.lineEdits`)
    var sentEdits: [String: Int] = [:]

    func hash(_ entity: String, _ id: String) -> String? { known[entity]?[id] }
    mutating func setHash(_ entity: String, _ id: String, _ h: String?) { known[entity, default: [:]][id] = h }
}

/// Device-wide sync bookkeeping (ids only): edits to lines made by Delete and
/// Strike, and ids that are gone for good. `RedactionEngine` reports edits.
struct SyncLedgerData: Codable, Equatable {
    /// Line id → how many times its text was edited here
    var lineEdits: [String: Int] = [:]
    /// entity → ids deleted (here or on a Mac): never imported again
    var gone: [String: Set<String>] = [:]
}

@MainActor
enum SyncLedger {
    static var directory: URL = URL.applicationSupportDirectory.appending(path: "Sync", directoryHint: .isDirectory)
    private static var ledgerURL: URL { directory.appending(path: "ledger.json") }

    static func load() -> SyncLedgerData {
        guard let data = try? Data(contentsOf: ledgerURL) else { return SyncLedgerData() }
        return (try? JSONDecoder().decode(SyncLedgerData.self, from: data)) ?? SyncLedgerData()
    }

    static func save(_ l: SyncLedgerData) {
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        if let data = try? JSONEncoder().encode(l) { try? data.write(to: ledgerURL, options: [.atomic, .completeFileProtection]) }
    }

    /// A line's text changed by Delete or Strike: it travels to each Mac as an edit.
    static func lineEdited(_ s: Segment) {
        guard let id = s.syncID else { return }
        var l = load()
        l.lineEdits[SyncIDs.wire(id), default: 0] += 1
        save(l)
    }

    /// The recording's notes were rewritten by a Delete or Strike. No hash of
    /// the notes before the edit stays behind. A Mac that had exactly those
    /// notes gets the same edit and rewrites its own copy the same way, so it
    /// now has `after`; a Mac that had other notes still gets these.
    static func notesRewritten(_ meetingID: UUID, before: String?, after: String?) {
        let rec = SyncIDs.wire(meetingID)
        for var s in SyncStore.all() {
            guard let known = s.known["notes"]?[rec] else { continue }
            s.setHash("notes", rec, known == before ? after : "edited")
            SyncStore.save(s)
        }
    }

    static func markGone(_ entity: String, _ id: String) {
        var l = load()
        l.gone[entity, default: []].insert(id)
        save(l)
    }
}

/// The per-Mac files and the Keychain secrets.
@MainActor
enum SyncStore {
    static let keychainService = "com.nofriction.meetings.sync"
    static let deviceIDKey = "syncDeviceID"

    private static func url(_ macID: String) -> URL { SyncLedger.directory.appending(path: "mac-\(macID).json") }

    static func all() -> [SyncMacState] {
        let files = (try? FileManager.default.contentsOfDirectory(at: SyncLedger.directory, includingPropertiesForKeys: nil)) ?? []
        return files.filter { $0.lastPathComponent.hasPrefix("mac-") }
            .compactMap { (try? Data(contentsOf: $0)).flatMap { try? JSONDecoder().decode(SyncMacState.self, from: $0) } }
            .sorted { $0.pairedAt < $1.pairedAt }
    }

    static func load(_ macID: String) -> SyncMacState? {
        (try? Data(contentsOf: url(macID))).flatMap { try? JSONDecoder().decode(SyncMacState.self, from: $0) }
    }

    static func save(_ s: SyncMacState) {
        try? FileManager.default.createDirectory(at: SyncLedger.directory, withIntermediateDirectories: true)
        if let data = try? JSONEncoder().encode(s) { try? data.write(to: url(s.macID), options: [.atomic, .completeFileProtection]) }
    }

    /// Forget: the secret leaves the Keychain and the Mac's file goes.
    static func forget(_ macID: String) {
        deleteSecret(macID)
        try? FileManager.default.removeItem(at: url(macID))
    }

    /// This iPhone's sync id (made once)
    static var deviceID: String {
        if let id = UserDefaults.standard.string(forKey: deviceIDKey), let w = SyncIDs.wire(id) { return w }
        let id = SyncIDs.wire(UUID())
        UserDefaults.standard.set(id, forKey: deviceIDKey)
        return id
    }

    // MARK: Keychain (this device only, never iCloud)

    private static func base(_ macID: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: "mac:\(macID)",
            kSecAttrSynchronizable as String: kCFBooleanFalse!,
        ]
    }

    static func setSecret(_ secret: Data, for macID: String) throws {
        SecItemDelete(base(macID) as CFDictionary)
        var q = base(macID)
        q[kSecValueData as String] = secret
        q[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(q as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainStore.Failure.status(status) }
    }

    static func secret(for macID: String) -> Data? {
        var q = base(macID)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let d = out as? Data, d.count == 32 else { return nil }
        return d
    }

    static func deleteSecret(_ macID: String) {
        SecItemDelete(base(macID) as CFDictionary)
    }
}
