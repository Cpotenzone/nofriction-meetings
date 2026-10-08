import SwiftData
import SwiftUI

/// The Chat tab (docs/TOPICS_AND_CHAT.md): ask about your recordings. The
/// scope at the top (All recordings · this Notebook · this Topic · this
/// recording) decides which recordings are searched; every answer shows
/// the scope it was made in and the passages it cited; a citation opens
/// the recording at that moment. Pro, provider and consent checks are the
/// same as for notes (`MeetingAI` / `AIClient`), with the same sheets.
struct ChatView: View {
    @Environment(\.modelContext) private var context
    @Environment(Store.self) private var store
    @Environment(AISettings.self) private var aiSettings
    @Environment(RecordingSession.self) private var session
    @Query(sort: \ChatThread.updatedAt, order: .reverse) private var threads: [ChatThread]
    @Query(sort: \Meeting.startedAt, order: .reverse) private var meetings: [Meeting]
    @State private var thread: ChatThread?
    @State private var scope: ChatScope = .all
    @State private var draft = ""
    @State private var working = false
    @State private var error: String?
    @State private var showThreads = false
    @State private var openCitation: ChatCitation?
    @State private var showPaywall = false
    @State private var showAISetup = false
    @State private var consentFor: AIProvider?
    @State private var pendingQuestion: String?
    @FocusState private var composing: Bool

    /// Saved recordings (not the one being recorded)
    private var saved: [Meeting] { meetings.filter { $0.id != session.meeting?.id && !$0.segments.isEmpty } }
    private var topicIndex: TopicIndex { TopicIndex(entries: Meeting.topicEntries(saved)) }
    private var notebooks: [String] { Notebook.recent(saved.map { ($0.courseName, $0.startedAt) }, limit: 30) }

    /// The recordings the scope covers
    private var scoped: [Meeting] { scope.filter(saved, topics: topicIndex) }

