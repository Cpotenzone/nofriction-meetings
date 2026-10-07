import SwiftUI

/// Two vertical pages (Digital Crown or swipe): Record, then Recordings.
/// A Discreet recording takes the whole screen instead (no paging, so the
/// crown opens its controls).
struct WatchRootView: View {
    @Environment(WatchAppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        Group {
            if model.recorder.machine.isActive && model.recorder.discreet {
                DiscreetRecordingView()
            } else {
                TabView(selection: $model.page) {
                    NavigationStack {
                        RecordView()
                    }
                    .tag(WatchAppModel.Page.record)
                    NavigationStack {
                        RecordingsListView()
                    }
                    .tag(WatchAppModel.Page.recordings)
                }
                .tabViewStyle(.verticalPage)
            }
        }
        .sheet(isPresented: $model.showNotice) {
            RecordingNoticeView()
        }
        .sheet(isPresented: $model.showStartFlow) {
            StartFlowView()
        }
    }
}

// MARK: - Record

struct RecordView: View {
    @Environment(WatchAppModel.self) private var model

    var body: some View {
        let recorder = model.recorder
        TimelineView(.periodic(from: .now, by: 1)) { ctx in
            Group {
                if recorder.machine.isActive {
                    ActiveRecordingView(now: ctx.date)
                } else {
                    idleView
                }
            }
        }
        .navigationTitle(recorder.machine.isActive ? "" : "noFriction")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(recorder.machine.isActive ? .hidden : .automatic, for: .navigationBar)
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
            .accessibilityHint("Choose what it is and how long, then record")
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
}

/// Recording (or paused), standard display: the big time left (elapsed
/// with no limit), Mark within one tap, then Pause and Stop.
struct ActiveRecordingView: View {
    @Environment(WatchAppModel.self) private var model
    let now: Date

