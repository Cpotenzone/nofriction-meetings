import SwiftUI

// Screen capture UI (docs/SCREEN_CAPTURE_IOS.md): the Record sheet's
// switches, the Record screen's status row, and the one-time notice about
// video an app hides from capture. Copy says "capture what's on your
// screen", never "record movies".

/// Keeps the extension's "Transcribe what's playing" flag in step with Pro,
/// and re-reads the container when the app comes on screen.
struct ScreenCaptureSync: ViewModifier {
    let center: ScreenCaptureCenter
    let store: Store
    @Environment(\.scenePhase) private var scenePhase

    func body(content: Content) -> some View {
        content
            .onChange(of: store.isPro) { _, _ in center.publishAppAudio() }
            .onChange(of: scenePhase) { _, phase in
                if phase == .active { center.sceneBecameActive() }
            }
    }
}

/// Record sheet: "Capture screen" and, under it, "Transcribe what's playing" (Pro).
struct CaptureScreenOptions: View {
    @Binding var captureScreen: Bool
    @Environment(Store.self) private var store
    @Environment(ScreenCaptureCenter.self) private var center
    @State private var wantsAudio = ScreenCapturePrefs.wantsAppAudio()
    /// The paywall for "Transcribe what's playing" (item-based: it names the feature)
    @State private var paywallFor: ProFeature?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Toggle(isOn: $captureScreen) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Capture screen").font(.subheadline.weight(.semibold))
                    Text("Keeps a picture of what's on your screen each time it changes, next to the transcript. Tap Start Broadcast on the next screen.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .tint(Theme.accent)
            .accessibilityIdentifier("record-capture-screen")
            if captureScreen {
                Toggle(isOn: audioBinding) {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 6) {
                            Text(ProGroup.playing.title).font(.subheadline.weight(.semibold))
                            if !store.isPro {
                                Text("Pro")
                                    .font(.caption2.weight(.bold))
                                    .padding(.horizontal, 6).padding(.vertical, 2)
                                    .background(Theme.ai.opacity(0.2), in: Capsule())
                                    .foregroundStyle(Theme.ai)
                            }
                        }
                        Text("Turns the sound of what's playing into text on this \(DeviceName.current). The sound itself isn't kept.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .tint(Theme.accent)
                .accessibilityIdentifier("record-transcribe-playing")
            }
        }
        .padding(12)
        .background(Theme.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .sheet(item: $paywallFor, onDismiss: { center.publishAppAudio() }) { PaywallView(feature: $0) }
    }

    /// Shows on only for Pro; turning it on as a free user opens the paywall.
    private var audioBinding: Binding<Bool> {
        Binding(
            get: { ScreenAudioPolicy.switchShowsOn(isPro: store.isPro, wantsIt: wantsAudio) },
            set: { on in
                wantsAudio = on
                ScreenCapturePrefs.setWantsAppAudio(on)
                if ScreenAudioPolicy.needsPaywall(turningOn: on, isPro: store.isPro) { paywallFor = ScreenAudioPolicy.feature }
                center.publishAppAudio()
            })
    }
}

/// Record screen, while recording: "Capturing your screen · 12 screens"
/// with Stop, or a Capture screen button. Both open Apple's sheet, where
/// starting or stopping is one tap.
struct ScreenCaptureRow: View {
    @Environment(ScreenCaptureCenter.self) private var center

    var body: some View {
        HStack(spacing: 10) {
            if center.isCapturing {
                Circle().fill(Theme.recording).frame(width: 8, height: 8)
                    .accessibilityHidden(true)
                Text(statusText)
                    .font(.footnote.weight(.medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                    .accessibilityIdentifier("screen-capture-status")
                Spacer(minLength: 0)
                Button("Stop") { center.presentPicker() }
                    .accessibilityLabel("Stop capturing your screen")
                    .accessibilityHint("Opens the system sheet to stop the screen broadcast. The recording continues.")
                    .accessibilityIdentifier("screen-capture-stop")
            } else {
                Button { center.presentPicker() } label: {
                    Label("Capture screen", systemImage: "rectangle.dashed.badge.record")
                }
                .accessibilityHint("Keeps a picture of what's on your screen each time it changes")
                .accessibilityIdentifier("screen-capture-start")
                Spacer(minLength: 0)
            }
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .font(.footnote.weight(.medium))
    }

    private var statusText: String {
        let n = center.screensKept
        return "Capturing your screen · " + (n == 1 ? "1 screen" : "\(n) screens")
    }
}

/// One-time: some video was hidden from capture.
struct HiddenVideoBanner: View {
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "eye.slash").foregroundStyle(Theme.accent)
                .accessibilityHidden(true)
            Text(ScreenCaptureCenter.hiddenNotice).font(.footnote)
            Spacer(minLength: 0)
            Button("OK", action: dismiss)
                .font(.footnote.weight(.semibold))
                .accessibilityLabel("Dismiss")
        }
        .padding(12)
        .background(Theme.accent.opacity(0.1), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 6)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("screen-capture-hidden-notice")
    }
}
