import SwiftData
import StoreKit
import SwiftUI
import UIKit

// MARK: - Endpoint setup

/// Store explicit routing input locally. No key detection, validation probe or model discovery.
@MainActor
@Observable
final class AIConnectModel {
    enum Status: Equatable {
        case idle
        case connected(String)
        case failed(String)
    }

    var serverURL = ""
    var serverModel = ""
    var serverKey = ""
    var status: Status = .idle
    var consentPrompt: AIProvider?

    var canSave: Bool {
        !serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
        !serverModel.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    func pasteFromClipboard() {
        if let s = UIPasteboard.general.string { serverKey = KeyDetector.normalize(s) }
    }

    func saveEndpoint(_ settings: AISettings) {
        guard let url = URLPolicy.parseBaseURL(serverURL) else { status = .failed("Enter your endpoint's base URL."); return }
        guard URLPolicy.isAllowed(url) else { status = .failed(AIError.insecureURL.localizedDescription); return }
        let model = serverModel.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !model.isEmpty else { status = .failed("Enter the model ID supplied by your endpoint."); return }
        let key = KeyDetector.normalize(serverKey)
        do {
            try settings.save(.custom, key: key.isEmpty ? nil : key, baseURL: url,
                              models: [ModelInfo(id: model, contextTokens: nil)])
            serverKey = ""
            status = .connected("Endpoint saved. It will be used when you request AI notes or a follow-up.")
            if !settings.hasConsent(.custom) { consentPrompt = .custom }
        } catch {
            status = .failed(Redactor.redact(error.localizedDescription, secrets: [key]))
        }
    }
}

struct AIEndpointSection: View {
    @Environment(AISettings.self) private var settings
    @Bindable var model: AIConnectModel

    var body: some View {
        Section {
            TextField("Base URL", text: $model.serverURL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .accessibilityIdentifier("ai-endpoint-url")
            TextField("Model ID", text: $model.serverModel)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("ai-model-id")
            SecureField("API key (optional)", text: $model.serverKey)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .privacySensitive()
                .accessibilityLabel("API key, optional")
                .accessibilityIdentifier("api-key-field")
            HStack(spacing: 10) {
                Button("Paste key", systemImage: "doc.on.clipboard") { model.pasteFromClipboard() }
                    .buttonStyle(.bordered)
                Button("Save endpoint", systemImage: "checkmark") { model.saveEndpoint(settings) }
                    .buttonStyle(.borderedProminent)
                    .foregroundStyle(.black)
                    .disabled(!model.canSave)
                    .accessibilityIdentifier("save-ai-endpoint")
            }
            StatusLine(status: model.status)
        } header: {
            Text("Your AI endpoint")
        } footer: {
            Text("Enter a base URL and model ID for your own OpenAI-compatible endpoint. The key is optional and stored only in this device's Keychain. Saving makes no network request. Remote endpoints need HTTPS; HTTP is allowed only on your device or private network.")
        }
    }
}

private struct StatusLine: View {
    let status: AIConnectModel.Status
    var body: some View {
        switch status {
        case .idle: EmptyView()
        case .connected(let s):
            Label(s, systemImage: "checkmark.circle.fill").font(.footnote).foregroundStyle(.green)
                .accessibilityIdentifier("connect-status")
        case .failed(let s):
            Label(s, systemImage: "exclamationmark.triangle.fill").font(.footnote).foregroundStyle(.orange)
                .accessibilityIdentifier("connect-status")
        }
    }
}

// MARK: - Setup sheet (from a meeting, when no provider is configured)

struct AISetupSheet: View {
    @Environment(AISettings.self) private var settings
    @Environment(\.dismiss) private var dismiss
    @State private var model = AIConnectModel()

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text("Use Apple on-device when available, or enter your own AI endpoint and model. noFriction provides no hosted models.")
                        .font(.callout)
                }
                AIEndpointSection(model: model)
                if AppleOnDevice.isAvailable {
                    Section {
                        Button("Use Apple on-device (no key)") {
                            settings.useApple()
                            dismiss()
                        }
                    } footer: {
                        Text("Runs on this iPhone. Nothing leaves the device.")
                    }
                }
            }
            .navigationTitle("Set up AI")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                if settings.isConfigured {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
            }
            .sheet(item: $model.consentPrompt, onDismiss: { if settings.isConfigured { dismiss() } }) { p in
                AIConsentSheet(provider: p) { settings.grantConsent(p) }
            }
        }
    }
}

