import SwiftData
import StoreKit
import SwiftUI
import UIKit

// MARK: - Endpoint setup

/// Store explicit routing input locally. No key detection, validation probe or
/// model discovery on save. A preset card only fills the URL and model; the
/// connection test runs only on its own tap.
@MainActor
@Observable
final class AIConnectModel {
    enum Status: Equatable {
        case idle
        case connected(String)
        case failed(String)
    }

    /// Card id for "enter your own endpoint" (keeps the form as typed).
    static let customCard = "custom"

    var serverURL = ""
    var serverModel = ""
    var serverKey = ""
    var status: Status = .idle
    var testStatus: Status = .idle
    var testing = false
    var consentPrompt: AIProvider?
    /// The highlighted card: a preset id or `customCard`. Nil until the user
    /// taps one or a saved preset URL is loaded; never set by default.
    var selectedCard: String?
    /// "Connecting…" / "Finish signing in…" while a Connect or Sign in runs
    var working: String?
    /// The key field and model under a key card ("Type the key or change the model")
    var showKeyField = false
    @ObservationIgnored private var signIn: ProviderSignIn?

    var canSave: Bool {
        !serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
        !serverModel.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The preset matching the URL in the form (nil for a custom URL).
    var formPreset: AIPreset? { AIPreset.matching(serverURL) }

    /// The preset the user chose from the cards (nil for custom / none).
    var chosenPreset: AIPreset? { selectedCard.flatMap(AIPreset.byID) }

    func pasteFromClipboard() {
        if let s = UIPasteboard.general.string { serverKey = KeyDetector.normalize(s) }
    }

    /// Fill the form from a preset: its base URL and default model. The key
    /// field is cleared so a key typed for one host is never carried to another.
    func choose(_ preset: AIPreset) {
        selectedCard = preset.id
        serverURL = preset.baseURL
        serverModel = preset.defaultModel
        serverKey = ""
        status = .idle
        testStatus = .idle
    }

    /// The custom card: blank fields (there is no default remote URL), or the
    /// saved custom URL when one is saved and is not a preset's.
    func chooseCustom(_ settings: AISettings) {
        selectedCard = Self.customCard
        if let saved = settings.saved[AIProvider.custom.id], let url = saved.baseURL, AIPreset.matching(URL(string: url)) == nil {
            serverURL = url
            serverModel = saved.model
        } else {
            serverURL = ""
            serverModel = ""
        }
        serverKey = ""
        status = .idle
        testStatus = .idle
    }

    /// Reflect the saved connection when the screen opens: a saved preset URL
    /// highlights its card; any other saved URL highlights Custom; a fresh
    /// install highlights nothing.
    func loadSaved(_ settings: AISettings) {
        guard let saved = settings.saved[AIProvider.custom.id], let url = saved.baseURL else { return }
        serverURL = url
        serverModel = saved.model
        selectedCard = AIPreset.matching(URL(string: url))?.id ?? Self.customCard
    }

    /// The saved custom endpoint is what the form shows (so the test checks what the user sees).
    func canTest(_ settings: AISettings) -> Bool {
        guard let saved = settings.saved[AIProvider.custom.id], let url = saved.baseURL else { return false }
        return URLPolicy.parseBaseURL(serverURL)?.absoluteString == url
            && serverModel.trimmingCharacters(in: .whitespacesAndNewlines) == saved.model
            && serverKey.isEmpty
    }

    /// Plain words under the form: where requests will go, and that the
    /// test sends only "Hi".
    var whatWillBeSent: String {
        let raw = serverURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !raw.isEmpty else { return "Enter a base URL to see where requests will go." }
        let host = URLPolicy.parseBaseURL(raw)?.host() ?? raw
        let model = serverModel.trimmingCharacters(in: .whitespacesAndNewlines)
        let who = formPreset.map { "\($0.name) at \(host)" } ?? host
        return "Requests go straight from this device to \(who) using model \(model.isEmpty ? "(enter a model)" : model) and your own key. Connecting sends only the word \"Hi\"; your recordings go only after you allow it."
    }

    /// One fixed word to the saved custom endpoint with its saved key. Runs
    /// only from the Test connection button.
    func testConnection(_ settings: AISettings, using client: AIClient = .shared) async {
        guard let ep = settings.endpoint(for: .custom) else {
            testStatus = .failed("Save the connection first.")
            return
        }
        testing = true
        testStatus = .idle
        let outcome = await client.testConnection(ep)
        testing = false
        testStatus = outcome.ok ? .connected(outcome.message) : .failed(outcome.message)
    }

    // MARK: Connect in one step (docs/AI_PROVIDERS.md "Connecting")

    /// **Connect**: save the endpoint, key and model, then send the one-word
    /// test, from one explicit tap. Then ask before recordings go to a public
    /// endpoint. A saved key for the same URL is kept when none is typed.
    func connect(_ settings: AISettings, key typed: String? = nil, using client: AIClient = .shared) async {
        guard let url = URLPolicy.parseBaseURL(serverURL) else { status = .failed("Enter your endpoint's base URL."); return }
        guard URLPolicy.isAllowed(url) else { status = .failed(AIError.insecureURL.localizedDescription); return }
        let model = serverModel.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !model.isEmpty else { status = .failed("Enter the model ID supplied by your endpoint."); return }
        var key = KeyDetector.normalize(typed ?? serverKey)
        // Checked before anything is saved: a bad key must not replace a working one
        if !key.isEmpty, !(8...512).contains(key.count) || !key.allSatisfy({ $0.isASCII && !$0.isWhitespace }) {
            status = .failed("That doesn't look like an API key. Copy it again and paste it.")
            return
        }
        if key.isEmpty, settings.saved[AIProvider.custom.id]?.baseURL == url.absoluteString {
            key = settings.endpoint(for: .custom)?.apiKey ?? ""
        }
        if formPreset != nil, key.isEmpty {
            status = .failed("Paste your key first.")
            return
        }
        status = .idle
        testStatus = .idle
        working = "Connecting…"
        defer { working = nil }
        do {
            try settings.save(.custom, key: key.isEmpty ? nil : key, baseURL: url, models: [ModelInfo(id: model, contextTokens: nil)])
        } catch {
            status = .failed(Redactor.redact(error.localizedDescription, secrets: [key]))
            return
        }
        serverKey = ""
        guard let ep = settings.endpoint(for: .custom) else { status = .failed("Save the connection first."); return }
        let outcome = await client.testConnection(ep)
        let name = settings.displayName(for: .custom)
        if outcome.ok {
            status = .connected("Connected to \(name) · \(model).")
            if !settings.hasConsent(.custom) { consentPrompt = .custom }
        } else {
            status = .failed([outcome.message, Self.fix(outcome.message, preset: formPreset)].compactMap { $0 }.joined(separator: " "))
        }
    }

    /// The one thing to do after a failed test, in plain words.
    nonisolated static func fix(_ message: String, preset: AIPreset?) -> String? {
        let m = message.lowercased()
        let keys = preset.flatMap { URL(string: $0.keyURL)?.host() }
        if m.contains("key") { return keys.map { "Copy the key again on \($0), then paste it." } }
        if m.contains("credit") || m.contains("limit") { return keys.map { "Add credit or check your limits on \($0)." } }
        if m.contains("reach") { return "Check your internet connection (or that the server is running)." }
        return nil
    }

    /// A key from the system Paste button, cleaned; or what to say when the
    /// clipboard held something else (a sentence, a web address).
    nonisolated static func key(fromPaste strings: [String], host: String) -> Result<String, AIConnectError> {
        let refuse = AIConnectError(message: "Copy your key on \(host) first, then tap Paste again.")
        let raw = (strings.first ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        // What a paste may legitimately carry around the key; anything else
        // with whitespace inside (a sentence, a password list) is not a key
        var core = raw.trimmingCharacters(in: CharacterSet(charactersIn: "\"'`“”‘’"))
        for prefix in ["export ", "Bearer ", "bearer "] where core.hasPrefix(prefix) {
            core = String(core.dropFirst(prefix.count)).trimmingCharacters(in: .whitespaces)
        }
        guard !core.isEmpty, !core.contains(where: \.isWhitespace) else { return .failure(refuse) }
        var key = KeyDetector.normalize(core)
        // NAME=value from an env file
        if let eq = key.firstIndex(of: "="), key[..<eq].allSatisfy({ $0.isUppercase || $0.isNumber || $0 == "_" }), key[..<eq].contains(where: \.isUppercase) {
            key = String(key[key.index(after: eq)...])
        }
        // Shape only (never which service): one token of 20+ key characters.
        // A copied password has symbols a key doesn't, and is refused.
        let allowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.:+/=")
        guard (20...512).contains(key.count), !key.contains("://"),
              key.unicodeScalars.allSatisfy(allowed.contains) else { return .failure(refuse) }
        return .success(key)
    }

    /// **Paste** under a key card: the key goes from the clipboard to the
    /// Keychain (never shown), then Connect.
    func pasteAndConnect(_ settings: AISettings, _ strings: [String], using client: AIClient = .shared) async {
        let host = chosenPreset.flatMap { URL(string: $0.keyURL)?.host() } ?? "your provider's site"
        switch Self.key(fromPaste: strings, host: host) {
        case .success(let key): await connect(settings, key: key, using: client)
        case .failure(let e): status = .failed(e.message)
        }
    }

    /// **Sign in with …**: the preset's own sign-in sheet; its key, then Connect.
    func signIn(_ settings: AISettings, preset: AIPreset) async {
        guard preset.hasSignIn else { return }
        let flow = ProviderSignIn()
        signIn = flow
        status = .idle
        testStatus = .idle
        working = "Finish signing in…"
        defer { signIn = nil; working = nil }
        do {
            let key = try await flow.run(signInURL: preset.signInURL, exchange: ProviderSignIn.exchange(at: preset.keyExchangeURL))
            selectedCard = nil
            serverURL = preset.baseURL
            serverModel = preset.defaultModel
            await connect(settings, key: key)
        } catch ProviderSignIn.Failure.cancelled {
            // The user's choice: say nothing
        } catch {
            status = .failed(error.localizedDescription)
        }
    }

    func cancelSignIn() {
        signIn?.cancel()
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
            testStatus = .idle
            let host = url.host() ?? url.absoluteString
            status = .connected("Saved. Requests will go to \(host). No request was sent; use Test connection to check the key.")
            if !settings.hasConsent(.custom) { consentPrompt = .custom }
        } catch {
            status = .failed(Redactor.redact(error.localizedDescription, secrets: [key]))
        }
    }
}

struct AIConnectError: Error, Equatable {
    let message: String
}

/// Connect your AI, as few taps as each service allows: Sign in with
/// OpenRouter (the key comes back on its own), Apple on-device, or a key
/// card (Get a key, then Paste) and the custom form. Nothing is selected until
/// tapped; nothing is sent until a Connect, Paste or Sign in tap; recordings
/// only after the user allows it.
struct AIEndpointSection: View {
    @Environment(AISettings.self) private var settings
    @Bindable var model: AIConnectModel

