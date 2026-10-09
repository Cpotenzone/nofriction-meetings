import AVFoundation
import SwiftData
import SwiftUI

struct MeetingDetailView: View {
    @Bindable var meeting: Meeting
    /// Open scrolled to the transcript line spoken at this moment (a Chat citation)
    var jumpTo: Date? = nil
    @Environment(\.modelContext) private var context
    @Environment(\.openURL) private var openURL
    @Environment(\.dismiss) private var dismiss
    @State private var viewing: Snapshot?
    @State private var confirmDelete = false
    @State private var player = AudioPlayback()
    @State private var aiWorking: String?
    @State private var aiError: String?
    @State private var email: String?
    @Environment(Store.self) private var store
    @Environment(AISettings.self) private var aiSettings
    @State private var showPaywall = false
    @State private var showAISetup = false
    @State private var consentFor: AIProvider?
    @State private var pendingAI: AIAction?
    @Environment(RedactionCenter.self) private var redactions
    // Editing (docs/REDACTION.md)
    @State private var editingSegment: Segment?
    @State private var selectingLines = false
    @State private var selectedLines: Set<PersistentIdentifier> = []
    @State private var selectingPhotos = false
    @State private var selectedPhotos: Set<PersistentIdentifier> = []
    @State private var strikeRequest: StrikeRequest?
    // Review: markers and the review / study guide (docs/STUDY_TOOLS.md)
    @State private var studyProgress: MeetingAI.StudyProgress?
    @State private var studyFailures: [String] = []
    @State private var showStudy = false
    @State private var studyRunning = false
    @State private var jumpTarget: PersistentIdentifier?
    /// Topics (docs/TOPICS_AND_CHAT.md): why the last Find topics failed
    @State private var topicsError: String?

