import Foundation
import Observation
import WatchConnectivity

/// The watch end of WatchConnectivity. Recordings go to the iPhone with
/// `transferFile(_:metadata:)`: the system queues them while the phone is
/// away and delivers them in the background, and reports each one back in
/// `session(_:didFinish:error:)` (an error keeps the file for another try).
/// The watch copy is deleted when the iPhone app confirms it stored the part
/// (`session(_:didReceiveUserInfo:)` with `WatchTransfer.Key.ack`). The
/// iPhone's recent notebook names arrive as the application context (latest
/// wins; kept by the system, not copied by the app).
@MainActor
@Observable
final class WatchConnection: NSObject, RecordingTransport {
    private(set) var activated = false
    private(set) var companionInstalled = false
    private(set) var reachable = false
    /// The iPhone's recent notebooks, for the Record flow's notebook step.
    /// Empty until the iPhone has sent a list (then only None is offered).
    private(set) var recentNotebooks: [String] = []
    @ObservationIgnored var queue: WatchTransferQueue?
    @ObservationIgnored private let session: WCSession? = WCSession.isSupported() ? .default : nil

    func activate() {
        guard let session, session.delegate == nil else {
            queue?.sendPending()
            return
        }
        session.delegate = self
        session.activate()
    }

    /// Whether the system still has deliveries for us (background task handling)
    var hasContentPending: Bool { session?.hasContentPending ?? false }

    // MARK: RecordingTransport

    var canTransfer: Bool { activated && companionInstalled }

    var outstandingTransfers: Set<String> {
        Set((session?.outstandingFileTransfers ?? []).compactMap { transfer in
            Self.part(transfer.file.metadata).map { WatchTransfer.partKey($0.id, $0.part) }
        })
    }

    func transferFile(_ url: URL, metadata: [String: Any]) {
        session?.transferFile(url, metadata: metadata)
    }

    #if DEBUG
    /// Demo states: a notebook list without an iPhone (kept over what activation reads)
    @ObservationIgnored private var demoNotebooks: [String]?

    func showDemoNotebooks(_ names: [String]) {
        demoNotebooks = WatchTransfer.notebooks(fromContext: WatchTransfer.notebookContext(names))
        recentNotebooks = demoNotebooks ?? []
    }
    #endif

    private func setNotebooks(_ names: [String]) {
        #if DEBUG
        if let demoNotebooks {
            recentNotebooks = demoNotebooks
            return
        }
        #endif
        recentNotebooks = names
    }

    /// Recording id and part number of a transfer, from its metadata.
    nonisolated static func part(_ metadata: [String: Any]?) -> (id: UUID, part: Int)? {
        guard let id = (metadata?[WatchTransfer.Key.recordingID] as? String).flatMap(UUID.init(uuidString:)) else { return nil }
        return (id, (metadata?[WatchTransfer.Key.part] as? Int) ?? 0)
    }
}

extension WatchConnection: WCSessionDelegate {
    nonisolated func session(_ session: WCSession, activationDidCompleteWith state: WCSessionActivationState, error: Error?) {
        let ok = state == .activated
        let installed = session.isCompanionAppInstalled
        let reachable = session.isReachable
        // The latest list the iPhone sent, also while this app wasn't running
        let notebooks = WatchTransfer.notebooks(fromContext: session.receivedApplicationContext)
        Task { @MainActor in
            self.activated = ok
            self.companionInstalled = installed
            self.reachable = reachable
            self.setNotebooks(notebooks)
            self.queue?.sendPending()
        }
    }

    /// The iPhone's recent notebook names changed (names only).
    nonisolated func session(_ session: WCSession, didReceiveApplicationContext applicationContext: [String: Any]) {
        let notebooks = WatchTransfer.notebooks(fromContext: applicationContext)
        Task { @MainActor in self.setNotebooks(notebooks) }
    }

    nonisolated func session(_ session: WCSession, didFinish fileTransfer: WCSessionFileTransfer, error: Error?) {
        guard let key = Self.part(fileTransfer.file.metadata) else { return }
        let message = error?.localizedDescription
        Task { @MainActor in self.queue?.didFinish(recordingID: key.id, part: key.part, errorMessage: message) }
    }

    /// The iPhone confirms parts it stored; only then is the watch copy deleted.
    nonisolated func session(_ session: WCSession, didReceiveUserInfo userInfo: [String: Any] = [:]) {
        guard let keys = userInfo[WatchTransfer.Key.ack] as? [String], !keys.isEmpty else { return }
        Task { @MainActor in self.queue?.confirmed(keys) }
    }

    nonisolated func sessionReachabilityDidChange(_ session: WCSession) {
        let reachable = session.isReachable
        Task { @MainActor in
            self.reachable = reachable
            // Phone back in range: retry anything that failed
            if reachable { self.queue?.sendPending() }
        }
    }

    nonisolated func sessionCompanionAppInstalledDidChange(_ session: WCSession) {
        let installed = session.isCompanionAppInstalled
        Task { @MainActor in
            self.companionInstalled = installed
            self.queue?.sendPending()
        }
    }
}