    var body: some View {
        Section {
            ForEach(AIPreset.all.filter(\.hasSignIn)) { p in signInRow(p) }
            if AppleOnDevice.isAvailable {
                Button {
                    settings.useApple()
                    model.selectedCard = nil
                    model.status = .connected("Apple on-device is in use. Nothing leaves this device.")
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(settings.effectiveProvider == .apple ? "Apple on-device: in use" : "Use Apple on-device")
                        Text("Free. Runs on this iPhone; nothing leaves it.").font(.caption).foregroundStyle(.secondary)
                    }
                }
                .disabled(settings.effectiveProvider == .apple)
                .accessibilityIdentifier("use-apple")
            }
        } header: {
            Text("Connect your AI")
        } footer: {
            Text("Nothing is sent until you connect, and your recordings only after you allow it.")
        }

        Section {
            presetCards
            if let p = model.chosenPreset, !p.hasSignIn { keySteps(p) }
            if model.selectedCard == AIConnectModel.customCard { customForm }
            if model.selectedCard != nil {
                Text(model.whatWillBeSent)
                    .font(.footnote).foregroundStyle(.secondary)
                    .accessibilityIdentifier("what-will-be-sent")
            }
            if let w = model.working, !w.hasPrefix("Finish") {
                HStack(spacing: 8) { ProgressView(); Text(w).font(.footnote) }
            }
            StatusLine(status: model.status)
            StatusLine(status: model.testStatus)
        } header: {
            Text("Or use your own key")
        } footer: {
            Text("Keys stay in this device's Keychain, tied to the endpoint; changing the endpoint deletes the old key. Internet endpoints need HTTPS; HTTP works only on this device or your private network.")
        }
        .onAppear { if model.selectedCard == nil { model.loadSaved(settings) } }
    }

