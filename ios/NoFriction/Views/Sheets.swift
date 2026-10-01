import SwiftUI

/// App Review 5.1.2(i): ask before the first request to each cloud provider.
struct AIConsentSheet: View {
    let provider: AIProvider
    let onAllow: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Image(systemName: "paperplane.circle.fill")
                        .font(.system(size: 44))
                        .foregroundStyle(Theme.ai)
                    Text("Send meeting content to \(provider.name)?")
                        .font(.title2.weight(.semibold))
                    Text(Self.body(provider.name))
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
        "To write notes, summaries and emails, noFriction sends the transcript, the meeting title, " +
            "attendee names and invite notes to \(name) using your API key. \(name)'s privacy policy " +
            "and terms apply. Nothing is sent to noFriction; we have no servers."
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
            Text("Before you record")
                .font(.title2.weight(.semibold))
            Text(Self.text)
                .foregroundStyle(.secondary)
            Text("Audio and transcripts are stored only on this device.")
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
            .accessibilityIdentifier("recording-notice-continue")
            Button("Cancel") { dismiss() }
                .frame(maxWidth: .infinity)
        }
        .padding(24)
        .presentationDetents([.medium])
        .background(Theme.background)
    }
}