// MARK: - Settings tab

struct SettingsView: View {
    @Environment(AISettings.self) private var settings
    @Environment(Store.self) private var store
    @State private var connect = AIConnectModel()
    @State private var showPaywall = false
    @State private var showManage = false
    @State private var restoreMessage: String?
    @State private var restoring = false
    @State private var customModel = ""
    @AppStorage(MeetingEndDetector.Config.enabledKey) private var autoStop = true
    @AppStorage(MeetingEndDetector.Config.minutesKey) private var autoStopMinutes = 3
    @AppStorage(Onboarding.completedKey) private var onboardingCompleted = false

    var body: some View {
        NavigationStack {
            Form {
                activeSection
                AIEndpointSection(model: connect)
                savedSection
                recordingSection
                AppleWatchSection()
                privacySection
                subscriptionSection
                aboutSection
            }
            .scrollContentBackground(.hidden)
            .background(Theme.background)
            .navigationTitle("Settings")
            .sheet(isPresented: $showPaywall) { PaywallView() }
            .manageSubscriptionsSheet(isPresented: $showManage)
            .sheet(item: $connect.consentPrompt) { p in
                AIConsentSheet(provider: p) { settings.grantConsent(p) }
            }
        }
    }

    // MARK: Recording

    private var recordingSection: some View {
        Section {
            Toggle("Stop automatically when the meeting ends", isOn: $autoStop)
                .accessibilityIdentifier("auto-stop-toggle")
            if autoStop {
                Stepper(value: $autoStopMinutes, in: MeetingEndDetector.Config.minutesRange) {
                    LabeledContent("After silence of", value: "\(autoStopMinutes) min")
                }
            }
        } header: {
            Text("Recording")
        } footer: {
            Text("When the calendar event is over and the room goes quiet, or nobody has spoken for this long, noFriction asks, then stops after 30 seconds unless you keep recording. Everything said is kept.")
        }
    }

    // MARK: AI