    private func signInRow(_ p: AIPreset) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("EASIEST").font(.caption2.weight(.bold)).foregroundStyle(Theme.ai)
            Text(p.name).font(.headline)
            Text(p.note).font(.caption).foregroundStyle(.secondary)
            if model.working?.hasPrefix("Finish") == true {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Finish signing in…").font(.callout)
                    Spacer()
                    Button("Cancel") { model.cancelSignIn() }.buttonStyle(.borderless)
                }
            } else {
                Button {
                    Task { await model.signIn(settings, preset: p) }
                } label: {
                    Label("Sign in with \(p.name.components(separatedBy: " (").first ?? p.name)", systemImage: "person.badge.key")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .foregroundStyle(.black)
                .disabled(model.working != nil)
                .accessibilityIdentifier("sign-in-\(p.id)")
            }
        }
        .padding(.vertical, 4)
    }

    @ViewBuilder private func keySteps(_ p: AIPreset) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            if let keyURL = URL(string: p.keyURL) {
                HStack(spacing: 10) {
                    StepBadge(n: 1)
                    Link("Get a key at \(keyURL.host() ?? p.host)", destination: keyURL)
                        .accessibilityIdentifier("get-key-link")
                }
            }
            HStack(spacing: 10) {
                StepBadge(n: 2)
                Text("Copy it, then")
                PasteButton(payloadType: String.self) { strings in
                    Task { @MainActor in await model.pasteAndConnect(settings, strings) }
                }
                .buttonBorderShape(.capsule)
                .disabled(model.working != nil)
                .accessibilityIdentifier("paste-and-connect")
            }
            Text("Paste checks the key with one word (\"Hi\"). Nothing else is read or sent.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding(.vertical, 4)
        DisclosureGroup("Type the key or change the model", isExpanded: $model.showKeyField) {
            keyField(placeholder: "Paste or type your API key")
            modelField(hint: p.modelHint)
            Button {
                Task { await model.connect(settings) }
            } label: {
                Label("Connect", systemImage: "bolt.horizontal").frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .foregroundStyle(.black)
            .disabled(model.working != nil || !model.canSave)
            .accessibilityIdentifier("connect-ai")
        }
    }

    @ViewBuilder private var customForm: some View {
        TextField("Base URL", text: $model.serverURL)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .keyboardType(.URL)
            .accessibilityIdentifier("ai-endpoint-url")
        modelField(hint: "")
        keyField(placeholder: "API key (optional)")
        VStack(alignment: .leading, spacing: 10) {
            Button {
                Task { await model.connect(settings) }
            } label: {
                Label("Connect", systemImage: "bolt.horizontal").frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .foregroundStyle(.black)
            .disabled(model.working != nil || !model.canSave)
            .accessibilityIdentifier("connect-ai")
            HStack(spacing: 10) {
                Button("Save without testing", systemImage: "checkmark") { model.saveEndpoint(settings) }
                    .buttonStyle(.bordered)
                    .disabled(!model.canSave)
                    .accessibilityIdentifier("save-ai-endpoint")
                testButton
            }
        }
    }

    private func modelField(hint: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            TextField("Model ID", text: $model.serverModel)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .accessibilityIdentifier("ai-model-id")
            if !hint.isEmpty {
                Text("Also: \(hint)").font(.caption2).foregroundStyle(.secondary)
            }
        }
    }

    private func keyField(placeholder: String) -> some View {
        SecureField(placeholder, text: $model.serverKey)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .privacySensitive()
            .accessibilityLabel(model.chosenPreset == nil ? "API key, optional" : "API key")
            .accessibilityIdentifier("api-key-field")
    }

    private var testButton: some View {
        Button {
            Task { await model.testConnection(settings) }
        } label: {
            if model.testing { ProgressView() } else { Label("Test", systemImage: "antenna.radiowaves.left.and.right") }
        }
        .buttonStyle(.bordered)
        .disabled(model.testing || !model.canTest(settings))
        .accessibilityHint("Sends the word Hi with a one-token answer. No recording content.")
        .accessibilityIdentifier("test-ai-connection")
    }

    /// Text-only cards (no third-party logos) for the services that take a key.
    private var presetCards: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(AIPreset.all.filter { !$0.hasSignIn }) { p in
                    PresetCard(name: p.name, note: p.note, selected: model.selectedCard == p.id) { model.choose(p) }
                        .accessibilityIdentifier("preset-\(p.id)")
                }
                PresetCard(name: "Custom endpoint", note: "Any OpenAI-compatible server, local or remote.",
                           selected: model.selectedCard == AIConnectModel.customCard) { model.chooseCustom(settings) }
                    .accessibilityIdentifier("preset-custom")
            }
            .padding(.vertical, 4)
        }
        .listRowInsets(EdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("AI service")
    }
}

