import Foundation
import Observation
import SwiftData
import UIKit

/// App-wide owner of Delete's 5-second undo window and of the purge queue.
/// Purges run one at a time (they may rewrite the same audio file).
///
/// A pending Delete is committed when: 5 seconds pass, another edit starts,
/// the app leaves the foreground, or the app is about to terminate. If the
/// process dies anyway, the persisted pending record is finished at next
/// launch (`recover`), so a Delete is never silently dropped.
@MainActor
@Observable
final class RedactionCenter {
    static let undoWindow: Duration = .seconds(5)

    /// The Delete that can still be undone (drives the toast)
    private(set) var pending: RedactionEngine.PendingDelete?
    /// Something went wrong committing; shown in the toast
    var errorMessage: String?

    @ObservationIgnored private var pendingContext: ModelContext?
    @ObservationIgnored private var timer: Task<Void, Never>?
    @ObservationIgnored private var tail: Task<Void, Never>?
    @ObservationIgnored private var observers: [NSObjectProtocol] = []

    init() {
        let center = NotificationCenter.default
        for name in [UIApplication.willResignActiveNotification, UIApplication.didEnterBackgroundNotification,
                     UIApplication.willTerminateNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.commitPending() }
            })
        }
    }

    // MARK: Actions

    func delete(_ target: EditTarget, in meeting: Meeting, context: ModelContext) {
        commitPending()
        do {
            let p = try RedactionEngine.delete(target, meeting: meeting, context: context)
            pending = p
            pendingContext = context
            errorMessage = nil
            timer = Task { [weak self] in
                try? await Task.sleep(for: Self.undoWindow)
                guard !Task.isCancelled else { return }
                self?.commitPending()
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func undo() {
        guard let p = pending, let context = pendingContext else { return }
        timer?.cancel()
        pending = nil
        pendingContext = nil
        do { try RedactionEngine.undo(p, context: context) } catch { errorMessage = error.localizedDescription }
    }

    /// Strike from the record. Waits for any queued purge first.
    @discardableResult
    func strike(_ target: EditTarget, reason: String?, in meeting: Meeting, context: ModelContext) async throws -> RedactionEngine.StrikeResult {
        commitPending()
        await tail?.value
        return try await RedactionEngine.strike(target, reason: reason, meeting: meeting, context: context)
    }

    /// Commit now (no-op if nothing is pending).
    func commitPending() {
        guard let p = pending, let context = pendingContext else { return }
        timer?.cancel()
        pending = nil
        pendingContext = nil
        enqueue { [weak self] in
            do {
                try await RedactionEngine.commit(p, context: context)
            } catch {
                self?.errorMessage = "Delete saved, but the purge didn't finish (\(error.localizedDescription)). It will retry next launch."
            }
        }
    }

    /// Finish Deletes interrupted by a crash or kill.
    func recover(context: ModelContext) {
        enqueue { await RedactionEngine.recover(context: context) }
    }

    /// Drop a pending Delete whose meeting is being deleted outright.
    func discardPending(for meeting: Meeting) {
        guard pending?.meeting.id == meeting.id else { return }
        timer?.cancel()
        pending = nil
        pendingContext = nil
    }

    /// Wait for queued purges (tests, strike).
    func flush() async {
        commitPending()
        await tail?.value
    }

    private func enqueue(_ work: @escaping @MainActor () async -> Void) {
        let previous = tail
        // Keep running briefly if the app is backgrounded mid-purge
        let bg = UIApplication.shared.beginBackgroundTask(withName: "redaction-purge")
        tail = Task { @MainActor in
            await previous?.value
            await work()
            if bg != .invalid { UIApplication.shared.endBackgroundTask(bg) }
        }
    }
}