    enum AIAction { case notes, email, study, topics }

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    header
                    if meeting.audioFileName != nil { playback }
                    if !meeting.segments.isEmpty { notes }
                    if !meeting.segments.isEmpty {
                        StudySection(meeting: meeting, working: studyRunning ? aiWorking : nil, busy: aiWorking != nil,
                                     progress: studyProgress, failures: studyFailures,
                                     onMake: { requestAI(.study) }, onOpen: { showStudy = true })
                    }
                    if !meeting.markers.isEmpty { MarkersSection(meeting: meeting, onJump: jump) }
                    if !meeting.people.isEmpty { people }
                    if !meeting.snapshots.isEmpty || !meeting.screenStrikes.isEmpty { photos }
                    MeetingLinksSection(meeting: meeting)
                    transcript
                }
                .padding(20)
                .frame(maxWidth: 760, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
            .onChange(of: jumpTarget) { _, target in
                guard let target else { return }
                withAnimation { proxy.scrollTo(target, anchor: .center) }
                jumpTarget = nil
            }
            .task {
                // Opened from a citation: let the transcript lay out, then scroll to the moment
                guard let jumpTo else { return }
                try? await Task.sleep(for: .milliseconds(400))
                jump(jumpTo)
            }
        }
        .sheet(isPresented: $showStudy) { StudyGuideView(meeting: meeting, onJump: jump) }
        .background(Theme.background)
        .safeAreaInset(edge: .bottom) { selectionBar }
        .navigationBarTitleDisplayMode(.inline)
        .sheet(item: $editingSegment, onDismiss: { player.stop() }) { segment in
            WordEditorSheet(segment: segment, meeting: meeting)
        }
        .sheet(item: $strikeRequest) { request in
            NavigationStack {
                StrikeConfirmView(target: request.target, meeting: meeting) {
                    strikeRequest = nil
                    endSelection()
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { strikeRequest = nil } }
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                ShareLink(item: MeetingExport.markdown(meeting), subject: Text(meeting.title), preview: SharePreview(meeting.title)) {
                    Image(systemName: "square.and.arrow.up")
                }
                .accessibilityLabel("Share recording")
                .accessibilityHint("Shares the title, people, notes and transcript as text")
            }
            ToolbarItem(placement: .secondaryAction) {
                Button("Delete Recording", systemImage: "trash", role: .destructive) { confirmDelete = true }
            }
        }
        .confirmationDialog("Delete this recording?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete", role: .destructive) { delete() }
        } message: {
            Text("Its transcript, audio and photos are removed from this device.")
        }
        .fullScreenCover(item: $viewing) { snapshot in
            SnapshotViewer(snapshot: snapshot)
        }
        .onDisappear { player.stop() }
        .sheet(item: Binding(get: { email.map(DraftText.init) }, set: { email = $0?.text })) { draft in
            EmailDraftSheet(text: draft.text)
        }
        .sheet(isPresented: $showPaywall, onDismiss: { resumePending(if: store.isPro) }) { PaywallView() }
        .sheet(isPresented: $showAISetup, onDismiss: { resumePending(if: aiSettings.endpoint() != nil) }) { AISetupSheet() }
        .sheet(item: $consentFor, onDismiss: { resumePending(if: aiSettings.endpoint()?.consentGranted == true) }) { p in
            AIConsentSheet(provider: p) { aiSettings.grantConsent(p) }
        }
    }

    // MARK: Sections

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Title", text: $meeting.title, axis: .vertical)
                .font(.title.weight(.semibold))
                .onSubmit { try? context.save() }
            Text(whenLine).font(.subheadline).foregroundStyle(.secondary)
            RecordingKindNotebookField(meeting: meeting)
            if meeting.isFromWatch {
                Label("Recorded on Apple Watch", systemImage: "applewatch")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("watch-source")
            }
            if let location = meeting.location, !location.isEmpty {
                Label(location, systemImage: "mappin").font(.subheadline).foregroundStyle(.secondary)
            }
            if let link = meeting.meetingURL, let url = URL(string: link) {
                Button { openURL(url) } label: {
                    Label(url.host ?? "Join link", systemImage: "video")
                }
                .font(.subheadline)
            }
            if let notes = meeting.inviteNotes, !notes.isEmpty {
                DisclosureGroup("Invite notes") {
                    Text(notes).font(.footnote).foregroundStyle(.secondary).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .font(.subheadline)
                .tint(.secondary)
            }
        }
    }

    private var whenLine: String {
        var parts = [meeting.startedAt.formatted(.dateTime.weekday(.wide).month(.wide).day().hour().minute())]
        if let d = meeting.duration { parts.append(d.minutesLabel) }
        return parts.joined(separator: " · ")
    }

    private var playback: some View {
        HStack(spacing: 14) {
            Button {
                player.toggle(url: Storage.audio.appending(path: meeting.audioFileName ?? ""))
            } label: {
                Image(systemName: player.isPlaying ? "pause.fill" : "play.fill")
                    .font(.system(size: 16, weight: .semibold))
                    .frame(width: 44, height: 44)
                    .background(Theme.card, in: Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(player.isPlaying ? "Pause recording" : "Play recording")
            .accessibilityIdentifier("playback-button")
            VStack(alignment: .leading, spacing: 6) {
                ProgressView(value: player.progress).tint(Theme.accent)
                    .accessibilityLabel("Playback position")
                Text(player.isPlaying || player.progress > 0 ? "\(player.currentTime.clock) / \(player.duration.clock)" : "Recording")
                    .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
        }
        .padding(12)
        .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }

    // MARK: AI notes (Pro)

    private var notes: some View {
        SectionBlock(title: "Notes") {
            VStack(alignment: .leading, spacing: 12) {
                if meeting.aiNotes != nil && meeting.aiNotesStale {
                    HStack(spacing: 8) {
                        Label("Made before an edit", systemImage: "exclamationmark.arrow.circlepath")
                            .font(.footnote)
                            .foregroundStyle(Theme.ai)
                        Spacer(minLength: 0)
                        Button("Make again") { requestAI(.notes) }
                            .accessibilityLabel("Make notes again")
                            .font(.footnote.weight(.semibold))
                            .tint(Theme.ai)
                            .disabled(aiWorking != nil)
                    }
                    .accessibilityIdentifier("ai-notes-stale")
                }
                if let md = meeting.aiNotes {
                    // One renderer for every type: headings and bullets, not raw ## / **
                    NotesMarkdownView(markdown: md)
                        .font(.callout)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if let at = meeting.aiNotesAt {
                        Text("Notes · \(at.formatted(date: .abbreviated, time: .shortened))")
                            .font(.caption2).foregroundStyle(.tertiary)
                    }
                }
                if let aiWorking {
                    HStack(spacing: 8) {
                        ProgressView()
                        Text(aiWorking).font(.footnote).foregroundStyle(.secondary)
                    }
                }
                if let aiError {
                    Label(aiError, systemImage: "exclamationmark.triangle")
                        .font(.footnote).foregroundStyle(.orange)
                }
                HStack(spacing: 10) {
                    Button(meeting.aiNotes == nil ? "Make notes" : "Make again", systemImage: "sparkles") {
                        requestAI(.notes)
                    }
                    .accessibilityHint(notesHint)
                    .accessibilityIdentifier("ai-summarize")
                    // A follow-up email is about a meeting's attendees
                    if meeting.kind == .meeting {
                        Button("Follow-up email", systemImage: "envelope") {
                            requestAI(.email)
                        }
                        .accessibilityHint("Drafts a follow-up email with your AI")
                        .accessibilityIdentifier("ai-email")
                    }
                }
                .buttonStyle(.bordered)
                .tint(Theme.ai)
                .disabled(aiWorking != nil)
                .font(.subheadline)
                Divider().overlay(Theme.hairline)
                TopicsEditor(meeting: meeting, busy: aiWorking != nil, error: topicsError) { requestAI(.topics) }
            }
            .padding(14)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
    }

    /// The notes style follows the type: meeting notes, lecture notes, personal notes.
    private var notesHint: String {
        switch meeting.kind {
        case .meeting: "Writes a summary, decisions and action items with your AI"
        case .class: "Writes lecture notes (concepts, definitions, announcements) with your AI"
        case .personal: "Writes a summary, key points and to-dos with your AI"
        }
    }

    /// Pro → provider configured → consent → run. Each missing step shows its
    /// sheet and resumes the action when the sheet closes.
    private func requestAI(_ action: AIAction) {
        pendingAI = nil
        guard store.isPro else {
            pendingAI = action
            showPaywall = true
            return
        }
        guard let endpoint = aiSettings.endpoint() else {
            pendingAI = action
            showAISetup = true
            return
        }
        if endpoint.needsConsent && !endpoint.consentGranted {
            pendingAI = action
            consentFor = endpoint.provider
            return
        }
        switch action {
        case .notes:
            runAI("Making notes with \(aiName(endpoint))…") {
                let text = try await MeetingAI.notes(context: MeetingAI.context(meeting), endpoint: endpoint,
                                                     kind: meeting.kind)
                meeting.aiNotes = text
                meeting.aiNotesAt = .now
                meeting.aiNotesStale = false
                try? context.save()
                // Same hook as the Mac: topics are named with the notes
                await findTopics(endpoint)
            }
        case .topics:
            runAI("Finding topics with \(aiName(endpoint))…") { await findTopics(endpoint) }
        case .email:
            runAI("Drafting email with \(aiName(endpoint))…") {
                email = try await MeetingAI.followUpEmail(context: MeetingAI.context(meeting), endpoint: endpoint)
            }
        case .study:
            runStudy(endpoint)
        }
    }

    /// Who is working, in the words Settings uses: "Apple on-device", a
    /// preset's name ("Anthropic (Claude)") or the endpoint's host. Never
    /// "Your AI endpoint", the internal name of the one custom connection.
    private func aiName(_ endpoint: AIEndpoint) -> String {
        if endpoint.provider == .apple { return AIProvider.apple.name }
        let name = aiSettings.displayName(for: endpoint.provider)
        if name != endpoint.provider.name { return name }
        return aiSettings.baseURL(for: endpoint.provider)?.host() ?? "your AI"
    }

    /// Review (study) guide: every part, saved only if the transcript was not edited meanwhile.
    private func runStudy(_ endpoint: AIEndpoint) {
        let input = StudyInput(meeting: meeting)
        let fingerprint = input.fingerprint
        studyFailures = []
        studyProgress = nil
        studyRunning = true
        runAI("Making the \(meeting.kind.guideTitle.lowercased()) with \(aiName(endpoint))…") {
            defer { studyProgress = nil; studyRunning = false }
            do {
                let results = try await MeetingAI.studyGuide(input, contextTokens: endpoint.contextTokens,
                                                             complete: MeetingAI.liveComplete(endpoint)) { p in
                    await MainActor.run { studyProgress = p }
                }
                let ok = results.compactMap { kind, r in (try? r.get()).map { (kind, $0) } }
                studyFailures = results.compactMap { _, r in
                    if case .failure(let f) = r { return f.errorDescription } else { return nil }
                }
                try StudyStore.save(ok, fingerprint: fingerprint, meeting: meeting, context: context)
                if !ok.isEmpty { showStudy = true }
            } catch {
                // Shown in the Review section, not under the notes
                studyFailures = [error.localizedDescription]
            }
        }
    }

    /// Name the recording's topics (docs/TOPICS_AND_CHAT.md). User topics
    /// and removed ones are respected by `TopicStore.applyAI`. A failure is
    /// shown under the topics, never under the notes.
    private func findTopics(_ endpoint: AIEndpoint) async {
        topicsError = nil
        let input = StudyInput(meeting: meeting)
        do {
            let found = try await MeetingAI.findTopics(input, contextTokens: endpoint.contextTokens,
                                                       complete: MeetingAI.liveComplete(endpoint))
            try TopicStore.applyAI(found, to: meeting, context: context)
        } catch {
            topicsError = error.localizedDescription
        }
    }

    /// Scroll the transcript to the line being spoken at `time`.
    private func jump(_ time: Date) {
        let rows = transcriptRows
        jumpTarget = (rows.last { $0.start <= time } ?? rows.first)?.persistentModelID
    }

    /// Markers by the transcript row they follow (the line being spoken when marked).
    private var markersByRow: [PersistentIdentifier: [MomentMarker]] {
        let rows = transcriptRows
        guard !rows.isEmpty else { return [:] }
        var out: [PersistentIdentifier: [MomentMarker]] = [:]
        for m in meeting.orderedMarkers {
            let host = rows.last { $0.start <= m.at } ?? rows[0]
            out[host.persistentModelID, default: []].append(m)
        }
        return out
    }

    /// Continue only if the step that interrupted is now satisfied (no loops on "Not now" / Close).
    private func resumePending(if satisfied: Bool) {
        guard let action = pendingAI else { return }
        pendingAI = nil
        if satisfied { requestAI(action) }
    }

    private func runAI(_ label: String, _ work: @escaping @MainActor () async throws -> Void) {
        aiWorking = label
        aiError = nil
        Task { @MainActor in
            defer { aiWorking = nil }
            do { try await work() } catch { aiError = error.localizedDescription }
        }
    }

    private var people: some View {
        SectionBlock(title: "People") {
            VStack(spacing: 0) {
                ForEach(meeting.people, id: \.person.email) { item in
                    PersonRow(person: item.person, role: item.role)
                    if item.person.email != meeting.people.last?.person.email { Divider().overlay(Theme.hairline) }
                }
            }
        }
    }

    private enum PhotoItem: Identifiable {
        case photo(Snapshot)
        case struck(Redaction)
        var id: String {
            switch self {
            case .photo(let s): "p-\(s.persistentModelID.hashValue)"
            case .struck(let r): "r-\(r.id.uuidString)"
            }
        }
        var time: Date {
            switch self {
            case .photo(let s): s.takenAt
            case .struck(let r): r.coveredFrom ?? r.createdAt
            }
        }
    }

    private var photoItems: [PhotoItem] {
        (meeting.orderedSnapshots.map(PhotoItem.photo) + meeting.screenStrikes.map(PhotoItem.struck))
            .sorted { $0.time < $1.time }
    }

    private var photos: some View {
        SectionBlock(title: "Photos") {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), spacing: 10)], spacing: 10) {
                ForEach(photoItems) { item in
                    switch item {
                    case .photo(let snapshot):
                        let on = selectedPhotos.contains(snapshot.persistentModelID)
                        SnapshotThumb(snapshot: snapshot)
                            .overlay(alignment: .topTrailing) {
                                if selectingPhotos {
                                    Image(systemName: on ? "checkmark.circle.fill" : "circle")
                                        .accessibilityHidden(true)
                                        .font(.title3)
                                        .foregroundStyle(on ? Theme.accent : .white)
                                        .shadow(radius: 2)
                                        .padding(6)
                                }
                            }
                            .overlay {
                                if on { RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Theme.accent, lineWidth: 2) }
                            }
                            .contentShape(Rectangle())
                            .onTapGesture {
                                if selectingPhotos { togglePhoto(snapshot) } else { viewing = snapshot }
                            }
                            .onLongPressGesture(minimumDuration: 0.35) {
                                startSelection(photos: true)
                                selectedPhotos.insert(snapshot.persistentModelID)
                            }
                            .accessibilityElement(children: .ignore)
                            .accessibilityLabel("Photo, \(snapshot.takenAt.formatted(date: .omitted, time: .shortened))")
                            .accessibilityHint(selectingPhotos ? "Selects or deselects this photo" : "Opens the photo. Touch and hold to select photos.")
                            .accessibilityAddTraits(.isButton)
                            .accessibilityAddTraits(on ? .isSelected : [])
                            .accessibilityAction(named: "Select") {
                                if !selectingPhotos { startSelection(photos: true) }
                                togglePhoto(snapshot)
                            }
                            .accessibilityIdentifier("photo-thumb")
                    case .struck(let r):
                        StrickenScreenCard(redaction: r)
                    }
                }
            }
        } accessory: {
            if !meeting.snapshots.isEmpty {
                Button(selectingPhotos ? "Done" : "Select") {
                    if selectingPhotos { endSelection() } else { startSelection(photos: true) }
                }
                .font(.subheadline)
                .accessibilityIdentifier("photos-select")
            }
        }
    }

    private var transcript: some View {
        SectionBlock(title: "Transcript") {
            if meeting.importPhase != nil {
                WatchImportStatus(meeting: meeting)
            }
            if meeting.segments.isEmpty {
                if meeting.importPhase == nil {
                    Text("Nothing was transcribed.").foregroundStyle(.secondary)
                }
            } else {
                let inlineMarkers = markersByRow
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(transcriptRows) { segment in
                        let hasWords = RedactionText.tokens(segment.text).contains(where: \.isWord)
                        EditableTranscriptRow(segment: segment, meeting: meeting, selecting: selectingLines,
                                              selected: selectedLines.contains(segment.persistentModelID))
                            .onTapGesture {
                                guard hasWords else { return }
                                if selectingLines { toggleLine(segment) } else { editingSegment = segment }
                            }
                            .accessibilityHint(!hasWords ? "" : selectingLines
                                ? "Selects or deselects this line"
                                : "Opens the line to delete or strike words from the record")
                            .accessibilityAction(named: "Edit words") {
                                if hasWords && !selectingLines { editingSegment = segment }
                            }
                            .contextMenu {
                                if hasWords && !selectingLines {
                                    Button("Edit words…", systemImage: "character.cursor.ibeam") { editingSegment = segment }
                                    Button("Delete line", systemImage: "trash", role: .destructive) {
                                        player.stop()
                                        redactions.delete(.lines([segment]), in: meeting, context: context)
                                    }
                                    Button("Strike from the record…", systemImage: "eye.slash") {
                                        strikeRequest = StrikeRequest(target: .lines([segment]))
                                    }
                                }
                            }
                            .id(segment.persistentModelID)
                        // Moments marked while this line was spoken
                        ForEach(inlineMarkers[segment.persistentModelID] ?? []) { m in
                            MarkerInlineRow(marker: m, meeting: meeting)
                        }
                    }
                }
            }
        } accessory: {
            if meeting.segments.contains(where: { RedactionText.tokens($0.text).contains(where: \.isWord) }) {
                Button(selectingLines ? "Done" : "Select") {
                    if selectingLines { endSelection() } else { startSelection(photos: false) }
                }
                .font(.subheadline)
                .accessibilityIdentifier("transcript-select")
            }
        }
    }

    /// Ordered lines; a struck run of lines (each just the same marker) shows once.
    private var transcriptRows: [Segment] {
        var out: [Segment] = []
        var last: UUID?
        for s in meeting.orderedSegments {
            let marker = RedactionText.onlyMarker(s.text)
            if let marker, marker == last { continue }
            last = marker
            out.append(s)
        }
        return out
    }

    // MARK: Selection (lines / photos)

    @ViewBuilder
    private var selectionBar: some View {
        if selectingLines || selectingPhotos {
            let count = selectingLines ? selectedLines.count : selectedPhotos.count
            EditActionBar(enabled: count > 0, cancel: { endSelection() }, onDelete: {
                guard let target = selectionTarget else { return }
                player.stop()
                redactions.delete(target, in: meeting, context: context)
                endSelection()
            }, onStrike: {
                guard let target = selectionTarget else { return }
                player.stop()
                strikeRequest = StrikeRequest(target: target)
            })
        }
    }

    private var selectionTarget: EditTarget? {
        if selectingLines {
            let segs = meeting.orderedSegments.filter { selectedLines.contains($0.persistentModelID) }
            return segs.isEmpty ? nil : .lines(segs)
        }
        let snaps = meeting.orderedSnapshots.filter { selectedPhotos.contains($0.persistentModelID) }
        return snaps.isEmpty ? nil : .screens(snaps)
    }

    private func startSelection(photos: Bool) {
        selectingPhotos = photos
        selectingLines = !photos
        selectedLines = []
        selectedPhotos = []
    }

    private func endSelection() {
        selectingLines = false
        selectingPhotos = false
        selectedLines = []
        selectedPhotos = []
    }

    private func toggleLine(_ s: Segment) {
        if selectedLines.contains(s.persistentModelID) { selectedLines.remove(s.persistentModelID) } else { selectedLines.insert(s.persistentModelID) }
    }

    private func togglePhoto(_ s: Snapshot) {
        if selectedPhotos.contains(s.persistentModelID) { selectedPhotos.remove(s.persistentModelID) } else { selectedPhotos.insert(s.persistentModelID) }
    }

    private func delete() {
        redactions.discardPending(for: meeting)
        if let name = meeting.audioFileName { try? FileManager.default.removeItem(at: Storage.audio.appending(path: name)) }
        // An Apple Watch recording: any copy still staged from the watch goes
        // too, and the import log keeps a re-delivery from bringing it back
        if let id = meeting.sourceRecordingID.flatMap(UUID.init(uuidString:)) { WatchInbox.shared.remove(id) }
        for s in meeting.snapshots { try? FileManager.default.removeItem(at: s.fileURL) }
        // Chat answers that cited this recording go with it (docs/TOPICS_AND_CHAT.md)
        ChatStore.purge(meetingID: meeting.id, title: meeting.title, deleted: true, context: context)
        context.delete(meeting)
        try? context.save()
        dismiss()
    }
}

