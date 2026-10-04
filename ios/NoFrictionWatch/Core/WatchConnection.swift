import Foundation
import Observation
import WatchConnectivity

/// The watch end of WatchConnectivity. Recordings go to the iPhone with
/// `transferFile(_:metadata:)`: the system queues them while the phone is
/// away and delivers them in the background, and reports each one back in
/// `session(_:didFinish:error:)`, where the queue deletes the watch copy
/// (success) or keeps it for another try (error).
@MainActor
@Observable
final class WatchConnection: NSObject, RecordingTransport {
    private(set) var activated = false
    private(set) var companionInstalled = false
    private(set) var reachable = false
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

    var outstandingRecordingIDs: Set<UUID> {
        Set((session?.outstandingFileTransfers ?? []).compactMap { Self.recordingID($0.file.metadata) })
    }

    func transferFile(_ url: URL, metadata: [String: Any]) {
        session?.transferFile(url, metadata: metadata)
    }

    nonisolated static func recordingID(_ metadata: [String: Any]?) -> UUID? {
        (metadata?[WatchTransfer.Key.recordingID] as? String).flatMap(UUID.init(uuidString:))
    }
}

extension WatchConnection: WCSessionDelegate {
    nonisolated func session(_ session: WCSession, activationDidCompleteWith state: WCSessionActivationState, error: Error?) {
        let ok = state == .activated
        let installed = session.isCompanionAppInstalled
        let reachable = session.isReachable
        Task { @MainActor in
            self.activated = ok
            self.companionInstalled = installed
            self.reachable = reachable
            self.queue?.sendPending()
        }
    }

    nonisolated func session(_ session: WCSession, didFinish fileTransfer: WCSessionFileTransfer, error: Error?) {
        guard let id = Self.recordingID(fileTransfer.file.metadata) else { return }
        let message = error?.localizedDescription
        Task { @MainActor in self.queue?.didFinish(recordingID: id, errorMessage: message) }
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