    var body: some View {
        let recorder = model.recorder
        let machine = recorder.machine
        let paused = machine.isPaused
        let left = machine.timeLeft(at: now)
        let elapsed = machine.elapsed(at: now)
        ZStack {
            VStack(spacing: 3) {
                HStack(spacing: 5) {
                    Circle()
                        .fill(paused ? Color.secondary : WatchTheme.recording)
                        .frame(width: 8, height: 8)
                    Text(statusText(paused: paused))
                        .font(.footnote.weight(.semibold))
                        .foregroundStyle(paused ? Color.secondary : WatchTheme.recording)
                        .lineLimit(1)
                }
                .accessibilityElement(children: .combine)
                Text(ClockText.format(left ?? elapsed))
                    .font(.system(size: 42, weight: .semibold, design: .rounded).monospacedDigit())
                    .foregroundStyle((left ?? .infinity) <= 300 ? WatchTheme.accent : .primary)
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                    .accessibilityLabel(left.map { "\(ClockText.format($0)) left" } ?? "Elapsed \(ClockText.format(elapsed))")
                    .accessibilityIdentifier(left == nil ? "watch-elapsed" : "watch-time-left")
                Text(caption(left: left, elapsed: elapsed))
                    .font(.caption2)
                    .foregroundStyle(recentlyMarked ? WatchTheme.accent : .secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                LevelMeter(level: recorder.level)
                    .frame(height: 5)
                    .padding(.horizontal, 10)
                    .accessibilityHidden(true)
                if let notice = recorder.notice {
                    Text(notice)
                        .font(.caption2)
                        .foregroundStyle(.orange)
                        .multilineTextAlignment(.center)
                        .lineLimit(2)
                        .minimumScaleFactor(0.8)
                }
                Spacer(minLength: 0)
                MarkButton(kind: machine.kind, enabled: machine.phase == .recording)
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
            if recorder.timeWarningVisible, let left {
                TimeWarningCard(left: left, deadline: machine.deadline)
            }
        }
    }

    private var recentlyMarked: Bool {
        guard let mark = model.recorder.lastMark else { return false }
        return now.timeIntervalSince(mark.at) < 3
    }

    /// "Recording · Class · BIO 101"
    private func statusText(paused: Bool) -> String {
        let machine = model.recorder.machine
        var parts = [paused ? "Paused" : "Recording"]
        if machine.kind != .meeting { parts.append(machine.kind.label) }
        if let notebook = machine.notebook { parts.append(notebook) }
        return parts.joined(separator: " · ")
    }

    private func caption(left: TimeInterval?, elapsed: TimeInterval) -> String {
        if recentlyMarked, let mark = model.recorder.lastMark {
            return "\(mark.kind.symbol) Marked \(mark.kind.label(for: model.recorder.machine.kind))"
        }
        let count = model.recorder.machine.markers.count
        let marks = count > 0 ? " · ★ \(count)" : ""
        if left != nil { return "left · \(ClockText.format(elapsed)) recorded" + marks }
        return "recorded" + marks
    }
}

/// One tap marks ★ Important. Touch and hold for ? Question or the third
/// kind (On the test / Follow up / Remember, by type). No notes on the watch.
struct MarkButton: View {
    @Environment(WatchAppModel.self) private var model
    let kind: RecordingKind
    let enabled: Bool
    @State private var choosing = false

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "star.fill")
            Text("Mark")
        }
        .font(.headline)
        .foregroundStyle(.black)
        .frame(maxWidth: .infinity, minHeight: 34)
        .background(WatchTheme.accent.opacity(enabled ? 1 : 0.35), in: Capsule())
        .contentShape(Capsule())
        .onTapGesture { if enabled { model.recorder.mark(.important) } }
        .onLongPressGesture(minimumDuration: 0.5) { if enabled { choosing = true } }
        .confirmationDialog("Mark this moment", isPresented: $choosing) {
            Button("\(MarkerKind.question.symbol) \(MarkerKind.question.label(for: kind))") { model.recorder.mark(.question) }
            Button("\(MarkerKind.test.symbol) \(MarkerKind.test.label(for: kind))") { model.recorder.mark(.test) }
            Button("\(MarkerKind.important.symbol) \(MarkerKind.important.label(for: kind))") { model.recorder.mark(.important) }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Mark this moment")
        .accessibilityHint("Marks it Important. Touch and hold for Question or \(MarkerKind.test.label(for: kind)).")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { if enabled { model.recorder.mark(.important) } }
        .accessibilityAction(named: MarkerKind.question.label(for: kind)) { if enabled { model.recorder.mark(.question) } }
        .accessibilityAction(named: MarkerKind.test.label(for: kind)) { if enabled { model.recorder.mark(.test) } }
        .accessibilityIdentifier("watch-mark")
    }
}

/// "5 min left" with +15 min and No limit (the notification offers the same).
struct TimeWarningCard: View {
    @Environment(WatchAppModel.self) private var model
    let left: TimeInterval
    let deadline: Date?

