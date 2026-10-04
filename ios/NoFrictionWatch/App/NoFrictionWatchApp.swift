import AVFoundation
import SwiftUI
import WatchConnectivity

@main
struct NoFrictionWatchApp: App {
    @State private var model = WatchAppModel()
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup {
            WatchRootView()
                .environment(model)
                .tint(WatchTheme.accent)
        }
        .onChange(of: scenePhase) { _, phase in
            model.sceneChanged(active: phase == .active)
        }
        // WatchConnectivity wakes the app to report finished transfers
        .backgroundTask(.watchConnectivity) {
            await model.handleConnectivityTask()
        }
    }
}

/// Owns the store, the connection and the recorder for the app's lifetime.
@MainActor
@Observable
final class WatchAppModel {
    /// For the App Intent (Start recording), which runs in this process
    static weak var shared: WatchAppModel?

    let store: WatchRecordingStore
    let connection: WatchConnection
    let queue: WatchTransferQueue
    let recorder: WatchRecorder
    /// Mirrors the store for SwiftUI
    private(set) var recordings: [WatchRecordingEntry] = []
    /// One-time notice before the first recording (same text as the iPhone)
    var noticeAccepted: Bool {
        didSet { UserDefaults.standard.set(noticeAccepted, forKey: Self.noticeKey) }
    }
    /// Show the notice, then start (first use, or started by the App Intent before it was accepted)
    var showNotice = false
    enum Page: Hashable { case record, recordings }
    var page: Page = .record
    /// Lost recordings found at launch (the app was killed while recording)
    private(set) var launchNotice: String?

    static let noticeKey = "recordingNoticeAccepted"

    init() {
        #if DEBUG
        let demo = DemoMode.current
        let store = demo != nil ? DemoMode.makeStore() : WatchRecordingStore()
        #else
        let store = WatchRecordingStore()
        #endif
        self.store = store
        connection = WatchConnection()
        queue = WatchTransferQueue(store: store, transport: connection)
        recorder = WatchRecorder(store: store, queue: queue)
        noticeAccepted = UserDefaults.standard.bool(forKey: Self.noticeKey)
        connection.queue = queue
        store.onChange = { [weak self] in self?.refresh() }

        let (_, lost) = store.recoverInterrupted(appVersion: WatchRecorder.appVersion, audioLength: WatchRecorder.audioLength(of:))
        if lost > 0 {
            launchNotice = "The app closed during a recording and its audio couldn't be saved."
        }
        store.prune()
        refresh()
        Self.shared = self

        #if DEBUG
        if let demo {
            DemoMode.apply(demo, to: self)
            return
        }
        #endif
        connection.activate()
        #if DEBUG
        if ProcessInfo.processInfo.arguments.contains("-NFWatchSendTestRecording") {
            DemoMode.queueSyntheticRecording(self)
        }
        if ProcessInfo.processInfo.arguments.contains("-NFWatchAutoRecord") {
            DemoMode.autoRecord(self)
        }
        #endif
    }

    func refresh() { recordings = store.recent }

    /// The big Record button.
    func recordTapped() {
        page = .record
        guard noticeAccepted else {
            showNotice = true
            return
        }
        launchNotice = nil
        Task { await recorder.start() }
    }

    func noticeConfirmed() {
        noticeAccepted = true
        showNotice = false
        recordTapped()
    }

    /// App Intent / Shortcut: the app is in the foreground (openAppWhenRun),
    /// so starting here is a user-initiated foreground start.
    func startFromIntent() {
        guard !recorder.machine.isActive else { return }
        recordTapped()
    }

    func sceneChanged(active: Bool) {
        recorder.meterVisible = active
        if active {
            connection.activate()
            queue.sendPending()
        }
    }

    func retryNow() { queue.sendPending() }

    func delete(_ entry: WatchRecordingEntry) {
        // Never pull a file out from under an in-flight transfer or the recorder
        guard entry.status != .sending, entry.status != .recording else { return }
        store.discard(entry.id)
    }

    /// Background WatchConnectivity task: activate and give pending
    /// deliveries a few seconds to arrive.
    func handleConnectivityTask() async {
        connection.activate()
        for _ in 0..<20 where connection.hasContentPending {
            try? await Task.sleep(for: .milliseconds(500))
        }
    }
}

enum WatchTheme {
    static let accent = Color(red: 250 / 255, green: 204 / 255, blue: 21 / 255)   // hazard yellow, as on iPhone
    static let recording = Color(red: 0.94, green: 0.27, blue: 0.27)
    static let recordingStrong = Color(red: 0.80, green: 0.16, blue: 0.16)
    static let delivered = Color(red: 0.30, green: 0.80, blue: 0.45)
}
