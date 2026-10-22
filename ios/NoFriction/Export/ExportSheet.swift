import SwiftData
import SwiftUI
import UIKit

/// Export a recording as a package (docs/EXPORT.md): pick what goes in,
/// pick the photos and screens to keep and caption them, let the AI fill in
/// what's missing, then share the .zip (Files, AirDrop, Mail…). Exporting is
/// free; the AI tools go through the same Pro → setup → consent steps as Make
/// notes. Nothing the AI writes here is stored: it goes into the package.
struct ExportSheet: View {
    @Bindable var meeting: Meeting
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var context
    @Environment(Store.self) private var store
    @Environment(AISettings.self) private var aiSettings

    struct Include: Equatable {
        var notes = true, highlights = true, pictures = true, marks = true
        var links = true, email = true, guide = true, transcript = true
    }

    enum Action { case notes, highlights, captions, pick, email, fill }

    @State private var include = Include()
    @State private var picked: Set<PersistentIdentifier> = []
    @State private var captions: [PersistentIdentifier: String] = [:]
    /// Captions the AI wrote (and the user hasn't edited)
    @State private var aiCaptioned: Set<PersistentIdentifier> = []
    /// The transcript the AI parts were made from: if it reads differently
    /// now (a Delete or Strike, here or over Sync), they may quote removed words
    @State private var basis = ""
    @State private var highlights: [ExportPackage.Highlight] = []
    @State private var email: ExportPackage.Email?
    @State private var working: String?
    @State private var errorText: String?
    @State private var shareItem: ShareItem?
    @State private var started = false
    // Pro → AI setup → consent (same steps as the recording's Make notes)
    @State private var paywallFor: ProFeature?
    @State private var showAISetup = false
    @State private var consentFor: AIProvider?
    @State private var pendingAI: Action?

    static let defaultPicks = 12
    static let aiPicks = 8

