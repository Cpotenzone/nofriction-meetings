import StoreKit
import SwiftUI
import UIKit

// MARK: - Connect flow

/// Paste a key → detect the provider → validate by listing models → save to
/// the Keychain → ask for consent.
@MainActor
@Observable
final class AIConnectModel {
    enum Status: Equatable {
        case idle
        case checking(String)
        case connected(String)
        case failed(String)
    }

    var keyText = ""
    var manualProvider: AIProvider = .openai
    var status: Status = .idle

    var serverProvider: AIProvider = .ollama {
        didSet { serverURL = serverProvider.defaultBaseURL?.absoluteString ?? "" }
    }
    var serverURL = AIProvider.ollama.defaultBaseURL?.absoluteString ?? ""
    var serverKey = ""

    /// Set after a successful connect when the provider still needs consent.
    var consentPrompt: AIProvider?

    var normalizedKey: String { KeyDetector.normalize(keyText) }
    var detected: AIProvider? { KeyDetector.detect(normalizedKey) }
    var isBusy: Bool { if case .checking = status { true } else { false } }

    func pasteFromClipboard() {
        if let s = UIPasteboard.general.string { keyText = KeyDetector.normalize(s) }
    }

    func connectKey(_ settings: AISettings) async {
        let key = normalizedKey
        guard !key.isEmpty else { status = .failed("Paste a key first."); return }
        let candidates = KeyDetector.candidates(for: key).isEmpty ? [manualProvider] : KeyDetector.candidates(for: key)
        var lastError: Error = AIError.notConfigured
        for (i, p) in candidates.enumerated() {
            status = .checking("Checking with \(p.name)…")
            do {
                let models = try await AIClient.shared.validate(AISettings.probe(p, key: key, baseURL: nil))
                try settings.save(p, key: key, baseURL: nil, models: models)
                keyText = ""
                status = .connected("Connected to \(p.name)" + (settings.model(for: p.id).map { " · \($0)" } ?? ""))
                if !settings.hasConsent(p) { consentPrompt = p }
                return
            } catch AIError.wrongKey(let m) where i < candidates.count - 1 {
                lastError = AIError.wrongKey(m)   // an sk- key may be DeepSeek's: try the next one
            } catch {
                lastError = error
                break
            }
        }
        status = .failed(Redactor.redact(lastError.localizedDescription, secrets: [key]))
    }

    func connectServer(_ settings: AISettings) async {
        guard let url = URLPolicy.parseBaseURL(serverURL) else { status = .failed("Enter the server's address."); return }
        guard URLPolicy.isAllowed(url) else { status = .failed(AIError.insecureURL.localizedDescription); return }
        let key = KeyDetector.normalize(serverKey)
        let p = serverProvider
        status = .checking("Checking \(url.host() ?? "server")…")
        do {
            let models = try await AIClient.shared.validate(AISettings.probe(p, key: key.isEmpty ? nil : key, baseURL: url))
            try settings.save(p, key: key.isEmpty ? nil : key, baseURL: url, models: models)
            serverKey = ""
            let model = settings.model(for: p.id) ?? ""
            status = .connected(model.isEmpty ? "Connected. Enter a model name below." : "Connected · \(model)")
            if !settings.hasConsent(p) { consentPrompt = p }
        } catch {
            status = .failed(Redactor.redact(error.localizedDescription, secrets: [key]))
        }
    }
}

// MARK: - Paste-a-key card (Settings + setup sheet)

struct AIKeySection: View {
    @Environment(AISettings.self) private var settings
    @Bindable var model: AIConnectModel

