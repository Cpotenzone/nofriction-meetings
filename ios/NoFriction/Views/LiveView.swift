import PhotosUI
import SwiftUI

/// Record. The transcript is the screen; the controls sit under your thumb.
struct LiveView: View {
    @Environment(RecordingSession.self) private var session
    @State private var showCamera = false
    @State private var photoItem: PhotosPickerItem?
    @State private var snapFlash = false
    @AppStorage("recordingNoticeAccepted") private var recordingNoticeAccepted = false
    @State private var showRecordingNotice = false
    /// "What is it?", "How long?" and a notebook before recording
    @State private var showPlanSheet = false
    @State private var planAfterNotice = false
    @AppStorage(ClassNotice.shownKey) private var classNoticeShown = false
    @State private var showClassNotice = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                header
                if let countdown = session.endCountdown {
                    MeetingEndBanner(deadline: countdown.deadline,
                                     keep: { session.keepRecording() },
                                     stop: { session.stopNow() })
                } else if session.timeWarningVisible, let deadline = session.timeDeadline {
                    TimeLimitBanner(deadline: deadline,
                                    extend: { session.extendTimeLimit() },
                                    removeLimit: { session.removeTimeLimit() },
                                    dismiss: { session.dismissTimeWarning() })
                } else if let notice = session.notice {
                    NoticeBanner(text: notice)
                }
                if showClassNotice {
                    ClassNoticeBanner { showClassNotice = false }
                }
                TranscriptStream(meeting: session.meeting, partial: session.partial, phase: session.phase)
                Label("Let everyone know you're recording.", systemImage: "person.wave.2")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .padding(.bottom, 6)
                    .accessibilityIdentifier("recording-reminder")
                // ★ / ? / ✎ moment markers (docs/STUDY_TOOLS.md); the ✎ label follows the type
                if session.isActive { MarkControl() }
                controls
            }
            .background(Theme.background.ignoresSafeArea())
            .toolbar(.hidden, for: .navigationBar)
            .overlay {
                if snapFlash {
                    Color.white.opacity(0.35).ignoresSafeArea().transition(.opacity).allowsHitTesting(false)
                }
            }
            .sheet(isPresented: $showRecordingNotice, onDismiss: {
                // Accepted: on to the Record sheet ("What is it?", "How long?")
                if planAfterNotice {
                    planAfterNotice = false
                    showPlanSheet = true
                }
            }) {
                RecordingNoticeSheet {
                    recordingNoticeAccepted = true
                    planAfterNotice = true
                }
            }
            .sheet(isPresented: $showPlanSheet) {
                RecordPlanSheet { limit, kind, notebook in
                    // First Class recording ever: a one-time reminder about school policy
                    if kind == .class && !classNoticeShown {
                        classNoticeShown = true
                        showClassNotice = true
                    }
                    Task { await session.start(limit: limit, kind: kind, notebook: notebook) }
                }
            }
            .fullScreenCover(isPresented: $showCamera) {
                CameraPicker { image in
                    if let image { saveSnapshot(image) }
                }
                .ignoresSafeArea()
            }
            .onChange(of: photoItem) { _, item in
                guard let item else { return }
                Task {
                    if let data = try? await item.loadTransferable(type: Data.self), let image = UIImage(data: data) {
                        saveSnapshot(image)
                    }
                    photoItem = nil
                }
            }
        }
    }

    // MARK: Header

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(session.meeting?.title ?? "Ready")
                    .font(.title2.weight(.semibold))
                    .lineLimit(2)
                Spacer()
                if session.isActive {
                    TimelineView(.periodic(from: .now, by: 1)) { ctx in
                        // With a time limit: what's left; otherwise elapsed
                        if let left = session.timeRemaining(at: ctx.date) {
                            Text("\(left.clock) left")
                                .font(.system(.body, design: .monospaced).weight(.medium))
                                .foregroundStyle(left <= 300 ? Theme.accent : (session.phase == .paused ? .secondary : .primary))
                                .contentTransition(.numericText())
                                .accessibilityLabel("\(Duration.seconds(left).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .wide))) left")
                        } else {
                            Text(session.elapsed(at: ctx.date).clock)
                                .font(.system(.body, design: .monospaced).weight(.medium))
                                .foregroundStyle(session.phase == .paused ? .secondary : .primary)
                                .contentTransition(.numericText())
                                .accessibilityLabel("Elapsed \(Duration.seconds(session.elapsed(at: ctx.date)).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .wide)))")
                        }
                    }
                }
            }
            Text(subtitle)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            if session.isActive, let deadline = session.timeDeadline {
                TimeLimitRow(deadline: deadline,
                             extend: { session.extendTimeLimit() },
                             removeLimit: { session.removeTimeLimit() })
                    .padding(.top, 4)
            }
            if let people = session.meeting?.people, !people.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(people, id: \.person.email) { item in
                            Text(item.person.displayName)
                                .font(.caption)
                                .padding(.horizontal, 10).padding(.vertical, 5)
                                .background(Theme.card, in: Capsule())
                        }
                    }
                }
                .padding(.top, 4)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var subtitle: String {
        switch session.phase {
        case .idle: return "Works offline. Stays on this \(DeviceName.current)."
        case .starting: return "Starting…"
        case .paused: return "Paused — nothing is being recorded"
        case .stopping: return "Saving…"
        case .recording:
            if let m = session.meeting, let s = m.scheduledStart, let e = m.scheduledEnd {
                return "\(s.formatted(date: .omitted, time: .shortened)) – \(e.formatted(date: .omitted, time: .shortened))"
            }
            // "Class · BIO 101 · Recording"
            guard let m = session.meeting else { return "Recording" }
            return ([m.kind.label] + [m.courseName].compactMap { $0 } + ["Recording"]).joined(separator: " · ")
        }
    }

    // MARK: Controls

    private var controls: some View {
        HStack(alignment: .center) {
            // Pause
            CircleButton(systemImage: session.phase == .paused ? "play.fill" : "pause.fill",
                         label: session.phase == .paused ? "Resume" : "Pause",
                         enabled: session.phase == .recording || session.phase == .paused) {
                session.togglePause()
            }
            .accessibilityHint(session.phase == .paused ? "Continues recording" : "Pauses recording. Nothing is recorded while paused.")
            Spacer()
            RecordButton(phase: session.phase, level: session.level) {
                if session.isActive {
                    Task { await session.stop() }
                } else if !recordingNoticeAccepted {
                    showRecordingNotice = true
                } else {
                    // The Record sheet first; it starts the recording
                    showPlanSheet = true
                }
            }
            Spacer()
            // Snap: camera where there is one; photo library as the alternative
            Menu {
                if UIImagePickerController.isSourceTypeAvailable(.camera) {
                    Button("Take Photo", systemImage: "camera") { showCamera = true }
                }
                PhotosPicker(selection: $photoItem, matching: .images) {
                    Label("Choose from Photos", systemImage: "photo.on.rectangle")
                }
            } label: {
                CircleButtonLabel(systemImage: "camera.fill", label: "Snap", enabled: session.isActive)
            } primaryAction: {
                if UIImagePickerController.isSourceTypeAvailable(.camera) { showCamera = true }
            }
            .disabled(!session.isActive)
            .accessibilityLabel("Snap")
            .accessibilityHint(session.isActive ? "Adds a photo of a slide, whiteboard or screen to this recording" : "Available while recording")
        }
        .padding(.horizontal, 36)
        .padding(.top, 14)
        .padding(.bottom, 10)
        .background(alignment: .top) {
            Rectangle().fill(Theme.hairline).frame(height: 1)
        }
    }

    private func saveSnapshot(_ image: UIImage) {
        session.addSnapshot(image)
        UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        withAnimation(.easeOut(duration: 0.08)) { snapFlash = true }
        withAnimation(.easeIn(duration: 0.25).delay(0.08)) { snapFlash = false }
    }
}

