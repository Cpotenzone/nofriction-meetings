import SwiftUI

/// Ask before meeting text is sent to the configured remote endpoint.
struct AIConsentSheet: View {
    let provider: AIProvider
    @Environment(AISettings.self) private var settings
    let onAllow: () -> Void
    @Environment(\.dismiss) private var dismiss

    private var recipient: String { settings.baseURL(for: provider)?.host() ?? provider.name }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Image(systemName: "paperplane.circle.fill")
                        .font(.system(size: 44))
                        .foregroundStyle(Theme.ai)
                        .accessibilityHidden(true)
                    Text("Send meeting content to \(recipient)?")
                        .font(.title2.weight(.semibold))
                    Text(Self.body(recipient))
                        .font(.body)
                        .foregroundStyle(.secondary)
                    Label("Audio and photos never leave this device.", systemImage: "lock.fill")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .padding(24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .safeAreaInset(edge: .bottom) {
                VStack(spacing: 10) {
                    Button {
                        onAllow()
                        dismiss()
                    } label: {
                        Text("Allow").frame(maxWidth: .infinity).padding(.vertical, 6)
                    }
                    .buttonStyle(.borderedProminent)
                    .foregroundStyle(.black)
                    .accessibilityIdentifier("consent-allow")
                    Button("Not now") { dismiss() }
                        .accessibilityIdentifier("consent-not-now")
                }
                .padding(20)
                .background(.bar)
            }
            .background(Theme.background)
        }
        .presentationDetents([.medium, .large])
    }

    /// Spec copy. iOS never sends screenshots, so that clause is left out.
    static func body(_ name: String) -> String {
        "If you allow this remote endpoint, noFriction sends the transcript, the meeting title, " +
            "attendee names and invite notes to \(name), including your API key if configured. The endpoint operator's privacy policy " +
            "and terms apply. This is optional: you can use a local or Apple on-device model instead. " +
            "noFriction offers no hosted models and receives none of this content."
    }
}

/// One-time notice before the first recording.
struct RecordingNoticeSheet: View {
    let onContinue: () -> Void
    @Environment(\.dismiss) private var dismiss

    static let text = "Recording laws differ — in many places everyone in the conversation must agree to be recorded. Tell participants you're recording."

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Image(systemName: "person.wave.2.fill")
                .font(.system(size: 40))
                .foregroundStyle(Theme.accent)
                .accessibilityHidden(true)
            Text("Before you record")
                .font(.title2.weight(.semibold))
                .accessibilityAddTraits(.isHeader)
            Text(Self.text)
                .foregroundStyle(.secondary)
            Text("Audio and transcripts are stored on this device. Optional cloud AI sends meeting text directly to the provider you choose.")
                .font(.footnote)
                .foregroundStyle(.secondary)
            Spacer(minLength: 0)
            Button {
                onContinue()
                dismiss()
            } label: {
                Text("I understand").frame(maxWidth: .infinity).padding(.vertical, 6)
            }
            .buttonStyle(.borderedProminent)
            .foregroundStyle(.black)
            .accessibilityIdentifier("recording-notice-continue")
            Button("Cancel") { dismiss() }
                .frame(maxWidth: .infinity)
        }
        .padding(24)
        .presentationDetents([.medium])
        .background(Theme.background)
    }
}