    var body: some View {
        Section {
            VStack(alignment: .leading, spacing: 12) {
                SecureField("Paste your API key", text: $model.keyText)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .font(.body.monospaced())
                    .padding(12)
                    .background(Theme.background, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .privacySensitive()
                    .submitLabel(.go)
                    .onSubmit { connect() }
                    .accessibilityLabel("API key")
                    .accessibilityHint("Paste a key from OpenAI, Anthropic, Gemini or another provider.")
                    .accessibilityIdentifier("api-key-field")
                HStack(spacing: 10) {
                    Button("Paste", systemImage: "doc.on.clipboard") { model.pasteFromClipboard() }
                        .buttonStyle(.bordered)
                        .accessibilityLabel("Paste key")
                        .accessibilityHint("Pastes an API key from the clipboard. The provider is detected from the key.")
                    Button {
                        connect()
                    } label: {
                        Label("Connect", systemImage: "bolt.horizontal")
                    }
                    .buttonStyle(.borderedProminent)
                    .foregroundStyle(.black)   // readable on hazard yellow
                    .disabled(model.normalizedKey.isEmpty || model.isBusy)
                    .accessibilityIdentifier("connect-key")
                }
                if !model.normalizedKey.isEmpty {
                    if let p = model.detected {
                        Label("Detected: \(p.name) key", systemImage: "checkmark.seal")
                            .font(.footnote).foregroundStyle(.secondary)
                    } else {
                        Picker("Provider", selection: $model.manualProvider) {
                            ForEach(AIProvider.cloud) { Text($0.name).tag($0) }
                        }
                        .font(.footnote)
                    }
                }
                StatusLine(status: model.status)
            }
            .padding(.vertical, 4)
        } header: {
            Text("Connect AI")
        } footer: {
            Text("OpenAI, Anthropic, Gemini, xAI, Groq, OpenRouter, Perplexity and more are detected from the key. Keys are stored in this device's Keychain and never sent anywhere except the provider.")
        }
    }

    private func connect() {
        Task { await model.connectKey(settings) }
    }
}

private struct StatusLine: View {
    let status: AIConnectModel.Status
    var body: some View {
        switch status {
        case .idle: EmptyView()
        case .checking(let s):
            HStack(spacing: 8) { ProgressView(); Text(s) }.font(.footnote).foregroundStyle(.secondary)
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
                    Text("AI notes use your own AI account. Paste an API key; OpenAI is the default.")
                        .font(.callout)
                    if let url = AIProvider.openai.getKeyURL {
                        Link("Get an OpenAI key", destination: url)
                    }
                }
                AIKeySection(model: model)
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
                AIKeySection(model: connect)
                savedSection
                recordingSection
                serverSection
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
                    let chat = saved.models.map(\.id).filter(ModelPicker.isChatCapable)
                    if !chat.isEmpty {
                        Picker("Model", selection: Binding(
                            get: { saved.model },
                            set: { settings.setModel($0, for: p) })) {
                            ForEach(chat.contains(saved.model) || saved.model.isEmpty ? chat : [saved.model] + chat, id: \.self) {
                                Text($0).tag($0)
                            }
                        }
                    }
                    HStack {
                        TextField(chat.isEmpty ? "Model name" : "Other model ID", text: $customModel)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                        Button("Use") {
                            settings.setModel(customModel, for: p)
                            customModel = ""
                        }
                        .disabled(customModel.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                    if chat.isEmpty && saved.model.isEmpty {
                        Text("Enter the model to use (for example, llama3.2).").font(.footnote).foregroundStyle(.orange)
                    } else if !saved.model.isEmpty && !chat.contains(saved.model) {
                        LabeledContent("Model", value: saved.model)
                    }
                    if p.editableBaseURL, let url = settings.baseURL(for: p) {
                        LabeledContent("Server", value: url.absoluteString)
                    }
                }
                Text(whatLeaves(p))
                    .font(.footnote).foregroundStyle(.secondary)
            } else {
                Text("No AI provider yet. Paste a key below to turn on AI notes and follow-up emails.")
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
                        Button("Delete Key", systemImage: "trash", role: .destructive) { settings.remove(p) }
                    }
                }
                if AppleOnDevice.isAvailable && settings.saved[AIProvider.apple.id] == nil {
                    Button("Use Apple on-device (no key)") { settings.useApple() }
                }
            } header: {
                Text("Saved providers")
            } footer: {
                Text("Tap to make active. Swipe left to delete a key.")
            }
        }
    }

    private var serverSection: some View {
        Section {
            Picker("Type", selection: $connect.serverProvider) {
                ForEach(AIProvider.selfHosted) { Text($0.name).tag($0) }
            }
            TextField("Server address", text: $connect.serverURL)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
            SecureField("API key (optional)", text: $connect.serverKey)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .privacySensitive()
            Button("Connect to server") { Task { await connect.connectServer(settings) } }
                .disabled(connect.serverURL.isEmpty || connect.isBusy)
        } header: {
            Text("Your own server")
        } footer: {
            Text("Ollama, LM Studio, or any OpenAI-compatible endpoint. Plain http:// works only for this device, your local network or Tailscale; everything else needs https://. iOS will ask for Local Network access the first time.")
        }
    }

    @ViewBuilder private var privacySection: some View {
        Section {
            Text("Audio, photos and transcripts are stored only on this device. Transcription runs on the device. When you use an AI feature, the meeting's text goes to the provider you chose, under that provider's terms. noFriction has no servers and receives nothing.")
                .font(.footnote)
            ForEach(AIProvider.all.filter { settings.consented.contains($0.id) }) { p in
                HStack {
                    Text("Sharing allowed: \(p.name)")
                    Spacer()
                    Button("Revoke", role: .destructive) { settings.revokeConsent(p) }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Revoke sharing with \(p.name)")
                }
            }
            DisclosureGroup("Get an API key") {
                ForEach(AIProvider.cloud) { p in
                    if let url = p.getKeyURL { Link(p.name, destination: url) }
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
        if p.editableBaseURL, let host = settings.baseURL(for: p)?.host() { parts.append(host) }
        if !s.model.isEmpty { parts.append(s.model) }
        return parts.joined(separator: " · ")
    }

    private func whatLeaves(_ p: AIProvider) -> String {
        if p == .apple { return "AI runs on this device. Nothing leaves it." }
        if !URLPolicy.needsConsent(provider: p, baseURL: settings.baseURL(for: p)) {
            return "AI requests go to your own server at \(settings.baseURL(for: p)?.host() ?? "your network"). Nothing goes to a cloud service."
        }
        return "When you use AI, the transcript, meeting title, attendee names and invite notes go to \(p.name). Audio and photos stay on this device."
    }
}