// MARK: - Transcript

private struct TranscriptStream: View {
    let meeting: Meeting?
    let partial: String
    let phase: RecordingSession.Phase

    var body: some View {
        let segments = meeting?.orderedSegments ?? []
        ScrollViewReader { proxy in
            ScrollView {
                if segments.isEmpty && partial.isEmpty {
                    EmptyTranscript(phase: phase)
                        .frame(maxWidth: .infinity)
                        .padding(.top, 80)
                } else {
                    LazyVStack(alignment: .leading, spacing: 14) {
                        ForEach(segments) { segment in
                            TranscriptLine(time: segment.start, text: segment.text)
                        }
                        if !partial.isEmpty {
                            TranscriptLine(time: nil, text: partial, provisional: true)
                        }
                        Color.clear.frame(height: 1).id("end")
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 12)
                    .frame(maxWidth: 720, alignment: .leading)
                    .frame(maxWidth: .infinity)
                }
            }
            .scrollDismissesKeyboard(.immediately)
            // A live transcript opens at its newest line
            .defaultScrollAnchor(.bottom)
            .onChange(of: segments.count) { _, _ in withAnimation(.easeOut(duration: 0.25)) { proxy.scrollTo("end", anchor: .bottom) } }
            .onChange(of: partial) { _, _ in proxy.scrollTo("end", anchor: .bottom) }
        }
    }
}