    private var snapshots: [Snapshot] {
        meeting.orderedSnapshots.filter { FileManager.default.fileExists(atPath: $0.fileURL.path) }
    }
    private var pickedSnapshots: [Snapshot] { snapshots.filter { picked.contains($0.persistentModelID) } }
    private var lines: [ExportPackage.Line] {
        RedactionText.entries(meeting).map { .init(time: max(0, $0.time.timeIntervalSince(meeting.startedAt)), text: $0.text) }
    }
    private var fingerprint: String {
        String(lines.map { "\($0.time)\t\($0.text)" }.joined(separator: "\n").hashValue)
    }
    private var hasAIParts: Bool {
        (include.highlights && !highlights.isEmpty) || (include.email && hasEmail)
            || (include.pictures && !aiCaptioned.intersection(picked).isEmpty)
    }
    private var hasTranscript: Bool { !meeting.segments.isEmpty }
    private var hasNotes: Bool { !(meeting.aiNotes ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    private var hasGuide: Bool { !meeting.studyMaterials.isEmpty }
    private var links: [MeetingLinkItem] { MeetingLinks.items(for: meeting) }
    private var hasEmail: Bool { !(email?.body ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(meeting.title).font(.headline)
                        Text(whenLine).font(.subheadline).foregroundStyle(.secondary)
                    }
                    .listRowBackground(Color.clear)
                }
                if let errorText {
                    Section { Label(errorText, systemImage: "exclamationmark.triangle").foregroundStyle(.red).font(.callout) }
                }
                contentsSection
                if include.highlights && !highlights.isEmpty { highlightsSection }
                if include.email && email != nil && meeting.kind == .meeting { emailSection }
                if !snapshots.isEmpty { picturesSection }
            }
            .scrollContentBackground(.hidden)
            .background(Theme.background)
            .navigationTitle("Export")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
                ToolbarItem(placement: .primaryAction) {
                    Button("Share…") { share() }
                        .fontWeight(.semibold)
                        .disabled(working != nil)
                        .accessibilityIdentifier("export-share")
                }
            }
            .safeAreaInset(edge: .bottom) { bottomBar }
        }
        .onAppear(perform: start)
        .sheet(item: $shareItem) { item in ActivityView(items: [item.url]).ignoresSafeArea() }
        .sheet(item: $paywallFor, onDismiss: { resume(if: store.isPro) }) { PaywallView(feature: $0) }
        .sheet(isPresented: $showAISetup, onDismiss: { resume(if: aiSettings.endpoint() != nil) }) { AISetupSheet() }
        .sheet(item: $consentFor, onDismiss: { resume(if: aiSettings.endpoint()?.consentGranted == true) }) { p in
            AIConsentSheet(provider: p) { aiSettings.grantConsent(p) }
        }
    }

    private var whenLine: String {
        var parts = [meeting.startedAt.formatted(.dateTime.weekday(.wide).month(.wide).day().hour().minute())]
        if let d = meeting.duration { parts.append(d.minutesLabel) }
        return parts.joined(separator: " · ")
    }

    // MARK: Sections

    private var contentsSection: some View {
        Section {
            row("Summary and notes", on: $include.notes, enabled: hasNotes,
                detail: hasNotes ? (meeting.aiNotesStale ? "Made before an edit" : "Made with your AI")
                    : hasTranscript ? "Not made yet" : "No transcript to make notes from",
                action: hasTranscript ? (hasNotes ? "Make again" : "Make notes") : nil) { request(.notes) }
            row("Highlights", on: $include.highlights, enabled: !highlights.isEmpty,
                detail: highlights.isEmpty ? "The AI picks the key moments and quotes them" : "\(highlights.count) key moments",
                action: hasTranscript ? (highlights.isEmpty ? "Find" : "Find again") : nil) { request(.highlights) }
            if !snapshots.isEmpty {
                row(ExportPackage.picturesNoun(snapshots.map(picture)), on: $include.pictures, enabled: !picked.isEmpty,
                    detail: "\(picked.count) of \(snapshots.count) picked", action: nil) {}
            }
            if !meeting.markers.isEmpty {
                row("Marked moments", on: $include.marks, enabled: true, detail: "\(meeting.markers.count) marked", action: nil) {}
            }
            if !links.isEmpty {
                row("Links", on: $include.links, enabled: true, detail: "\(links.count) said or added", action: nil) {}
            }
            if meeting.kind == .meeting {
                row("Follow-up email", on: $include.email, enabled: hasEmail,
                    detail: hasEmail ? "Drafted; edit it below" : "The AI drafts a recap email",
                    action: hasTranscript ? (hasEmail ? "Draft again" : "Draft") : nil) { request(.email) }
            }
            if hasGuide {
                row(meeting.kind.guideTitle, on: $include.guide, enabled: true, detail: "As made in Review", action: nil) {}
            }
            if hasTranscript {
                row("Transcript", on: $include.transcript, enabled: true, detail: "As transcript.txt", action: nil) {}
            }
        } header: {
            Text("What goes in")
        } footer: {
            Text("Shared as a .zip with the recording as Markdown and as a web page you can print to PDF.")
        }
    }

    private func row(_ title: String, on: Binding<Bool>, enabled: Bool, detail: String, action: String?,
                     _ run: @escaping () -> Void) -> some View {
        HStack(spacing: 12) {
            Toggle(isOn: Binding(get: { on.wrappedValue && enabled }, set: { on.wrappedValue = $0 })) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                    Text(detail).font(.caption).foregroundStyle(.secondary)
                }
            }
            .disabled(!enabled)
            .toggleStyle(CheckToggleStyle())
            if let action {
                Button(action, action: run)
                    .buttonStyle(.bordered)
                    .tint(Theme.ai)
                    .controlSize(.small)
                    .disabled(working != nil)
            }
        }
    }

    private var highlightsSection: some View {
        Section("Highlights") {
            ForEach($highlights) { $h in
                VStack(alignment: .leading, spacing: 4) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(h.time.clock).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                        TextField("Highlight", text: $h.label, axis: .vertical)
                    }
                    if let q = h.quote { Text("“\(q)”").font(.caption).italic().foregroundStyle(.secondary) }
                }
            }
            .onDelete { highlights.remove(atOffsets: $0) }
        }
    }

    private var emailSection: some View {
        Section("Follow-up email") {
            TextField("Subject", text: Binding(get: { email?.subject ?? "" }, set: { email?.subject = $0 }))
            TextField("Message", text: Binding(get: { email?.body ?? "" }, set: { email?.body = $0 }), axis: .vertical)
                .lineLimit(4...12)
        }
    }

    private var picturesSection: some View {
        Section {
            HStack {
                Button("Pick all") { picked = Set(snapshots.map(\.persistentModelID)) }
                Spacer()
                Button("Clear") { picked = [] }
                Spacer()
                Button("Pick with AI") { request(.pick) }.tint(Theme.ai).disabled(working != nil || !hasTranscript)
            }
            .buttonStyle(.borderless)
            .font(.callout)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 96), spacing: 8)], spacing: 8) {
                ForEach(snapshots) { s in
                    let on = picked.contains(s.persistentModelID)
                    Button {
                        if on { picked.remove(s.persistentModelID) } else { picked.insert(s.persistentModelID) }
                    } label: {
                        ExportThumb(url: s.fileURL)
                            .overlay(alignment: .topLeading) {
                                Image(systemName: on ? "checkmark.circle.fill" : "circle")
                                    .font(.title3)
                                    .foregroundStyle(on ? Theme.accent : .white)
                                    .shadow(radius: 2)
                                    .padding(5)
                            }
                            .overlay(alignment: .bottomLeading) {
                                Text(time(of: s).clock)
                                    .font(.caption2.monospacedDigit())
                                    .padding(.horizontal, 5).padding(.vertical, 1)
                                    .background(.black.opacity(0.6), in: RoundedRectangle(cornerRadius: 4))
                                    .foregroundStyle(.white)
                                    .padding(5)
                            }
                            .overlay(RoundedRectangle(cornerRadius: 8).stroke(on ? Theme.accent : .clear, lineWidth: 2))
                            .opacity(on ? 1 : 0.6)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(s.isScreen ? "Screen" : "Photo") at \(time(of: s).clock)")
                    .accessibilityAddTraits(on ? .isSelected : [])
                }
            }
            .padding(.vertical, 4)
            if !pickedSnapshots.isEmpty {
                ForEach(pickedSnapshots) { s in
                    HStack(spacing: 10) {
                        ExportThumb(url: s.fileURL).frame(width: 56)
                        TextField("Caption (optional)", text: Binding(
                            get: { captions[s.persistentModelID] ?? "" },
                            set: { captions[s.persistentModelID] = $0; aiCaptioned.remove(s.persistentModelID) }), axis: .vertical)
                            .font(.callout)
                    }
                }
                Button("Write captions with AI") { request(.captions) }
                    .tint(Theme.ai)
                    .disabled(working != nil || uncaptioned.isEmpty)
            }
        } header: {
            Text(ExportPackage.picturesNoun(snapshots.map(picture)))
        } footer: {
            Text("Tap the ones to keep. Captions come from what was said at that moment.")
        }
    }

    private var bottomBar: some View {
        VStack(spacing: 8) {
            if let working {
                HStack(spacing: 8) { ProgressView(); Text(working).font(.callout) }
            }
            Button {
                request(.fill)
            } label: {
                Label("Fill with AI", systemImage: "sparkles").frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .tint(Theme.ai)
            .disabled(working != nil || !hasTranscript)
            .accessibilityHint("Makes whatever is missing: notes, highlights, captions and the follow-up email")
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 10)
        .background(.bar)
    }

    // MARK: Data

    private func time(of s: Snapshot) -> TimeInterval { max(0, s.takenAt.timeIntervalSince(meeting.startedAt)) }

    private func picture(_ s: Snapshot) -> ExportPackage.Picture {
        .init(fileURL: s.fileURL, time: time(of: s), isPhoto: !s.isScreen,
              caption: captions[s.persistentModelID].flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 })
    }

    private var uncaptioned: [Snapshot] {
        pickedSnapshots.filter { (captions[$0.persistentModelID] ?? "").trimmingCharacters(in: .whitespaces).isEmpty }
    }

    /// Every photo, then screens spread over the recording, up to 12.
    private func start() {
        guard !started else { return }
        started = true
        basis = fingerprint
        let photos = snapshots.filter { !$0.isScreen }
        let screens = snapshots.filter(\.isScreen)
        var chosen = Array(photos.prefix(Self.defaultPicks))
        let room = Self.defaultPicks - chosen.count
        if room > 0 { chosen += ExportAI.sample(screens.count, room).map { screens[$0] } }
        picked = Set(chosen.map(\.persistentModelID))
    }

    private func load<T: Decodable>(_ kind: StudyKind, _ t: T.Type) -> T? {
        meeting.studyMaterial(kind).flatMap { StudyParse.decode(t, $0.json) }
    }

    private func input() -> ExportPackage.Input {
        var i = ExportPackage.Input(title: meeting.title)
        i.kind = meeting.kind
        i.when = meeting.startedAt.formatted(date: .complete, time: .shortened)
        i.duration = meeting.duration?.minutesLabel
        i.notebook = meeting.courseName
        i.people = meeting.people.map(\.person.displayName)
        if include.notes && hasNotes {
            i.notes = meeting.aiNotes
            i.hasAI = true
        }
        if include.highlights {
            i.highlights = highlights.filter { !$0.label.trimmingCharacters(in: .whitespaces).isEmpty }
            if !i.highlights.isEmpty { i.hasAI = true }
        }
        if include.pictures {
            i.pictures = pickedSnapshots.map(picture)
            if i.pictures.contains(where: { $0.caption != nil }) { i.hasAI = true }
        }
        if include.marks {
            i.marks = meeting.orderedMarkers.map {
                .init(time: $0.offset(in: meeting), symbol: $0.markerKind.symbol, label: $0.markerKind.label(for: meeting.kind), note: $0.note)
            }
        }
        if include.links {
            i.links = links.map { .init(url: $0.url, title: $0.title, time: $0.firstSaid.map { max(0, $0.timeIntervalSince(meeting.startedAt)) }) }
        }
        if include.email, meeting.kind == .meeting, hasEmail, let email {
            i.email = email
            i.hasAI = true
        }
        if include.guide && hasGuide {
            let cards = load(.flashcards, StudyCards.self)
            i.guideMarkdown = StudyExport.guideMarkdown(
                title: meeting.title, kind: meeting.kind, when: meeting.startedAt.formatted(date: .complete, time: .shortened),
                summary: load(.summary, StudySummary.self), terms: load(.terms, StudyTerms.self), cards: cards,
                quiz: load(.quiz, StudyQuiz.self), asks: load(.questions, StudyAsks.self),
                marks: meeting.orderedMarkers.map { .init(ms: Int(($0.offset(in: meeting) * 1000).rounded()), kind: $0.markerKind, note: $0.note) })
            if let cards, !cards.cards.isEmpty { i.flashcardsCSV = StudyExport.flashcardsCSV(cards.cards) }
            i.hasAI = true
        }
        if include.transcript { i.transcript = lines }
        return i
    }

    private func share() {
        errorText = nil
        if hasAIParts && fingerprint != basis {
            transcriptChanged()
            return
        }
        do {
            ExportPackage.clearWorkFolder()
            let name = ExportPackage.folderName(date: meeting.startedAt, title: meeting.title)
            let folder = try ExportPackage.writeFolder(input(), name: name, in: ExportPackage.workFolder)
            shareItem = ShareItem(url: try ExportPackage.zip(folder: folder))
        } catch {
            errorText = "Couldn't make the package: \(error.localizedDescription)"
        }
    }

    // MARK: AI

    private func proFeature(_ a: Action) -> ProFeature {
        switch a {
        case .notes: .notes
        case .email: .followUp
        default: .ai
        }
    }

    private func resume(if satisfied: Bool) {
        guard let a = pendingAI else { return }
        pendingAI = nil
        if satisfied { request(a) }
    }

    private func request(_ a: Action) {
        pendingAI = nil
        guard store.isPro else { pendingAI = a; paywallFor = proFeature(a); return }
        guard let endpoint = aiSettings.endpoint() else { pendingAI = a; showAISetup = true; return }
        if endpoint.needsConsent && !endpoint.consentGranted { pendingAI = a; consentFor = endpoint.provider; return }
        errorText = nil
        Task { @MainActor in
            defer { working = nil }
            do {
                checkBasis()
                switch a {
                case .notes: try await makeNotes(endpoint)
                case .highlights: try await findHighlights(endpoint)
                case .captions: try await writeCaptions(uncaptioned, endpoint)
                case .pick: try await pickWithAI(endpoint)
                case .email: try await draftEmail(endpoint)
                case .fill: try await fill(endpoint)
                }
            } catch is TranscriptChanged {
                transcriptChanged()
            } catch {
                errorText = error.localizedDescription
            }
        }
    }

    struct TranscriptChanged: LocalizedError {
        var errorDescription: String? {
            "The transcript was edited while you were exporting. The highlights, captions and email made before the edit were removed. Make them again."
        }
    }

    /// The AI parts may quote removed words: they go, and the new transcript
    /// becomes the basis.
    private func transcriptChanged() {
        highlights = []
        email = nil
        for id in aiCaptioned { captions[id] = nil }
        aiCaptioned = []
        basis = fingerprint
        errorText = TranscriptChanged().errorDescription
    }

    /// Before an AI tool runs: parts made from an older transcript go (with
    /// a note), then the tool runs on the transcript as it is now.
    private func checkBasis() {
        guard fingerprint != basis else { return }
        if hasAIParts { transcriptChanged() } else { basis = fingerprint }
    }

    private func makeNotes(_ endpoint: AIEndpoint) async throws {
        working = "Making notes…"
        let text = try await MeetingAI.notes(context: MeetingAI.context(meeting), endpoint: endpoint, kind: meeting.kind)
        meeting.aiNotes = text
        meeting.aiNotesAt = .now
        meeting.aiNotesStale = false
        try? context.save()
        include.notes = true
        // Same hook as Make notes: topics are named with the notes (a failure here is quiet)
        if let found = try? await MeetingAI.findTopics(StudyInput(meeting: meeting), contextTokens: endpoint.contextTokens,
                                                       complete: MeetingAI.liveComplete(endpoint)) {
            _ = try? TopicStore.applyAI(found, to: meeting, context: context)
        }
    }

    private func findHighlights(_ endpoint: AIEndpoint) async throws {
        working = "Finding highlights…"
        let sent = fingerprint
        let found = try await ExportAI.highlights(title: meeting.title, lines: lines, complete: MeetingAI.liveComplete(endpoint))
        guard sent == fingerprint else { throw TranscriptChanged() }
        highlights = found
        include.highlights = true
    }

    private func aiPictures(_ list: [Snapshot]) -> [ExportAI.Picture] {
        list.map { .init(time: time(of: $0), end: nil, isPhoto: !$0.isScreen) }
    }

    private func writeCaptions(_ list: [Snapshot], _ endpoint: AIEndpoint) async throws {
        guard !list.isEmpty else { return }
        working = "Writing captions…"
        let sent = fingerprint
        let found = try await ExportAI.captions(title: meeting.title, pictures: aiPictures(list), lines: lines, pick: nil,
                                                complete: MeetingAI.liveComplete(endpoint))
        guard sent == fingerprint else { throw TranscriptChanged() }
        for (i, c) in found { setAICaption(c, for: list[i].persistentModelID) }
    }

    private func pickWithAI(_ endpoint: AIEndpoint) async throws {
        working = "Picking photos and screens…"
        let all = snapshots
        let sent = fingerprint
        let found = try await ExportAI.captions(title: meeting.title, pictures: aiPictures(all), lines: lines, pick: Self.aiPicks,
                                                complete: MeetingAI.liveComplete(endpoint))
        guard sent == fingerprint else { throw TranscriptChanged() }
        picked = Set(found.map { all[$0.0].persistentModelID })
        for (i, c) in found { setAICaption(c, for: all[i].persistentModelID) }
        include.pictures = true
    }

    /// Typed captions win: the AI fills only empty ones (typing goes on while it works).
    private func setAICaption(_ c: String, for id: PersistentIdentifier) {
        guard (captions[id] ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        captions[id] = c
        aiCaptioned.insert(id)
    }

    private func draftEmail(_ endpoint: AIEndpoint) async throws {
        working = "Drafting the follow-up email…"
        let sent = fingerprint
        let text = try await MeetingAI.followUpEmail(context: MeetingAI.context(meeting), endpoint: endpoint)
        guard sent == fingerprint else { throw TranscriptChanged() }
        email = Self.splitEmail(text, fallbackSubject: meeting.title)
        include.email = true
    }

    /// Whatever is missing, one after another.
    private func fill(_ endpoint: AIEndpoint) async throws {
        if !hasNotes { try await makeNotes(endpoint) }
        if highlights.isEmpty { try await findHighlights(endpoint) }
        if !uncaptioned.isEmpty { try await writeCaptions(uncaptioned, endpoint) }
        if meeting.kind == .meeting && !hasEmail { try await draftEmail(endpoint) }
    }

    /// "Subject: …" on the first line, then the message.
    static func splitEmail(_ text: String, fallbackSubject: String) -> ExportPackage.Email {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        var lines = trimmed.components(separatedBy: .newlines)
        if let first = lines.first, first.lowercased().hasPrefix("subject:") {
            lines.removeFirst()
            let subject = first.dropFirst("subject:".count).trimmingCharacters(in: .whitespaces)
            return .init(subject: subject.isEmpty ? fallbackSubject : subject,
                         body: lines.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines))
        }
        return .init(subject: fallbackSubject, body: trimmed)
    }
}

