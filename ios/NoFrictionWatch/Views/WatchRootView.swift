import SwiftUI

/// Record screen; recent recordings one tap away.
struct WatchRootView: View {
    @Environment(WatchAppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        NavigationStack {
            RecordView()
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) {
                        NavigationLink {
                            RecordingsListView()
                        } label: {
                            Image(systemName: "list.bullet")
                        }
                        .accessibilityLabel("Recordings")
                        .accessibilityIdentifier("watch-recordings")
                    }
                }
        }
        .sheet(isPresented: $model.showNotice) {
            RecordingNoticeView()
        }
    }
}

// MARK: - Record

struct RecordView: View {
    @Environment(WatchAppModel.self) private var model

    var body: some View {
        let recorder = model.recorder
        TimelineView(.periodic(from: .now, by: 1)) { _ in
            Group {
                if recorder.machine.isActive {
                    activeView(recorder)
                } else {
                    idleView
                }
            }
        }
        .navigationTitle(recorder.machine.isActive ? "" : "noFriction")
        .navigationBarTitleDisplayMode(.inline)
    }

    // Idle: one big button
    private var idleView: some View {
        VStack(spacing: 8) {
            Button {
                model.recordTapped()
            } label: {
                ZStack {
                    Circle().fill(WatchTheme.recordingStrong)
                    VStack(spacing: 2) {
                        Image(systemName: "mic.fill").font(.system(size: 30, weight: .semibold))
                        Text("Record").font(.headline)
                    }
                    .foregroundStyle(.white)
                }
                .frame(maxWidth: 118, maxHeight: 118)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Start recording")
            .accessibilityIdentifier("watch-record")
            Text(idleCaption)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .lineLimit(3)
                .minimumScaleFactor(0.8)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var idleCaption: String {
        if let notice = model.recorder.notice ?? model.launchNotice { return notice }
        let waiting = model.store.waitingCount
        if waiting > 0 { return waiting == 1 ? "1 recording waiting for your iPhone" : "\(waiting) recordings waiting for your iPhone" }
        return "Transcribed on your iPhone"
    }

    // Recording / paused
    private func activeView(_ recorder: WatchRecorder) -> some View {
        let paused = recorder.machine.isPaused
        return VStack(spacing: 6) {
            HStack(spacing: 5) {
                Circle()
                    .fill(paused ? Color.secondary : WatchTheme.recording)
                    .frame(width: 8, height: 8)
                Text(paused ? "Paused" : "Recording")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(paused ? Color.secondary : WatchTheme.recording)
            }
            .accessibilityElement(children: .combine)
            Text(ClockText.format(recorder.elapsed))
                .font(.system(size: 40, weight: .semibold, design: .rounded).monospacedDigit())
                .minimumScaleFactor(0.6)
                .lineLimit(1)
                .accessibilityLabel("Elapsed \(ClockText.format(recorder.elapsed))")
                .accessibilityIdentifier("watch-elapsed")
            LevelMeter(level: recorder.level)
                .frame(height: 8)
                .padding(.horizontal, 8)
                .accessibilityHidden(true)
            if let notice = recorder.notice {
                Text(notice)
                    .font(.caption2)
                    .foregroundStyle(.orange)
                    .multilineTextAlignment(.center)
                    .lineLimit(3)
                    .minimumScaleFactor(0.8)
            }
            Spacer(minLength: 0)
            HStack(spacing: 8) {
                Button {
                    recorder.togglePause()
                } label: {
                    Image(systemName: paused ? "play.fill" : "pause.fill")
                        .font(.title3)
                        .frame(maxWidth: .infinity)
                }
                .tint(WatchTheme.accent)
                .accessibilityLabel(paused ? "Resume recording" : "Pause recording")
                .accessibilityIdentifier("watch-pause")
                Button {
                    recorder.stop()
                } label: {
                    Image(systemName: "stop.fill")
                        .font(.title3)
                        .frame(maxWidth: .infinity)
                }
                .tint(WatchTheme.recording)
                .accessibilityLabel("Stop and send to iPhone")
                .accessibilityIdentifier("watch-stop")
            }
            .buttonStyle(.borderedProminent)
            .foregroundStyle(.black)
        }
    }
}

/// Live input level: a filled capsule.
struct LevelMeter: View {
    let level: Float

    var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.white.opacity(0.15))
                Capsule()
                    .fill(WatchTheme.accent)
                    .frame(width: max(6, geo.size.width * CGFloat(min(max(level, 0), 1))))
                    .animation(.linear(duration: 0.1), value: level)
            }
        }
    }
}

// MARK: - Recordings

struct RecordingsListView: View {
    @Environment(WatchAppModel.self) private var model

    var body: some View {
        List {
            if model.connection.activated && !model.connection.companionInstalled {
                Text("Install noFriction on your iPhone to receive recordings.")
                    .font(.footnote)
                    .foregroundStyle(.orange)
            }
            if model.recordings.isEmpty {
                Text("No recordings yet.")
                    .foregroundStyle(.secondary)
            }
            ForEach(model.recordings) { entry in
                RecordingRow(entry: entry)
                    .swipeActions {
                        if entry.status != .sending && entry.status != .recording {
                            Button(role: .destructive) {
                                model.delete(entry)
                            } label: {
                                Label("Delete", systemImage: "trash")
                            }
                        }
                    }
            }
            if model.recordings.contains(where: { $0.status == .failed }) {
                Button("Try sending again", systemImage: "arrow.clockwise") { model.retryNow() }
            }
            Section {
                EmptyView()
            } footer: {
                Text("Each recording is deleted from your watch once your iPhone has it. noFriction on your iPhone transcribes it on the device.")
            }
        }
        .navigationTitle("Recordings")
    }
}

struct RecordingRow: View {
    let entry: WatchRecordingEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(entry.startedAt.formatted(.dateTime.weekday(.abbreviated).hour().minute()))
                .font(.headline)
            if entry.duration > 0 {
                Text(ClockText.format(entry.duration))
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            Label(status.text, systemImage: status.icon)
                .font(.footnote)
                .foregroundStyle(status.color)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("watch-recording-row")
    }

    private var status: (text: String, icon: String, color: Color) {
        switch entry.status {
        case .recording: return ("Recording", "record.circle", WatchTheme.recording)
        case .saved: return ("Saved on watch", "applewatch", .secondary)
        case .sending: return ("Sending to iPhone", "arrow.up.circle", WatchTheme.accent)
        case .delivered: return ("Delivered", "checkmark.circle.fill", WatchTheme.delivered)
        case .failed: return ("Saved on watch · will retry", "exclamationmark.arrow.circlepath", .orange)
        }
    }
}

// MARK: - First-use notice

struct RecordingNoticeView: View {
    @Environment(WatchAppModel.self) private var model

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                Image(systemName: "person.wave.2.fill")
                    .font(.title2)
                    .foregroundStyle(WatchTheme.accent)
                    .accessibilityHidden(true)
                Text("Before you record")
                    .font(.headline)
                    .accessibilityAddTraits(.isHeader)
                Text(RecordingNotice.text)
                    .font(.footnote)
                Text("Audio goes from your watch to your iPhone and is transcribed there, on the device.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                Button("I understand") { model.noticeConfirmed() }
                    .buttonStyle(.borderedProminent)
                    .foregroundStyle(.black)
                    .accessibilityIdentifier("watch-notice-continue")
            }
        }
    }
}