struct TranscriptLine: View {
    let time: Date?
    let text: String
    var provisional = false
    /// Time column grows with Dynamic Type so "10:42" never wraps
    @ScaledMetric(relativeTo: .caption2) var timeWidth: CGFloat = 52
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 14) {
            Text(time.map { $0.formatted(date: .omitted, time: .shortened) } ?? "")
                .font(.system(.caption2, design: .monospaced))
                .foregroundStyle(.tertiary)
                .frame(width: timeWidth, alignment: .trailing)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
            (Text(text) + (provisional ? Text(" ▍").foregroundStyle(Theme.accent) : Text("")))
                .font(sizeClass == .regular ? .title3 : .body)   // iPad: readable across the room
                .lineSpacing(3)
                .foregroundStyle(provisional ? .secondary : .primary)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(provisional ? "\(text), still listening" : text)
        .accessibilityValue(time.map { $0.formatted(date: .omitted, time: .shortened) } ?? "")
    }
}

private struct EmptyTranscript: View {
    let phase: RecordingSession.Phase

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: "waveform")
                .font(.system(size: 34, weight: .light))
                .foregroundStyle(phase == .recording ? Theme.accent : .secondary)
                .symbolEffect(.variableColor.iterative, isActive: phase == .recording)
                .accessibilityHidden(true)
            Text(phase == .recording ? "Listening" : "Tap to record")
                .font(.headline)
            if phase == .recording {
                Text("Words appear as they're spoken.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            } else {
                PrivacyPromise()
                    .padding(.top, 6)
                Text("Your calendar names the meeting and who's in it.")
                    .font(.footnote)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
                    .padding(.top, 4)
            }
        }
        .padding(.horizontal, 32)
    }
}

/// "iPhone" / "iPad", for copy that names this device.
enum DeviceName {
    static var current: String {
        UIDevice.current.userInterfaceIdiom == .pad ? "iPad" : "iPhone"
    }
}

/// The home screen's promise: recording and on-device transcription work
/// offline, and nothing leaves the device unless the user sends it (AI
/// through a server they set up, exporting, sharing).
struct PrivacyPromise: View {
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "lock.fill")
                .font(.subheadline)
                .foregroundStyle(Theme.accent)
                .frame(width: 30, height: 30)
                .background(Theme.accent.opacity(0.12), in: Circle())
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Text("Recording and transcription work offline.")
                    .font(.subheadline.weight(.semibold))
                Text("Nothing leaves this \(DeviceName.current) unless you want it to.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(maxWidth: 420, alignment: .leading)
        .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityHint("Something leaves only when you choose: AI through a server you set up, or exporting and sharing.")
        .accessibilityIdentifier("privacy-promise")
    }
}

// MARK: - Buttons

private struct RecordButton: View {
    let phase: RecordingSession.Phase
    let level: Float
    let action: () -> Void