    var body: some View {
        VStack(spacing: 6) {
            HStack {
                Text("\(max(1, Int((left / 60).rounded(.up)))) min left")
                    .font(.headline)
                Spacer(minLength: 0)
                Button {
                    model.recorder.dismissWarning()
                } label: {
                    Image(systemName: "xmark")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Dismiss")
            }
            .padding(.trailing, 10)
            if let deadline {
                Text("Stops at \(deadline.formatted(date: .omitted, time: .shortened))")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            Button("+15 min") { model.recorder.extendLimit() }
                .buttonStyle(.borderedProminent)
                .tint(WatchTheme.accent)
                .foregroundStyle(.black)
                .accessibilityIdentifier("watch-extend")
            Button("No limit") { model.recorder.removeLimit() }
                .accessibilityIdentifier("watch-no-limit")
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 4)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        // Opaque: nothing of the recording screen shows through the choices
        .background(Color.black)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("watch-time-warning")
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

// MARK: - Record flow

/// What is it? → How long? → Notebook, then recording starts. The first
/// row starts at once with the last choices. Discreet is remembered.
struct StartFlowView: View {
    @Environment(WatchAppModel.self) private var model
    @State private var remembered = WatchStartOptions.remembered()
    @State private var discreet = DiscreetSetting.isOn()
    @State private var path: [Step] = []

    enum Step: Hashable {
        case length(RecordingKind)
        case notebook(RecordingKind, RecordingLimit)
    }

    var body: some View {
        NavigationStack(path: $path) {
            List {
                Section {
                    Button {
                        start(remembered.kind, remembered.limit, nil)
                    } label: {
                        VStack(spacing: 2) {
                            Label("Start", systemImage: "mic.fill")
                                .font(.headline)
                            Text("\(remembered.kind.label) · \(remembered.limit.label)")
                                .font(.footnote)
                                .foregroundStyle(.white.opacity(0.85))
                                .lineLimit(1)
                                .minimumScaleFactor(0.8)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 4)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(WatchTheme.recordingStrong)
                    .foregroundStyle(.white)
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                    .accessibilityLabel("Start, \(remembered.kind.label), \(remembered.limit.spoken)")
                    .accessibilityIdentifier("watch-quick-start")
                }
                Section {
                    ForEach(RecordingKind.allCases) { kind in
                        Button {
                            path.append(.length(kind))
                        } label: {
                            HStack {
                                Label(kind.label, systemImage: kind.systemImage)
                                Spacer(minLength: 4)
                                if kind == remembered.kind {
                                    Image(systemName: "checkmark").foregroundStyle(WatchTheme.accent)
                                        .accessibilityHidden(true)
                                }
                            }
                        }
                        .accessibilityAddTraits(kind == remembered.kind ? .isSelected : [])
                        .accessibilityIdentifier("watch-kind-\(kind.rawValue)")
                    }
                } header: {
                    Text(RecordingKind.pickerTitle)
                } footer: {
                    Text(RecordingKind.help)
                }
                Section {
                    Toggle("Discreet", isOn: $discreet)
                        .accessibilityIdentifier("watch-discreet")
                } footer: {
                    Text("A dim, low-distraction screen while recording, for lectures. The microphone indicator still shows; tell people you're recording.")
                }
            }
            .navigationTitle("Record")
            .navigationDestination(for: Step.self) { step in
                switch step {
                case .length(let kind):
                    LengthStepView(remembered: remembered.limit) { limit in
                        if model.recentNotebooks.isEmpty {
                            start(kind, limit, nil)
                        } else {
                            path.append(.notebook(kind, limit))
                        }
                    }
                case .notebook(let kind, let limit):
                    NotebookStepView(notebooks: model.recentNotebooks) { start(kind, limit, $0) }
                }
            }
        }
        // Opaque: the red Record button doesn't glow through the sheet
        .presentationBackground(Color.black)
        .onChange(of: discreet) { _, on in DiscreetSetting.set(on) }
        #if DEBUG
        .onAppear { path = model.demoStartPath }
        #endif
    }

    private func start(_ kind: RecordingKind, _ limit: RecordingLimit, _ notebook: String?) {
        model.start(WatchStartOptions(kind: kind, limit: limit, notebook: notebook, discreet: discreet))
    }
}

/// "How long?": 15 / 30 / 60 / 90 / No limit (Digital Crown scrolls).
struct LengthStepView: View {
    let remembered: RecordingLimit
    let pick: (RecordingLimit) -> Void

    var body: some View {
        List {
            ForEach(RecordingLimit.choices, id: \.self) { limit in
                Button {
                    pick(limit)
                } label: {
                    HStack {
                        Text(limit.label)
                        Spacer(minLength: 4)
                        if limit == remembered {
                            Image(systemName: "checkmark").foregroundStyle(WatchTheme.accent)
                                .accessibilityHidden(true)
                        }
                    }
                }
                .accessibilityLabel(limit.spoken)
                .accessibilityAddTraits(limit == remembered ? .isSelected : [])
                .accessibilityIdentifier("watch-length-\(limit.storageValue)")
            }
        }
        .navigationTitle("How long?")
    }
}

/// A notebook from the iPhone's recent notebooks, or None.
struct NotebookStepView: View {
    let notebooks: [String]
    let pick: (String?) -> Void

    var body: some View {
        List {
            Button("None") { pick(nil) }
                .accessibilityIdentifier("watch-notebook-none")
            Section {
                ForEach(notebooks, id: \.self) { name in
                    Button {
                        pick(name)
                    } label: {
                        Label(name, systemImage: "book.closed")
                            .lineLimit(2)
                    }
                }
            } footer: {
                Text("Recent notebooks from noFriction on your iPhone.")
            }
        }
        .navigationTitle(Notebook.label)
    }
}

// MARK: - Discreet

/// Discreet: almost black. The only sign of the recording is the noFriction
/// logo fading slowly in and out at low opacity (static with Reduce Motion,
/// dimmer and still with the wrist down). A tap anywhere marks ★ (with a
/// light tap and a faint brighten of the logo) and shows the time for a
/// moment; touch and hold, or turn the Digital Crown, for ? / ✎, the time
/// limit, Pause and Stop. Stopping takes two deliberate steps. The system
/// microphone indicator is unaffected.
struct DiscreetRecordingView: View {
    @Environment(WatchAppModel.self) private var model
    @Environment(\.isLuminanceReduced) private var luminanceReduced
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var glanceUntil: Date?
    @State private var flashUntil: Date?
    @State private var showControls = false
    @State private var crown = 0.0
    @State private var crownTurned = 0.0
    @State private var started = Date()

    var body: some View {
        let recorder = model.recorder
        TimelineView(.animation(minimumInterval: 0.1, paused: luminanceReduced)) { ctx in
            let now = ctx.date
            let p = RecordingPresentation.make(discreet: true, luminanceReduced: luminanceReduced, reduceMotion: reduceMotion,
                                               paused: recorder.machine.isPaused,
                                               glancing: glanceUntil.map { now < $0 } ?? false,
                                               markFlash: flashUntil.map { now < $0 } ?? false)
            ZStack {
                Color.black
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { tapped(p) }
                    .onLongPressGesture(minimumDuration: 0.6) { showControls = true }
                if let logo = p.logo {
                    Image("DiscreetLogo")
                        .renderingMode(.template)
                        .resizable()
                        .scaledToFit()
                        .frame(width: 70, height: 70)
                        .foregroundStyle(.white)
                        .opacity(logo.opacity(at: now.timeIntervalSince(started)))
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
                VStack(spacing: 4) {
                    Spacer()
                    if p.showsGlanceTime {
                        Text(glanceText(now))
                            .font(.footnote.monospacedDigit())
                            .foregroundStyle(.gray)
                            .opacity(p.glanceOpacity)
                            .allowsHitTesting(false)
                    }
                    if recorder.timeWarningVisible && !luminanceReduced {
                        HStack(spacing: 6) {
                            Button("+15") { recorder.extendLimit() }
                                .accessibilityLabel("Add 15 minutes")
                            Button("No limit") { recorder.removeLimit() }
                            Button { recorder.dismissWarning() } label: { Image(systemName: "xmark") }
                                .accessibilityLabel("Dismiss")
                        }
                        .font(.caption2)
                        .buttonStyle(.bordered)
                        .foregroundStyle(.gray)
                        .opacity(0.6)
                    }
                }
                .padding(.bottom, 4)
            }
        }
        .toolbar(.hidden, for: .navigationBar)
        .focusable()
        .digitalCrownRotation($crown, from: -1_000_000, through: 1_000_000, by: 1, sensitivity: .low,
                              isContinuous: true, isHapticFeedbackEnabled: false)
        .onChange(of: crown) { old, new in
            crownTurned += abs(new - old)
            if crownTurned > 6 {
                crownTurned = 0
                showControls = true
            }
        }
        .onChange(of: luminanceReduced) { _, reduced in
            // Wrist raised: show the time for a moment
            if !reduced { glance() }
        }
        .onChange(of: recorder.timeWarningVisible) { _, visible in if visible { glance() } }
        .onAppear {
            started = .now
            glance()
            #if DEBUG
            if model.demoShowDiscreetControls { showControls = true }
            #endif
        }
        .sheet(isPresented: $showControls) {
            DiscreetControlsView()
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Recording, Discreet")
        .accessibilityHint("Double-tap to mark this moment. Touch and hold for more.")
        .accessibilityAction { _ = recorder.mark(.important) }
        .accessibilityAction(named: "Controls") { showControls = true }
        .accessibilityIdentifier("watch-discreet-screen")
    }

    private func tapped(_ p: RecordingPresentation) {
        glance()
        guard p.tapAnywhereMarks, model.recorder.mark(.important) != nil else { return }
        flashUntil = Date().addingTimeInterval(RecordingPresentation.markFlashSeconds)
    }

    private func glance() {
        glanceUntil = Date().addingTimeInterval(RecordingPresentation.glanceSeconds)
    }

    private func glanceText(_ now: Date) -> String {
        let machine = model.recorder.machine
        if machine.isPaused { return "Paused" }
        if let left = machine.timeLeft(at: now) { return "\(ClockText.format(left)) left" }
        return ClockText.format(machine.elapsed(at: now))
    }
}

/// Discreet controls, kept dim: ? / ✎ marks, the time limit, Pause, and Stop.
struct DiscreetControlsView: View {
    @Environment(WatchAppModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let recorder = model.recorder
        let machine = recorder.machine
        List {
            if let notice = recorder.notice {
                Text(notice).font(.footnote).foregroundStyle(.gray)
            }
            if machine.phase == .recording {
                Section {
                    Button("\(MarkerKind.question.symbol) \(MarkerKind.question.label(for: machine.kind))") { markAndClose(.question) }
                    Button("\(MarkerKind.test.symbol) \(MarkerKind.test.label(for: machine.kind))") { markAndClose(.test) }
                } header: {
                    Text("Mark")
                }
            }
            if machine.deadline != nil {
                Section {
                    Button("+15 min") { recorder.extendLimit(); dismiss() }
                    Button("No limit") { recorder.removeLimit(); dismiss() }
                } header: {
                    Text(machine.timeLeft(at: .now).map { "\(ClockText.format($0)) left" } ?? "Time limit")
                }
            }
            Section {
                Button(machine.isPaused ? "Resume" : "Pause") { recorder.togglePause(); dismiss() }
                Button("Stop recording", role: .destructive) { recorder.stop(); dismiss() }
                    .accessibilityIdentifier("watch-discreet-stop")
            }
        }
        .foregroundStyle(.gray)
        .tint(.gray)
        .listRowBackground(Color.white.opacity(0.05))
        .presentationBackground(Color.black)
    }

    private func markAndClose(_ kind: MarkerKind) {
        model.recorder.mark(kind)
        dismiss()
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
                        if entry.canDelete {
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
                Text("Each recording is deleted from your watch once noFriction on your iPhone confirms it has it. The iPhone transcribes it on the device.")
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
            if let details {
                Text(details)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Label(status.text, systemImage: status.icon)
                .font(.footnote)
                .foregroundStyle(status.color)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("watch-recording-row")
    }

    /// "Class · BIO 101 · ★ 3" (the notebook and marks only until delivered)
    private var details: String? {
        var parts: [String] = []
        if let kind = entry.kind, kind != .meeting { parts.append(kind.label) }
        if let notebook = entry.notebook { parts.append(notebook) }
        if !entry.markers.isEmpty { parts.append("★ \(entry.markers.count)") }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    private var status: (text: String, icon: String, color: Color) {
        switch entry.status {
        case .recording: return ("Recording", "record.circle", WatchTheme.recording)
        case .saved: return ("Saved on watch", "applewatch", .secondary)
        case .sending:
            let total = entry.parts.count
            let done = entry.confirmedParts.count
            return (total > 1 && done > 0 ? "Sending to iPhone (\(done)/\(total))" : "Sending to iPhone", "arrow.up.circle", WatchTheme.accent)
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
        .presentationBackground(Color.black)
    }
}