/// An Apple Watch recording still being transcribed on this iPhone (or stopped, with Retry).
struct WatchImportStatus: View {
    let meeting: Meeting
    @Environment(WatchImporter.self) private var importer

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            switch meeting.importPhase {
            case .failed:
                Label(meeting.importError ?? "Transcription stopped.", systemImage: "exclamationmark.triangle")
                    .font(.footnote)
                    .foregroundStyle(.orange)
                Button("Retry transcription", systemImage: "arrow.clockwise") { importer.retry(meeting) }
                    .font(.subheadline)
                    .accessibilityIdentifier("watch-import-retry")
            default:
                HStack(spacing: 8) {
                    ProgressView()
                    Text(label).font(.footnote).foregroundStyle(.secondary)
                }
                if let fraction = meeting.importFraction, fraction > 0 {
                    ProgressView(value: fraction).tint(Theme.accent)
                        .accessibilityLabel("Transcription progress")
                }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("watch-import-status")
    }

    private var label: String {
        if meeting.importPhase == .transcribing {
            let percent = meeting.importFraction.map { " \(Int(($0 * 100).rounded()))%" } ?? ""
            return "Transcribing on this iPhone…" + percent
        }
        return "Waiting to transcribe on this iPhone…"
    }
}

struct SectionBlock<Content: View, Accessory: View>: View {
    let title: String
    @ViewBuilder var content: Content
    @ViewBuilder var accessory: Accessory

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline) {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.secondary)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 0)
                accessory
            }
            content
        }
    }
}

