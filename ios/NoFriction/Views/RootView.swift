import SwiftData
import SwiftUI

/// iPhone: a tab bar. iPad: the same tabs become a sidebar
/// (.sidebarAdaptable), and Recordings becomes a list + detail split.
struct RootView: View {
    @Environment(RecordingSession.self) private var session
    @Environment(\.modelContext) private var context
    @Environment(\.scenePhase) private var scenePhase
    @Environment(RedactionCenter.self) private var redactions
    @Environment(WatchImporter.self) private var watchImporter
    @State private var tab: AppTab = .now
    @AppStorage(Onboarding.completedKey) private var onboardingCompleted = false

    enum AppTab: Hashable { case now, meetings, chat, people, settings }

    var body: some View {
        TabView(selection: $tab) {
            Tab("Record", systemImage: "waveform", value: AppTab.now) {
                LiveView()
            }
            Tab("Recordings", systemImage: "rectangle.stack", value: AppTab.meetings) {
                MeetingsView()
            }
            // Chat with your recordings (docs/TOPICS_AND_CHAT.md)
            Tab("Chat", systemImage: "bubble.left.and.text.bubble.right", value: AppTab.chat) {
                ChatView()
            }
            Tab("People", systemImage: "person.2", value: AppTab.people) {
                PeopleView()
            }
            Tab("Settings", systemImage: "gearshape", value: AppTab.settings) {
                SettingsView()
            }
        }
        .tabViewStyle(.sidebarAdaptable)
        .overlay(alignment: .bottom) {
            RedactionToast().padding(.bottom, 64)
        }
        .fullScreenCover(isPresented: Binding(get: { !onboardingCompleted }, set: { onboardingCompleted = !$0 })) {
            OnboardingView { onboardingCompleted = true }
        }
        .onAppear {
            session.attach(context)
            // Finish any Delete cut short by the app being killed in its undo window
            redactions.recover(context: context)
            #if DEBUG
            DemoData.seedIfRequested(context)
            DemoData.seedWatchDemoIfRequested(context)
            if ProcessInfo.processInfo.arguments.contains("-NFDemoLive") {
                DemoData.showLiveMeeting(session: session, context: context)
            }
            if ProcessInfo.processInfo.arguments.contains("-NFAutoRecord") {
                Task { await session.start() }
            }
            #endif
        }
        .onChange(of: session.phase) { _, phase in
            // Watch imports wait while this iPhone records; carry on after
            if phase == .idle { watchImporter.resume() }
        }
        .onChange(of: scenePhase) { _, phase in
            // Pick up meetings recorded before calendar access was granted
            if phase == .active { _ = MeetingLinker.backfill(in: context) }
            // Apple Watch recordings waiting (or cut short in the background)
            if phase == .active { watchImporter.resume() }
            // Leaving the foreground commits a pending Delete (never silently dropped)
            if phase != .active { redactions.commitPending() }
            // A meeting-end countdown follows the app off screen as a notification
            session.sceneDidChange(active: phase == .active)
        }
    }
}
