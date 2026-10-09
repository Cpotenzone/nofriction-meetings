import AVFoundation
import EventKit
import Speech
import SwiftData
import SwiftUI

/// First-run state. `completed` is set when the welcome flow is finished or
/// skipped; Settings → "Show welcome again" clears it.
enum Onboarding {
    static let completedKey = "onboardingCompleted"
}

// MARK: - Permissions

/// The permissions asked for up front, each with its reason. Notifications
/// are asked in context instead, the first time a timed recording is about
/// to end (`RecordingSession`), never here.
@MainActor
@Observable
final class PermissionsModel {
    enum Kind: String, CaseIterable, Identifiable {
        case microphone, speech, calendar
        var id: String { rawValue }

        var title: String {
            switch self {
            case .microphone: "Microphone"
            case .speech: "Speech recognition"
            case .calendar: "Calendar"
            }
        }

        var reason: String {
            switch self {
            case .microphone: "To record. Audio stays on this device."
            case .speech: "To turn the recording into text, on this device."
            case .calendar: "To name each recording and list who attended. Read only."
            }
        }

        var systemImage: String {
            switch self {
            case .microphone: "mic.fill"
            case .speech: "text.bubble.fill"
            case .calendar: "calendar"
            }
        }
    }

    enum State: Equatable { case notAsked, granted, denied }

    private(set) var states: [Kind: State] = [:]

    func state(_ kind: Kind) -> State { states[kind] ?? .notAsked }

    func refresh() async {
        states[.microphone] = switch AVAudioApplication.shared.recordPermission {
        case .granted: .granted
        case .denied: .denied
        default: .notAsked
        }
        states[.speech] = switch SFSpeechRecognizer.authorizationStatus() {
        case .authorized: .granted
        case .notDetermined: .notAsked
        default: .denied
        }
        states[.calendar] = switch EKEventStore.authorizationStatus(for: .event) {
        case .fullAccess: .granted
        case .notDetermined: .notAsked
        default: .denied
        }
    }

    func request(_ kind: Kind) async {
        switch kind {
        case .microphone:
            _ = await AudioCapture.requestPermission()
        case .speech:
            _ = await withCheckedContinuation { (cont: CheckedContinuation<SFSpeechRecognizerAuthorizationStatus, Never>) in
                SFSpeechRecognizer.requestAuthorization { cont.resume(returning: $0) }
            }
        case .calendar:
            _ = await CalendarService.shared.requestAccess()
        }
        await refresh()
    }
}

// MARK: - Flow

/// Welcome → permissions, then the Record screen. Both steps can be skipped.
/// The recording notice is shown once, on the first tap of Record; AI setup
/// and Pro appear the first time they are needed (Make notes, Chat, Review).
struct OnboardingView: View {
    let onFinish: () -> Void

    enum Step: Int, CaseIterable { case welcome, permissions }