extension SectionBlock where Accessory == EmptyView {
    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
        self.accessory = EmptyView()
    }
}

/// A strike waiting for its confirmation sheet
struct StrikeRequest: Identifiable {
    let id = UUID()
    let target: EditTarget
}

// MARK: - Photos

private struct SnapshotThumb: View {
    let snapshot: Snapshot
    var body: some View {
        ZStack(alignment: .bottomLeading) {
            Color(Theme.card)
                .aspectRatio(4 / 3, contentMode: .fit)
                .overlay {
                    if let image = UIImage(contentsOfFile: snapshot.fileURL.path(percentEncoded: false)) {
                        Image(uiImage: image).resizable().scaledToFill()
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            Text(snapshot.takenAt.formatted(date: .omitted, time: .shortened))
                .font(.caption2.monospacedDigit())
                .padding(.horizontal, 6).padding(.vertical, 3)
                .background(.black.opacity(0.55), in: Capsule())
                .padding(6)
        }
    }
}

private struct SnapshotViewer: View {
    let snapshot: Snapshot
    @Environment(\.dismiss) private var dismiss
    @State private var scale: CGFloat = 1

    var body: some View {
        ZStack(alignment: .topTrailing) {
            Color.black.ignoresSafeArea()
            if let image = UIImage(contentsOfFile: snapshot.fileURL.path(percentEncoded: false)) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .scaleEffect(scale)
                    .gesture(MagnifyGesture().onChanged { scale = max(1, $0.magnification) }.onEnded { _ in withAnimation { scale = 1 } })
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                ShareLink(item: Image(uiImage: image), preview: SharePreview("Photo", image: Image(uiImage: image))) {
                    Image(systemName: "square.and.arrow.up").padding(12).background(.ultraThinMaterial, in: Circle())
                }
                .accessibilityLabel("Share photo")
                .padding(.top, 16).padding(.trailing, 70)
            }
            Button { dismiss() } label: {
                Image(systemName: "xmark").font(.headline).padding(12).background(.ultraThinMaterial, in: Circle())
            }
            .accessibilityLabel("Close")
            .padding(16)
        }
    }
}

// MARK: - Audio

@MainActor
@Observable
final class AudioPlayback {
    private var player: AVAudioPlayer?
    private var timer: Timer?
    private(set) var isPlaying = false
    private(set) var progress: Double = 0
    private(set) var currentTime: TimeInterval = 0
    private(set) var duration: TimeInterval = 0

