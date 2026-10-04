import AppIntents

/// "Start a recording in noFriction": Siri, the Shortcuts app, and through
/// a shortcut the Action button on Apple Watch Ultra. It opens the app first
/// (watchOS only lets recording start from the foreground), then starts, or
/// shows the one-time recording notice if it hasn't been accepted yet.
struct StartRecordingIntent: AppIntent {
    static let title: LocalizedStringResource = "Start Recording"
    static let description = IntentDescription("Opens noFriction on Apple Watch and starts recording a meeting.")
    static let openAppWhenRun = true

    @MainActor
    func perform() async throws -> some IntentResult {
        WatchAppModel.shared?.startFromIntent()
        return .result()
    }
}

struct NoFrictionWatchShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: StartRecordingIntent(),
            phrases: [
                "Start a recording in \(.applicationName)",
                "Record a meeting with \(.applicationName)",
            ],
            shortTitle: "Start Recording",
            systemImageName: "mic.fill"
        )
    }
}
