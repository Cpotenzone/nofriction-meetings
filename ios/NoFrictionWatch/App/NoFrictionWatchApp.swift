import AVFoundation
import SwiftUI
import UserNotifications
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
    /// The Record flow: what it is → how long → notebook
    var showStartFlow = false
    /// What follows the notice once it's accepted
    private var afterNotice: AfterNotice = .startFlow
    private enum AfterNotice { case startFlow, rememberedStart }
    enum Page: Hashable { case record, recordings }
    var page: Page = .record
    #if DEBUG
    /// Demo states: open the Record flow at a later step, or the Discreet controls
    var demoStartPath: [StartFlowView.Step] = []
    var demoShowDiscreetControls = false
    #endif
    /// Lost recordings found at launch (the app was killed while recording)
    private(set) var launchNotice: String?

    static let noticeKey = "recordingNoticeAccepted"
    /// The warning notification's +15 min / No limit
    private let notifications = WatchNotificationHandler()

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
        UNUserNotificationCenter.current().setNotificationCategories([WatchTimeLimitNotifier.notificationCategory])
        UNUserNotificationCenter.current().delegate = notifications
        // A warning scheduled by a run that ended has nothing to warn about
        if !recorder.machine.isActive { WatchTimeLimitNotifier.cancel() }

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

    /// The iPhone's recent notebooks (names only), for the notebook step
    var recentNotebooks: [String] { connection.recentNotebooks }

    /// The big Record button: the notice the first time, then the Record flow.
    func recordTapped() {
        page = .record
        guard !recorder.machine.isActive else { return }
        guard noticeAccepted else {
            afterNotice = .startFlow
            showNotice = true
            return
        }
        launchNotice = nil
        showStartFlow = true
    }

    /// The Record flow's last step (or its quick start): remember the
    /// choices and start.
    func start(_ options: WatchStartOptions) {
        options.remember()
        showStartFlow = false
        launchNotice = nil
        Task { await recorder.start(options) }
    }

    func noticeConfirmed() {
        noticeAccepted = true
        showNotice = false
        switch afterNotice {
        case .rememberedStart:
            launchNotice = nil
            Task { await recorder.start(.remembered()) }
        case .startFlow:
            // One sheet after another: let the notice finish closing first
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(400))
                self.showStartFlow = true
            }
        }
    }

    /// App Intent / Shortcut: the app is in the foreground (openAppWhenRun),
    /// so starting here is a user-initiated foreground start. It uses the
    /// remembered type, length and Discreet choice, with no notebook.
    func startFromIntent() {
        guard !recorder.machine.isActive else { return }
        page = .record
        showStartFlow = false
        guard noticeAccepted else {
            afterNotice = .rememberedStart
            showNotice = true
            return
        }
        launchNotice = nil
        Task { await recorder.start(.remembered()) }
    }

    func sceneChanged(active: Bool) {
        recorder.meterVisible = active
        if active {
            // Back from the background: catch up on the time limit
            recorder.tickLimit()
            connection.activate()
            queue.sendPending()
        }
    }

    func retryNow() { queue.sendPending() }

    func delete(_ entry: WatchRecordingEntry) {
        // Never pull a file out from under an in-flight transfer or the
        // recorder, or delete half of what the iPhone already has
        guard entry.canDelete else { return }
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

/// The time-limit warning notification: shown by the system while the app
/// is in the background; in the foreground the app shows its own warning
/// (and its own, quieter haptic in Discreet), so the banner is suppressed.
final class WatchNotificationHandler: NSObject, UNUserNotificationCenterDelegate {
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async
        -> UNNotificationPresentationOptions {
        notification.request.content.categoryIdentifier == WatchTimeLimitNotifier.category ? [] : [.banner, .sound]
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let action = response.actionIdentifier
        await MainActor.run {
            guard let recorder = WatchAppModel.shared?.recorder else { return }
            switch action {
            case WatchTimeLimitNotifier.extendAction: recorder.extendLimit()
            case WatchTimeLimitNotifier.noLimitAction: recorder.removeLimit()
            default: break
            }
        }
    }
}

enum WatchTheme {
    static let accent = Color(red: 250 / 255, green: 204 / 255, blue: 21 / 255)   // hazard yellow, as on iPhone
    static let recording = Color(red: 0.94, green: 0.27, blue: 0.27)
    static let recordingStrong = Color(red: 0.80, green: 0.16, blue: 0.16)
    static let delivered = Color(red: 0.30, green: 0.80, blue: 0.45)
}