    func toggle(url: URL) {
        if isPlaying { player?.pause(); isPlaying = false; return }
        if player == nil {
            try? AVAudioSession.sharedInstance().setCategory(.playback)
            try? AVAudioSession.sharedInstance().setActive(true)
            player = try? AVAudioPlayer(contentsOf: url)
            duration = player?.duration ?? 0
        }
        player?.play()
        isPlaying = player?.isPlaying ?? false
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
    }

    private func tick() {
        guard let player else { return }
        currentTime = player.currentTime
        progress = duration > 0 ? currentTime / duration : 0
        if !player.isPlaying { isPlaying = false; timer?.invalidate() }
    }

    func stop() {
        player?.stop(); player = nil; timer?.invalidate()
        isPlaying = false; progress = 0; currentTime = 0
    }
}

// MARK: - Export

enum MeetingExport {
    static func markdown(_ m: Meeting) -> String {
        var out = "# \(m.title)\n\n"
        out += m.startedAt.formatted(date: .complete, time: .shortened)
        if let d = m.duration { out += " · \(d.minutesLabel)" }
        out += " · \(m.kind.label)\n"
        if let notebook = m.courseName { out += "\(Notebook.label): \(notebook)\n" }
        if !m.people.isEmpty {
            out += "\n## People\n"
            for (p, role) in m.people {
                var line = "- \(p.displayName)"
                if let c = p.company { line += " (\(c))" }
                if role == "organizer" { line += " — organizer" }
                if let li = p.linkedinURL { line += " — \(li)" }
                out += line + "\n"
            }
        }
        if let notes = m.aiNotes {
            out += "\n\(notes)\n"
            if m.aiNotesStale { out += "\n_These notes were made before the transcript was edited._\n" }
        }
        out += "\n## Transcript\n\n"
        // Stricken spans render as [stricken from the record], struck screens
        // as [screen stricken from the record] (docs/REDACTION.md)
        for e in RedactionText.entries(m) {
            out += "**\(e.time.formatted(date: .omitted, time: .shortened))** \(e.text)\n\n"
        }
        return out
    }
}


// MARK: - Email draft

private struct DraftText: Identifiable {
    let text: String
    var id: String { text }
}

private struct EmailDraftSheet: View {
    let text: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                Text(text).textSelection(.enabled).padding(20)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .navigationTitle("Follow-up email")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
                ToolbarItem(placement: .primaryAction) { ShareLink(item: text) }
            }
        }
    }
}
