import SwiftData
import SwiftUI

@main
struct NoFrictionApp: App {
    @State private var session = RecordingSession()
    /// Created at launch so the StoreKit Transaction.updates listener starts immediately.
    @State private var store = Store()
    @State private var aiSettings = AISettings()
    /// Delete's undo window + the purge queue (docs/REDACTION.md)
    @State private var redactions = RedactionCenter()

    init() {
        Storage.prepare()
        // Meeting-end prompt actions can arrive while recording in the background
        MeetingEndNotifier.shared.install()
        #if DEBUG
        // UI tests: -NFResetOnboarding starts from a first launch;
        // demo / auto-record runs skip the welcome
        let args = ProcessInfo.processInfo.arguments
        if args.contains("-NFResetOnboarding") {
            UserDefaults.standard.removeObject(forKey: Onboarding.completedKey)
            UserDefaults.standard.removeObject(forKey: "recordingNoticeAccepted")
        } else if args.contains("-NFSkipOnboarding") || args.contains("-NFSeedDemo") || args.contains("-NFAutoRecord") {
            UserDefaults.standard.set(true, forKey: Onboarding.completedKey)
        }
        #endif
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(session)
                .environment(store)
                .environment(aiSettings)
                .environment(redactions)
                .preferredColorScheme(.dark)
                .tint(Theme.accent)
        }
        .modelContainer(for: Storage.modelTypes)
    }
}

enum Theme {
    static let accent = Color(red: 250 / 255, green: 204 / 255, blue: 21 / 255)   // hazard yellow
    static let background = Color(red: 0.035, green: 0.035, blue: 0.04)
    static let card = Color(red: 0.085, green: 0.085, blue: 0.095)
    static let hairline = Color.white.opacity(0.08)
    static let recording = Color(red: 0.94, green: 0.27, blue: 0.27)
    /// Filled red behind white text (5.4:1; `recording` is 3.7:1)
    static let recordingStrong = Color(red: 0.80, green: 0.16, blue: 0.16)
    static let linkedIn = Color(red: 10 / 255, green: 102 / 255, blue: 194 / 255)
    /// AI features: majolica cobalt from the Trinacria icon, lifted for the dark background
    static let ai = Color(red: 90 / 255, green: 140 / 255, blue: 230 / 255)
}

extension TimeInterval {
    /// 75 → "1:15", 3725 → "1:02:05"
    var clock: String {
        let s = Int(self.rounded(.down))
        return s >= 3600
            ? String(format: "%d:%02d:%02d", s / 3600, (s % 3600) / 60, s % 60)
            : String(format: "%d:%02d", s / 60, s % 60)
    }

    /// 2700 → "45 min"
    var minutesLabel: String {
        let m = Int((self / 60).rounded())
        return m >= 60 ? "\(m / 60) h \(m % 60) min" : "\(max(m, 1)) min"
    }
}
