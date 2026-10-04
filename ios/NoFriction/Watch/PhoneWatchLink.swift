import Foundation
import Observation
import UIKit
import WatchConnectivity

/// The iPhone end of WatchConnectivity. Activated at launch (App.init),
/// because the system may launch the app in the background to hand over a
/// recording. Receives files; sends the watch only acknowledgments (part
/// ids), never audio, text or settings.
@MainActor
@Observable
final class PhoneWatchLink: NSObject {
    static let shared = PhoneWatchLink()

    /// WatchConnectivity exists on this device (not on iPad)
    let isSupported = WCSession.isSupported()
    private(set) var activated = false
    private(set) var isPaired = false
    private(set) var isWatchAppInstalled = false
    /// Recordings partly received (a paused recording whose other parts are
    /// still on their way) or not yet imported
    private(set) var arrivingCount = 0

    @ObservationIgnored var importer: WatchImporter?

    func activate() {
        guard isSupported, WCSession.default.delegate == nil else { return }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    /// A recording was staged in the inbox: import it, and transcribe while
    /// the system gives us time. Anything left resumes in the foreground.
    func received() {
        guard let importer else { return }
        let box = BackgroundTaskBox()
        box.id = UIApplication.shared.beginBackgroundTask(withName: "Import Apple Watch recording") { [weak importer] in
            importer?.cancel()
            box.end()
        }
        Task {
            await importer.processInbox()
            self.refreshInbox()
            importer.resume { box.end() }
        }
    }

    func refreshInbox() {
        arrivingCount = WatchInbox.shared.recordingCount
    }

    private func refresh(_ state: SessionState) {
        activated = state.activated
        isPaired = state.paired
        isWatchAppInstalled = state.installed
    }

    fileprivate struct SessionState: Sendable {
        var activated: Bool
        var paired: Bool
        var installed: Bool

        init(_ session: WCSession) {
            activated = session.activationState == .activated
            paired = activated && session.isPaired
            installed = activated && session.isWatchAppInstalled
        }
    }

    /// Ends a UIKit background task exactly once.
    private final class BackgroundTaskBox {
        var id: UIBackgroundTaskIdentifier = .invalid
        @MainActor func end() {
            guard id != .invalid else { return }
            UIApplication.shared.endBackgroundTask(id)
            id = .invalid
        }
    }
}

extension PhoneWatchLink: WCSessionDelegate {
    nonisolated func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        let state = SessionState(session)
        Task { @MainActor in self.refresh(state) }
    }

    nonisolated func sessionDidBecomeInactive(_ session: WCSession) {}

    nonisolated func sessionDidDeactivate(_ session: WCSession) {
        // The user switched watches: reactivate for the new one
        session.activate()
    }

    nonisolated func sessionWatchStateDidChange(_ session: WCSession) {
        let state = SessionState(session)
        Task { @MainActor in self.refresh(state) }
    }

    /// The system deletes `file.fileURL` when this returns, so it is moved
    /// into the inbox right here, on WatchConnectivity's queue. Once it is
    /// stored, the watch is told (`WatchTransfer.Key.ack`) and only then
    /// deletes its copy. Nothing is acknowledged that wasn't stored, so the
    /// watch keeps anything the iPhone couldn't take and sends it again.
    nonisolated func session(_ session: WCSession, didReceive file: WCSessionFile) {
        guard let metadata = WatchRecordingMetadata(dictionary: file.metadata ?? [:]) else { return }
        do {
            try WatchInbox.shared.stage(file.fileURL, metadata: metadata)
        } catch {
            return
        }
        session.transferUserInfo([WatchTransfer.Key.ack: [WatchTransfer.partKey(metadata.recordingID, metadata.part)]])
        Task { @MainActor in self.received() }
    }
}