private struct StepBadge: View {
    let n: Int
    var body: some View {
        Text("\(n)")
            .font(.caption.weight(.bold))
            .foregroundStyle(.black)
            .frame(width: 22, height: 22)
            .background(Circle().fill(Theme.ai))
            .accessibilityHidden(true)
    }
}

private struct PresetCard: View {
    let name: String
    let note: String
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 3) {
                Text(name).font(.subheadline.weight(.semibold)).foregroundStyle(.primary)
                Text(note).font(.caption2).foregroundStyle(.secondary).lineLimit(2)
            }
            .frame(width: 150, alignment: .leading)
            .padding(10)
            .background(RoundedRectangle(cornerRadius: 10).fill(Color.secondary.opacity(0.08)))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(selected ? Theme.ai : Color.secondary.opacity(0.25), lineWidth: selected ? 2 : 1))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
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
                AIEndpointSection(model: model)
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
    @AppStorage(Onboarding.completedKey) private var onboardingCompleted = false

    var body: some View {
        NavigationStack {
            Form {
                activeSection
                AIEndpointSection(model: connect)
                savedSection
                recordingSection
                AppleWatchSection()
                SyncSection()
                privacySection
                subscriptionSection
                aboutSection
            }
            .scrollContentBackground(.hidden)
            .scrollDismissesKeyboard(.immediately)
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
            Toggle("Stop when it's over", isOn: $autoStop)
                .accessibilityIdentifier("auto-stop-toggle")
        } header: {
            Text("Recording")
        } footer: {
            Text("When the calendar event is over and the room goes quiet, or nobody has spoken for \(Int(MeetingEndDetector.Config().silenceMinutes)) minutes, noFriction asks, then stops after 30 seconds unless you keep recording. Everything said is kept.")
        }
    }

    // MARK: AI

    @ViewBuilder private var activeSection: some View {
        Section("AI") {
            if let p = settings.effectiveProvider {
                LabeledContent("Active", value: settings.displayName(for: p))
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
                Text(settings.needsEndpointSetup ? "Set up AI again: the AI service chosen in an earlier version is no longer available. Your recordings are unchanged. Choose Apple on-device when available, pick a card and paste your own key, or enter your endpoint and model below." : "Choose Apple on-device when available, pick a card and paste your own key, or enter your endpoint and model below to make notes, review guides and Chat answers.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder private var savedSection: some View {
        let list = settings.savedProviders
        // Offered only when it would change something (not while Apple on-device is already in use)
        let offerApple = AppleOnDevice.isAvailable && settings.saved[AIProvider.apple.id] == nil && settings.effectiveProvider != .apple
        if !list.isEmpty || offerApple {
            Section {
                ForEach(list) { p in
                    Button {
                        settings.setActive(p)
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(settings.displayName(for: p)).foregroundStyle(.primary)
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
                    .accessibilityHint("Uses this connection for AI")
                    .swipeActions {
                        Button("Delete", role: .destructive) { settings.remove(p) }
                    }
                    .contextMenu {
                        Button("Delete connection", systemImage: "trash", role: .destructive) { settings.remove(p) }
                    }
                }
                if offerApple {
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
            Text("Audio, photos and transcripts are stored only on this device. Transcription runs on the device. Local and Apple on-device models support offline AI after setup. If you choose a remote AI endpoint, the recording's text goes directly to that endpoint under its operator's terms. noFriction offers no hosted models and receives none of this content.")
                .font(.footnote)
            ForEach(AIProvider.all.filter { settings.consented.contains($0.id) }) { p in
                HStack {
                    Text("Sharing allowed: \(settings.baseURL(for: p)?.host() ?? p.name)")
                    Spacer()
                    Button("Revoke", role: .destructive) { settings.revokeConsent(p) }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Revoke sharing with \(settings.baseURL(for: p)?.host() ?? p.name)")
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
        return "When you use AI, the transcript, recording title, attendee names and invite notes go to \(settings.baseURL(for: p)?.host() ?? p.name). Audio and photos stay on this device."
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
                if link.arrivingCount > 0 {
                    LabeledContent("Arriving from the watch", value: "\(link.arrivingCount)")
                        .accessibilityIdentifier("watch-arriving")
                }
                LabeledContent("Waiting to transcribe", value: "\(importing.count)")
                    .accessibilityIdentifier("watch-pending")
            } header: {
                Text("Apple Watch")
            } footer: {
                Text(footer)
            }
            .onAppear { link.refreshInbox() }
        }
    }

    private var footer: String {
        var text = "Record on your watch: the audio comes to this iPhone over Apple's watch connection, is transcribed here on the device, and is deleted from the watch once it arrives. The watch's notebook list shows the names of your recent notebooks from this iPhone."
        if link.isPaired && !link.isWatchAppInstalled {
            text += " To install, open the Watch app on this iPhone and find noFriction under Available Apps."
        }
        if importing.contains(where: { $0.importPhase == .failed }) {
            text += " Recordings that could not be transcribed show Retry in the recording."
        }
        return text
    }
}
