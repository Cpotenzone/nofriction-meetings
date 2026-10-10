import Foundation
import Observation
import SwiftData
import SwiftUI
import UIKit

/// Sync with your Mac, for the UI and the triggers (docs/SYNC.md). The
/// iPhone starts every session: when the app comes to the foreground, when a
/// recording stops, and on Sync now. Pro only.
@MainActor
@Observable
final class SyncCenter {
    enum Status: Equatable {
        case idle
        case syncing(String)
        case failed(String)
    }

    private(set) var macs: [SyncMacState] = []
    private(set) var status: Status = .idle
    @ObservationIgnored private let context: ModelContext
    @ObservationIgnored private let isPro: () -> Bool
    @ObservationIgnored private weak var redactions: RedactionCenter?
    @ObservationIgnored private weak var screenCapture: ScreenCaptureCenter?
    @ObservationIgnored private var lastAutomatic: Date?

    init(context: ModelContext, isPro: @escaping () -> Bool, redactions: RedactionCenter?, screenCapture: ScreenCaptureCenter?) {
        self.context = context
        self.isPro = isPro
        self.redactions = redactions
        self.screenCapture = screenCapture
        reload()
    }

    var isPaired: Bool { !macs.isEmpty }
    var isSyncing: Bool { if case .syncing = status { return true }; return false }
    var lastSyncedAt: Date? { macs.compactMap(\.lastSyncedAt).max() }

    func reload() { macs = SyncStore.all() }

    /// Foreground or a recording stopped: sync quietly (at most every 30 s).
    func syncAutomatically() {
        guard isPro(), isPaired, !isSyncing else { return }
        if let last = lastAutomatic, Date.now.timeIntervalSince(last) < 30 { return }
        lastAutomatic = .now
        Task { await syncNow() }
    }

    func syncNow() async {
        guard isPro() else { status = .failed(SyncError.notPro.localizedDescription); return }
        guard !isSyncing else { return }
        // A Delete still in its undo window is committed first, so what the
        // Mac gets is final
        await redactions?.flush()
        var failure: String?
        for var state in SyncStore.all() {
            status = .syncing(state.name)
            do {
                _ = try await SyncSession.run(&state, engine: { key in
                    SyncEngine(context: self.context, tokenKey: key, redactions: self.redactions, screenCapture: self.screenCapture)
                })
            } catch {
                let message = (error as? LocalizedError)?.errorDescription ?? error.localizedDescription
                state.lastError = message
                SyncStore.save(state)
                failure = message
            }
        }
        reload()
        status = failure.map(Status.failed) ?? .idle
    }

    func pair(_ raw: String) async throws {
        guard isPro() else { throw SyncError.notPro }
        guard let link = PairingLink(raw) else {
            throw SyncError.protocolError("that isn't a noFriction pairing code. On your Mac: Settings → Sync → Pair a device.")
        }
        status = .syncing(link.name)
        defer { if case .syncing = status { status = .idle } }
        _ = try await SyncSession.pair(link, deviceName: UIDevice.current.name)
        reload()
        await syncNow()
    }

    func forget(_ macID: String) {
        SyncStore.forget(macID)
        reload()
    }
}

/// Triggers: foreground, and a recording that just stopped.
struct SyncTriggers: ViewModifier {
    let center: SyncCenter
    let session: RecordingSession
    @Environment(\.scenePhase) private var scenePhase

    func body(content: Content) -> some View {
        content
            .onChange(of: scenePhase) { _, phase in
                if phase == .active { center.syncAutomatically() }
            }
            .onChange(of: session.phase) { old, new in
                if old == .stopping && new == .idle { center.syncAutomatically() }
            }
    }
}