    @ViewBuilder private var activeSection: some View {
        Section("AI provider") {
            if let p = settings.effectiveProvider {
                LabeledContent("Active", value: p.name)
                if p == .apple {
                    LabeledContent("Model", value: "Apple on-device")
                } else if let saved = settings.saved[p.id] {
                    LabeledContent("Model", value: saved.model)
                    HStack {
                        TextField("New model ID", text: $customModel)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        Button("Use") {
                            settings.setModel(customModel, for: p)
                            customModel = ""
                        }
                        .disabled(customModel.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    }
                    if let url = settings.baseURL(for: p) {
                        LabeledContent("Server", value: url.absoluteString)
                    }
                }
                Text(whatLeaves(p))
                    .font(.footnote).foregroundStyle(.secondary)
            } else {
                Text(settings.needsEndpointSetup ? "Configure AI: the previous provider is no longer available. Your meetings are unchanged. Choose Apple on-device when available, or enter your endpoint and model below." : "Choose Apple on-device when available, or enter your endpoint and model below to use AI notes and follow-ups.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder private var savedSection: some View {
        let list = settings.savedProviders
        if !list.isEmpty || AppleOnDevice.isAvailable {
            Section {
                ForEach(list) { p in
                    Button {
                        settings.setActive(p)
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(p.name).foregroundStyle(.primary)
                                Text(detail(p)).font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            if settings.effectiveProvider == p {
                                Image(systemName: "checkmark").foregroundStyle(Theme.ai)
                                    .accessibilityHidden(true)
                            }
                        }
                    }
                    .accessibilityAddTraits(settings.effectiveProvider == p ? .isSelected : [])
                    .accessibilityHint("Makes this the active AI provider")
                    .swipeActions {
                        Button("Delete", role: .destructive) { settings.remove(p) }
                    }
                    .contextMenu {
                        Button("Delete connection", systemImage: "trash", role: .destructive) { settings.remove(p) }
                    }
                }
                if AppleOnDevice.isAvailable && settings.saved[AIProvider.apple.id] == nil {
                    Button("Use Apple on-device (no key)") { settings.useApple() }
                }
            } header: {
                Text("Saved connections")
            } footer: {
                Text("Tap to make active. Swipe left to delete a connection and its key.")
            }
        }
    }

    @ViewBuilder private var privacySection: some View {
        Section {
            Text("Audio, photos and transcripts are stored only on this device. Transcription runs on the device. Local and Apple on-device models support offline AI after setup. If you choose a remote AI endpoint, the meeting's text goes directly to that endpoint under its operator's terms. noFriction offers no hosted models and receives none of this content.")
                .font(.footnote)
            ForEach(AIProvider.all.filter { settings.consented.contains($0.id) }) { p in
                HStack {
                    Text("Sharing allowed: \(settings.baseURL(for: p)?.host() ?? p.name)")
                    Spacer()
                    Button("Revoke", role: .destructive) { settings.revokeConsent(p) }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Revoke sharing with \(p.name)")
                }
            }
        } header: {
            Text("What leaves this device")
        }
    }

    // MARK: Subscription

    private var subscriptionSection: some View {
        Section("Subscription") {
            LabeledContent("Status", value: store.isPro ? "noFriction Pro" : "Free")
            if !store.isPro {
                Button("Upgrade to Pro") { showPaywall = true }
            } else {
                Button("Manage Subscription") { showManage = true }
            }
            Button {
                restoring = true
                restoreMessage = nil
                Task {
                    defer { restoring = false }
                    do {
                        try await store.restore()
                        restoreMessage = store.isPro ? "Pro restored." : "No active subscription found."
                    } catch {
                        restoreMessage = "Couldn't restore: \(error.localizedDescription)"
                    }
                }
            } label: {
                HStack {
                    Text("Restore Purchases")
                    if restoring { Spacer(); ProgressView() }
                }
            }
            .disabled(restoring)
            if let restoreMessage {
                Text(restoreMessage).font(.footnote).foregroundStyle(.secondary)
            }
        }
    }

    private var aboutSection: some View {
        Section("About") {
            Link("Privacy Policy", destination: AppLinks.privacyPolicy)
            Link("Terms of Use (EULA)", destination: AppLinks.terms)
            Link("Contact Support (\(AppLinks.supportEmail))", destination: AppLinks.supportMail)
            Button("Show welcome again") { onboardingCompleted = false }
                .accessibilityIdentifier("show-welcome")
            LabeledContent("Version", value: AppLinks.versionString)
        }
    }

    // MARK: Text

    private func detail(_ p: AIProvider) -> String {
        guard let s = settings.saved[p.id] else { return "" }
        var parts: [String] = []
        if let l = s.last4 { parts.append("••••\(l)") }
        if let host = settings.baseURL(for: p)?.host() { parts.append(host) }
        if !s.model.isEmpty { parts.append(s.model) }
        return parts.joined(separator: " · ")
    }

    private func whatLeaves(_ p: AIProvider) -> String {
        if p == .apple { return "AI runs on this device. Nothing leaves it." }
        if !URLPolicy.needsConsent(provider: p, baseURL: settings.baseURL(for: p)) {
            return "AI requests go to your own server at \(settings.baseURL(for: p)?.host() ?? "your network"). Nothing goes to a cloud service."
        }
        return "When you use AI, the transcript, meeting title, attendee names and invite notes go to \(settings.baseURL(for: p)?.host() ?? p.name). Audio and photos stay on this device."
    }
}

// MARK: - Apple Watch

/// Pairing / install status and recordings still on their way (docs/WATCH_APP.md).
struct AppleWatchSection: View {
    @Environment(PhoneWatchLink.self) private var link
    @Query(filter: #Predicate<Meeting> { $0.importState != nil }) private var importing: [Meeting]

    var body: some View {
        if link.isSupported {
            Section {
                LabeledContent("Apple Watch", value: !link.activated ? "Checking…" : link.isPaired ? "Paired" : "Not paired")
                    .accessibilityIdentifier("watch-paired")
                if link.isPaired {
                    LabeledContent("noFriction on the watch", value: link.isWatchAppInstalled ? "Installed" : "Not installed")
                        .accessibilityIdentifier("watch-app-installed")
                }
                LabeledContent("Waiting to transcribe", value: "\(importing.count)")
                    .accessibilityIdentifier("watch-pending")
            } header: {
                Text("Apple Watch")
            } footer: {
                Text(footer)
            }
        }
    }

    private var footer: String {
        var text = "Record on your watch: the audio comes to this iPhone over Apple's watch connection, is transcribed here on the device, and is deleted from the watch once it arrives."
        if link.isPaired && !link.isWatchAppInstalled {
            text += " To install, open the Watch app on this iPhone and find noFriction under Available Apps."
        }
        if importing.contains(where: { $0.importPhase == .failed }) {
            text += " Recordings that couldn't be transcribed show Retry in the meeting."
        }
        return text
    }
}