    @State private var step: Step = .welcome
    @State private var permissions = PermissionsModel()
    @Environment(\.modelContext) private var context
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(spacing: 0) {
            topBar
            Group {
                switch step {
                case .welcome: welcome
                case .permissions: permissionsPage
                }
            }
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .transition(.asymmetric(insertion: .move(edge: .trailing).combined(with: .opacity),
                                    removal: .move(edge: .leading).combined(with: .opacity)))
            .id(step)
        }
        .background(Theme.background.ignoresSafeArea())
        .tint(Theme.accent)
        .preferredColorScheme(.dark)
        .task { await permissions.refresh() }
    }

    private func next() {
        guard let n = Step(rawValue: step.rawValue + 1) else { return onFinish() }
        withAnimation(.easeInOut(duration: 0.25)) { step = n }
    }

    private func back() {
        guard let p = Step(rawValue: step.rawValue - 1) else { return }
        withAnimation(.easeInOut(duration: 0.25)) { step = p }
    }

    // MARK: Chrome

    private var topBar: some View {
        HStack {
            if step != .welcome {
                Button { back() } label: {
                    Image(systemName: "chevron.left").font(.body.weight(.semibold))
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel("Back")
                .accessibilityIdentifier("onboarding-back")
            }
            Spacer()
            HStack(spacing: 6) {
                ForEach(Step.allCases, id: \.self) { s in
                    Capsule()
                        .fill(s.rawValue <= step.rawValue ? Theme.accent : Color.white.opacity(0.18))
                        .frame(width: s == step ? 18 : 6, height: 6)
                }
            }
            .accessibilityElement()
            .accessibilityLabel("Step \(step.rawValue + 1) of \(Step.allCases.count)")
            Spacer()
            Button("Skip") { onFinish() }
                .font(.body.weight(.medium))
                .frame(minHeight: 44)
                .accessibilityHint("Closes the welcome. You can open it again from Settings.")
                .accessibilityIdentifier("onboarding-skip")
        }
        .padding(.horizontal, 16)
        .frame(minHeight: 52)
    }

    /// The page body scrolls (large text never pushes the button off screen);
    /// the primary action stays pinned at the bottom.
    private func page<Content: View, Actions: View>(
        @ViewBuilder _ content: () -> Content, @ViewBuilder actions: () -> Actions
    ) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) { content() }
                .padding(.horizontal, 24)
                .padding(.vertical, 12)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollBounceBehavior(.basedOnSize)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 10) { actions() }
                .padding(.horizontal, 24)
                .padding(.top, 12)
                .padding(.bottom, 16)
                .background(Theme.background)
        }
    }

    private func primary(_ title: String, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title).font(.headline).frame(maxWidth: .infinity).padding(.vertical, 8)
        }
        .buttonStyle(.borderedProminent)
        .foregroundStyle(.black)   // black on hazard yellow: 13.7:1 (white would be 1.5:1)
        .accessibilityIdentifier(id)
    }

    // MARK: Pages

    private var welcome: some View {
        page {
            Image("Logo")
                .resizable()
                .scaledToFit()
                .frame(width: 96, height: 96)
                .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                .accessibilityHidden(true)
                .padding(.top, 8)
            Text("Welcome to noFriction")
                .font(.largeTitle.weight(.bold))
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("onboarding-welcome")
            Text("Record a meeting, a class or anything else, and get a transcript you can search, edit and share.")
                .font(.title3)
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 16) {
                feature("waveform", "Transcribed on this device", "Recording and transcription stay on your iPhone or iPad.")
                feature("calendar", "Matched to your calendar", "Each recording is named after its event, with who attended.")
                feature("eye.slash", "Strike from the record", "Remove words, lines or photos for good, audio included.")
                feature("sparkles", "Notes, your way", "Apple on-device or your own AI endpoint, when you want notes.")
            }
            .padding(.top, 6)
        } actions: {
            primary("Continue", id: "onboarding-continue") { next() }
        }
    }

    private func feature(_ icon: String, _ title: String, _ detail: String) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: icon)
                .font(.title3)
                .foregroundStyle(Theme.accent)
                .frame(width: 30)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.headline)
                Text(detail).font(.subheadline).foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var permissionsPage: some View {
        page {
            Text("Permissions")
                .font(.largeTitle.weight(.bold))
                .accessibilityAddTraits(.isHeader)
                .padding(.top, 8)
            Text("Allow what you'd like now. iOS asks once for each; you can change them later in the Settings app.")
                .foregroundStyle(.secondary)
            VStack(spacing: 12) {
                ForEach(PermissionsModel.Kind.allCases) { kind in
                    permissionRow(kind)
                }
            }
        } actions: {
            primary("Get started", id: "onboarding-finish") { onFinish() }
        }
    }

    private func permissionRow(_ kind: PermissionsModel.Kind) -> some View {
        let state = permissions.state(kind)
        return HStack(alignment: .center, spacing: 14) {
            Image(systemName: kind.systemImage)
                .font(.title3)
                .foregroundStyle(Theme.accent)
                .frame(width: 30)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(kind.title).font(.headline)
                Text(kind.reason).font(.subheadline).foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            switch state {
            case .granted:
                Image(systemName: "checkmark.circle.fill")
                    .font(.title2)
                    .foregroundStyle(.green)
                    .accessibilityLabel("Allowed")
            case .denied:
                Button("Settings") {
                    if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) }
                }
                .buttonStyle(.bordered)
                .accessibilityLabel("\(kind.title) is off. Open Settings")
            case .notAsked:
                Button("Allow") {
                    Task {
                        await permissions.request(kind)
                        if kind == .calendar, permissions.state(.calendar) == .granted {
                            _ = MeetingLinker.backfill(in: context)
                        }
                    }
                }
                .buttonStyle(.borderedProminent)
                .foregroundStyle(.black)
                .accessibilityLabel("Allow \(kind.title)")
                .accessibilityHint(kind.reason)
            }
        }
        .padding(14)
        .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("permission-\(kind.rawValue)")
    }
}