    var body: some View {
        let active = phase == .recording || phase == .paused || phase == .starting
        Button(action: {
            UIImpactFeedbackGenerator(style: .rigid).impactOccurred()
            action()
        }) {
            ZStack {
                Circle()
                    .stroke(Color.white.opacity(0.9), lineWidth: 3)
                    .frame(width: 78, height: 78)
                // Level ring
                Circle()
                    .stroke(Theme.recording.opacity(0.35), lineWidth: 6)
                    .frame(width: 78 + CGFloat(level) * 18, height: 78 + CGFloat(level) * 18)
                    .opacity(phase == .recording ? 1 : 0)
                    .animation(.easeOut(duration: 0.1), value: level)
                RoundedRectangle(cornerRadius: active ? 8 : 32, style: .continuous)
                    .fill(Theme.recording)
                    .frame(width: active ? 30 : 64, height: active ? 30 : 64)
                    .animation(.spring(response: 0.35, dampingFraction: 0.7), value: active)
                if phase == .starting || phase == .stopping {
                    ProgressView().tint(.white)
                }
            }
            .frame(width: 100, height: 100)
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .disabled(phase == .starting || phase == .stopping)
        .accessibilityLabel(active ? "Stop recording" : "Start recording")
        .accessibilityHint(active ? "Stops and saves the recording" : "Records and transcribes on this device")
        .accessibilityIdentifier("record-button")
    }
}

private struct CircleButton: View {
    let systemImage: String
    let label: String
    let enabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) { CircleButtonLabel(systemImage: systemImage, label: label, enabled: enabled) }
            .buttonStyle(.plain)
            .disabled(!enabled)
    }
}

private struct CircleButtonLabel: View {
    let systemImage: String
    let label: String
    let enabled: Bool

    var body: some View {
        VStack(spacing: 6) {
            Image(systemName: systemImage)
                .font(.system(size: 18, weight: .semibold))
                .frame(width: 52, height: 52)
                .background(Theme.card, in: Circle())
            Text(label).font(.caption2).foregroundStyle(.secondary)
        }
        .opacity(enabled ? 1 : 0.35)
        .accessibilityElement(children: .combine)
    }
}

struct NoticeBanner: View {
    let text: String
    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.circle.fill").foregroundStyle(Theme.accent)
                .accessibilityHidden(true)
            Text(text).font(.footnote)
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
        .padding(12)
        .background(Theme.accent.opacity(0.1), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 6)
    }
}

/// "Meeting seems to have ended — stopping in 30 s" with the two choices.
struct MeetingEndBanner: View {
    let deadline: Date
    let keep: () -> Void
    let stop: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "moon.zzz.fill").foregroundStyle(Theme.accent)
                    .accessibilityHidden(true)
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    let left = max(0, Int(deadline.timeIntervalSince(ctx.date).rounded(.up)))
                    Text("Meeting seems to have ended — stopping in \(left) s")
                        .font(.footnote.weight(.semibold))
                        .contentTransition(.numericText())
                }
                Spacer(minLength: 0)
            }
            // Side by side; stacked when large text wouldn't fit
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 10) { bannerButtons }
                VStack(alignment: .leading, spacing: 8) { bannerButtons }
            }
            .font(.footnote.weight(.medium))
        }
        .padding(12)
        .background(Theme.accent.opacity(0.1), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("meeting-end-banner")
    }

    @ViewBuilder private var bannerButtons: some View {
        Button("Keep recording", action: keep)
            .buttonStyle(.bordered)
            .fixedSize()
            .accessibilityHint("Cancels the automatic stop")
            .accessibilityIdentifier("meeting-end-keep")
        Button("Stop now", role: .destructive, action: stop)
            .buttonStyle(.borderedProminent)
            .tint(Theme.recordingStrong)
            .fixedSize()
            .accessibilityHint("Stops and saves the meeting now")
            .accessibilityIdentifier("meeting-end-stop")
    }
}

/// UIKit camera, wrapped.
struct CameraPicker: UIViewControllerRepresentable {
    let onFinish: (UIImage?) -> Void
    @Environment(\.dismiss) private var dismiss

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_: UIImagePickerController, context _: Context) {}
    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let parent: CameraPicker
        init(_ parent: CameraPicker) { self.parent = parent }

        func imagePickerController(_: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            parent.onFinish(info[.originalImage] as? UIImage)
            parent.dismiss()
        }

        func imagePickerControllerDidCancel(_: UIImagePickerController) {
            parent.onFinish(nil)
            parent.dismiss()
        }
    }
}