    private var messages: [ChatThreadMessage] { thread?.orderedMessages ?? [] }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                scopeBar
                Divider().overlay(Theme.hairline)
                conversation
                composer
            }
            .background(Theme.background)
            .navigationTitle("Chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Chats", systemImage: "list.bullet") { showThreads = true }
                        .accessibilityIdentifier("chat-threads")
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("New chat", systemImage: "square.and.pencil") { newChat() }
                        .accessibilityIdentifier("chat-new")
                }
            }
            .navigationDestination(item: $openCitation) { c in
                if let m = meetings.first(where: { $0.id == c.meetingID }) {
                    MeetingDetailView(meeting: m, jumpTo: c.timestamp)
                } else {
                    ContentUnavailableView("This recording was deleted", systemImage: "trash")
                        .background(Theme.background)
                }
            }
            .sheet(isPresented: $showThreads) {
                ChatThreadList(threads: threads, current: thread?.id, select: { t in
                    thread = t
                    scope = t.scope
                    showThreads = false
                }, delete: { t in
                    if thread?.id == t.id { thread = nil }
                    ChatStore.delete(t, context: context)
                })
            }
            .sheet(isPresented: $showPaywall, onDismiss: { resumePending(if: store.isPro) }) { PaywallView() }
            .sheet(isPresented: $showAISetup, onDismiss: { resumePending(if: aiSettings.endpoint() != nil) }) { AISetupSheet() }
            .sheet(item: $consentFor, onDismiss: { resumePending(if: aiSettings.endpoint()?.consentGranted == true) }) { p in
                AIConsentSheet(provider: p) { aiSettings.grantConsent(p) }
            }
            .onAppear {
                if thread == nil, let t = threads.first { thread = t; scope = t.scope }
                // A thread's recording may be gone: fall back to all
                if case .recording(let id, _) = scope, !meetings.contains(where: { $0.id == id }) { scope = .all }
            }
        }
    }

    // MARK: Scope

    private var scopeBar: some View {
        HStack(spacing: 10) {
            Menu {
                Button { scope = .all } label: { Label(ChatScope.allLabel, systemImage: "rectangle.stack") }
                if !notebooks.isEmpty {
                    Menu("This \(Notebook.label)", systemImage: "book.closed") {
                        ForEach(notebooks, id: \.self) { n in Button(n) { scope = .notebook(n) } }
                    }
                }
                if !topicIndex.groups.isEmpty {
                    Menu("This \(Topic.label)", systemImage: "tag") {
                        ForEach(topicIndex.groups.prefix(30)) { g in Button(g.label) { scope = .topic(key: g.key, label: g.label) } }
                    }
                }
                if !saved.isEmpty {
                    Menu("This recording", systemImage: "waveform") {
                        ForEach(saved.prefix(30)) { m in
                            Button { scope = .recording(id: m.id, title: m.title) } label: {
                                Text("\(m.title) · \(m.startedAt.formatted(date: .abbreviated, time: .omitted))")
                            }
                        }
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: scope.systemImage)
                    Text(scope.label).lineLimit(1)
                    Image(systemName: "chevron.down").font(.caption2)
                }
                .font(.subheadline.weight(.medium))
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background(Theme.card, in: Capsule())
            }
            .accessibilityLabel("Scope: \(scope.label)")
            .accessibilityIdentifier("chat-scope")
            Spacer(minLength: 0)
            Text(scoped.count == 1 ? "1 recording" : "\(scoped.count) recordings")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
    }

    // MARK: Conversation

    private var conversation: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    if let thread, thread.flagged, let note = thread.flagNote {
                        Label(note, systemImage: "exclamationmark.triangle")
                            .font(.footnote).foregroundStyle(.orange)
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(Theme.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                            .accessibilityIdentifier("chat-flag")
                    }
                    if messages.isEmpty {
                        emptyState
                    }
                    ForEach(messages) { m in
                        ChatBubble(message: m, onCitation: { openCitation = $0 })
                            .id(m.id)
                    }
                    if working {
                        HStack(spacing: 8) {
                            ProgressView()
                            Text("Reading your recordings…").font(.footnote).foregroundStyle(.secondary)
                        }
                        .id("working")
                    }
                    if let error {
                        Label(error, systemImage: "exclamationmark.triangle")
                            .font(.footnote).foregroundStyle(.orange)
                            .accessibilityIdentifier("chat-error")
                    }
                }
                .padding(16)
                .frame(maxWidth: 760, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: messages.count) { _, _ in
                if let last = messages.last { withAnimation { proxy.scrollTo(last.id, anchor: .bottom) } }
            }
            .onChange(of: working) { _, w in
                if w { withAnimation { proxy.scrollTo("working", anchor: .bottom) } }
            }
        }
    }

    private var suggestions: [String] {
        let recs = scoped.map { ChatSuggestions.Recording(title: $0.title, kind: $0.kind, startedAt: $0.startedAt) }
        let topics: [String]
        switch scope {
        case .recording(let id, _): topics = saved.first { $0.id == id }?.orderedTopics.map(\.label) ?? []
        case .topic: topics = []
        default: topics = TopicIndex(entries: Meeting.topicEntries(scoped)).groups.map(\.label)
        }
        return ChatSuggestions.questions(scope: scope, recordings: recs, topics: topics)
    }

    @ViewBuilder
    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 12) {
            if saved.isEmpty {
                ContentUnavailableView("Nothing to ask about yet", systemImage: "bubble.left.and.text.bubble.right",
                                       description: Text("Record something first. Chat answers from your transcripts, notes and marked moments, and cites the moment."))
            } else {
                Text("Ask about what was said, decided or assigned. Answers come only from the recordings in the scope above and cite the moment.")
                    .font(.footnote).foregroundStyle(.secondary)
                if scoped.isEmpty {
                    Text("No recordings in this scope.").font(.footnote).foregroundStyle(.orange)
                }
                ForEach(suggestions, id: \.self) { q in
                    Button { ask(q) } label: {
                        HStack {
                            Text(q).multilineTextAlignment(.leading)
                            Spacer(minLength: 0)
                            Image(systemName: "arrow.up.circle").foregroundStyle(Theme.ai)
                        }
                        .font(.subheadline)
                        .padding(12)
                        .background(Theme.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                    .buttonStyle(.plain)
                    .disabled(scoped.isEmpty)
                    .accessibilityIdentifier("chat-suggestion")
                }
            }
        }
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Ask about your recordings", text: $draft, axis: .vertical)
                .lineLimit(1...5)
                .focused($composing)
                .padding(.horizontal, 14).padding(.vertical, 10)
                .background(Theme.card, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .accessibilityIdentifier("chat-input")
                .onSubmit { ask(draft) }
            Button { ask(draft) } label: {
                Image(systemName: "arrow.up.circle.fill").font(.system(size: 30))
            }
            .tint(Theme.ai)
            .disabled(working || draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            .accessibilityLabel("Send")
            .accessibilityIdentifier("chat-send")
        }
        .padding(.horizontal, 16).padding(.vertical, 10)
        .background(Theme.background)
    }

    // MARK: Actions

    private func newChat() {
        thread = nil
        error = nil
        draft = ""
    }

    /// Pro → provider configured → consent → run (same steps as a recording's AI buttons).
    private func ask(_ question: String) {
        let q = String(question.trimmingCharacters(in: .whitespacesAndNewlines).prefix(ChatStore.maxQuestionLength))
        guard !q.isEmpty, !working else { return }
        pendingQuestion = nil
        guard store.isPro else { pendingQuestion = q; showPaywall = true; return }
        guard let endpoint = aiSettings.endpoint() else { pendingQuestion = q; showAISetup = true; return }
        if endpoint.needsConsent && !endpoint.consentGranted { pendingQuestion = q; consentFor = endpoint.provider; return }
        guard !scoped.isEmpty else { error = MeetingAI.ChatFailure.nothingToSearch.errorDescription; return }

        let t = thread ?? ChatStore.newThread(scope: scope, context: context)
        thread = t
        draft = ""
        error = nil
        let memory = ChatStore.memory(t)
        ChatStore.append(ChatThreadMessage(role: .user, content: q, scopeLabel: scope.label), to: t, context: context)
        let sources = scoped.map(ChatSource.init(meeting:))
        let scopeLabel = scope.label
        working = true
        Task { @MainActor in
            defer { working = false }
            do {
                let answer = try await MeetingAI.chat(question: q, scopeLabel: scopeLabel, sources: sources, memory: memory,
                                                      contextTokens: endpoint.contextTokens, complete: MeetingAI.liveComplete(endpoint))
                ChatStore.append(ChatThreadMessage(role: .assistant, content: answer.text, scopeLabel: scopeLabel, citations: answer.citations),
                                 to: t, context: context)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private func resumePending(if satisfied: Bool) {
        guard let q = pendingQuestion else { return }
        pendingQuestion = nil
        if satisfied { ask(q) } else { draft = q }
    }
}

/// One turn: the question right-aligned, the answer with its scope and citation chips.
struct ChatBubble: View {
    let message: ChatThreadMessage
    let onCitation: (ChatCitation) -> Void

    var body: some View {
        if message.isUser {
            HStack {
                Spacer(minLength: 40)
                Text(message.content)
                    .font(.body)
                    .padding(.horizontal, 14).padding(.vertical, 10)
                    .background(Theme.ai.opacity(0.22), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .textSelection(.enabled)
            }
            .accessibilityElement(children: .combine)
            .accessibilityLabel("You: \(message.content)")
        } else {
            VStack(alignment: .leading, spacing: 8) {
                Text(MarkdownText.attributed(message.content))
                    .font(.callout)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                let cites = message.citations
                if !cites.isEmpty {
                    FlowLayout(spacing: 6) {
                        ForEach(cites) { c in
                            Button { onCitation(c) } label: {
                                HStack(spacing: 4) {
                                    Text("[\(c.n)]").font(.caption2.weight(.bold).monospacedDigit())
                                    Text(c.title).lineLimit(1)
                                    Text(c.kind == ChatPassage.Kind.notes.rawValue ? "notes" : c.offset.clock)
                                        .font(.caption2.monospacedDigit()).foregroundStyle(.secondary)
                                }
                                .font(.caption)
                                .padding(.horizontal, 9).padding(.vertical, 5)
                                .background(Theme.card, in: Capsule())
                                .overlay { Capsule().stroke(Theme.hairline, lineWidth: 1) }
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Citation \(c.n): \(c.title), \(c.kind) \(c.offset.clock)")
                            .accessibilityHint("Opens the recording at that moment")
                            .accessibilityIdentifier("chat-citation")
                        }
                    }
                }
                if let scope = message.scopeLabel {
                    Text("Scope: \(scope) · \(message.createdAt.formatted(date: .abbreviated, time: .shortened))")
                        .font(.caption2).foregroundStyle(.tertiary)
                        .accessibilityIdentifier("chat-answer-scope")
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
    }
}

/// Past chats: pick one, or swipe to delete.
struct ChatThreadList: View {
    let threads: [ChatThread]
    let current: UUID?
    let select: (ChatThread) -> Void
    let delete: (ChatThread) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                ForEach(threads) { t in
                    Button { select(t) } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            HStack(spacing: 6) {
                                Text(t.title).font(.body.weight(t.id == current ? .semibold : .regular)).lineLimit(1)
                                if t.flagged { Image(systemName: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange) }
                            }
                            Text("\(t.scope.label) · \(t.updatedAt.formatted(date: .abbreviated, time: .shortened))")
                                .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    .tint(.primary)
                    .swipeActions {
                        Button("Delete", systemImage: "trash", role: .destructive) { delete(t) }
                    }
                }
            }
            .overlay {
                if threads.isEmpty { ContentUnavailableView("No chats yet", systemImage: "bubble.left.and.text.bubble.right") }
            }
            .navigationTitle("Chats")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
        }
    }
}

/// Inline Markdown for AI text (bold, italics, links; lists stay as lines).
enum MarkdownText {
    static func attributed(_ s: String) -> AttributedString {
        (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
            ?? AttributedString(s)
    }
}