private struct ShareItem: Identifiable {
    let url: URL
    var id: String { url.path }
}

/// A checkbox look for the "What goes in" rows.
private struct CheckToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button {
            configuration.isOn.toggle()
        } label: {
            HStack(spacing: 12) {
                Image(systemName: configuration.isOn ? "checkmark.square.fill" : "square")
                    .font(.title3)
                    .foregroundStyle(configuration.isOn ? Theme.accent : .secondary)
                configuration.label
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(configuration.isOn ? "On" : "Off")
        .accessibilityAddTraits(.isToggle)
    }
}

/// A photo or screen, downsampled off the main thread.
private struct ExportThumb: View {
    let url: URL
    @State private var image: UIImage?

    var body: some View {
        Color.white.opacity(0.05)
            .aspectRatio(4 / 3, contentMode: .fit)
            .overlay {
                if let image { Image(uiImage: image).resizable().scaledToFill() }
            }
            .clipShape(RoundedRectangle(cornerRadius: 8))
            .task(id: url) {
                image = await Task.detached(priority: .userInitiated) {
                    UIImage(contentsOfFile: url.path)?.preparingThumbnail(of: CGSize(width: 320, height: 240))
                }.value
            }
    }
}

/// The share sheet for the package.
private struct ActivityView: UIViewControllerRepresentable {
    let items: [Any]
    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }
    func updateUIViewController(_ vc: UIActivityViewController, context: Context) {}
}
