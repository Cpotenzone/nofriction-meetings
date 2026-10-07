import SwiftData
import SwiftUI

extension MarkerKind {
    var color: Color {
        switch self {
        case .important: Theme.accent
        case .question: Color(red: 96 / 255, green: 165 / 255, blue: 250 / 255)
        case .test: Color(red: 52 / 255, green: 211 / 255, blue: 153 / 255)
        }
    }
}

// MARK: - Recording: Mark

/// Under the transcript while recording: one tap marks ★ Important; for a
/// few seconds after, change the type or add a note.
struct MarkControl: View {
    @Environment(RecordingSession.self) private var session
    @State private var last: MomentMarker?
    @State private var noteDraft = ""
    @State private var editingNote = false
    @State private var hideTask: Task<Void, Never>?

    var body: some View {
        VStack(spacing: 8) {
            if let last, last.modelContext != nil {
                HStack(spacing: 6) {
                    ForEach(MarkerKind.allCases, id: \.self) { k in
                        Button {
                            last.setKind(k)
                            try? last.modelContext?.save()
                            keepOpen()
                        } label: {
                            Label(k.label, systemImage: k.systemImage)
                                .font(.caption.weight(last.markerKind == k ? .semibold : .regular))
                        }
                        .buttonStyle(.bordered)
                        .tint(last.markerKind == k ? k.color : .secondary)
                        .accessibilityAddTraits(last.markerKind == k ? .isSelected : [])
                    }
                    Button {
                        noteDraft = last.note ?? ""
                        editingNote = true
                        hideTask?.cancel()
                    } label: {
                        Image(systemName: last.note == nil ? "note.text.badge.plus" : "note.text")
                    }
                    .buttonStyle(.bordered)
                    .accessibilityLabel(last.note == nil ? "Add a note" : "Edit the note")
                }
                .transition(.opacity)
                .accessibilityIdentifier("mark-kinds")
            }
            Button {
                mark()
            } label: {
                Label("Mark this moment", systemImage: "bookmark.fill")
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: 320)
            }
            .buttonStyle(.bordered)
            .tint(Theme.accent)
            .disabled(session.phase != .recording && session.phase != .paused)
            .accessibilityHint("Marks it Important. Change it to Question or On the test right after.")
            .accessibilityIdentifier("mark-button")
        }
        .padding(.horizontal, 20)
        .padding(.bottom, 4)
        .animation(.easeOut(duration: 0.2), value: last?.id)
        .alert("Note for this moment", isPresented: $editingNote) {
            TextField("Optional, e.g. “ask about step 3”", text: $noteDraft)
            Button("Save") {
                last?.setNote(noteDraft)
                try? last?.modelContext?.save()
                keepOpen()
            }
            Button("Cancel", role: .cancel) { keepOpen() }
        }
        .onChange(of: session.phase) { _, phase in
            if phase == .idle { last = nil }
        }
    }

    private func mark() {
        guard let m = session.addMarker() else { return }
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        UIAccessibility.post(notification: .announcement, argument: "Marked important")
        last = m
        keepOpen()
    }

    private func keepOpen() {
        hideTask?.cancel()
        hideTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(6))
            if !Task.isCancelled && !editingNote { last = nil }
        }
    }
}

// MARK: - Meeting: marked moments

struct MarkersSection: View {
    let meeting: Meeting
    let onJump: (Date) -> Void
    @Environment(\.modelContext) private var context
    @State private var filter: MarkerKind?
    @State private var editing: MomentMarker?
    @State private var noteDraft = ""

    var body: some View {
        let all = meeting.orderedMarkers
        let shown = all.filter { filter == nil || $0.markerKind == filter }
        SectionBlock(title: "Marked moments") {
            VStack(alignment: .leading, spacing: 8) {
                Picker("Show", selection: $filter) {
                    Text("All \(all.count)").tag(MarkerKind?.none)
                    ForEach(MarkerKind.allCases, id: \.self) { k in
                        Text("\(k.symbol) \(all.filter { $0.markerKind == k }.count)").tag(MarkerKind?.some(k))
                    }
                }
                .pickerStyle(.segmented)
                .accessibilityLabel("Show markers of type")
                if shown.isEmpty {
                    Text("None of this type.").font(.footnote).foregroundStyle(.secondary)
                }
                ForEach(shown) { m in
                    HStack(spacing: 10) {
                        Button { onJump(m.at) } label: {
                            Label(m.offset(in: meeting).clock, systemImage: m.markerKind.systemImage)
                                .font(.footnote.monospacedDigit())
                                .foregroundStyle(m.markerKind.color)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("\(m.markerKind.label) at \(m.offset(in: meeting).clock)")
                        .accessibilityHint("Shows this moment in the transcript")
                        Text(m.note ?? m.markerKind.label)
                            .font(.subheadline)
                            .foregroundStyle(m.note == nil ? .secondary : .primary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Menu {
                            ForEach(MarkerKind.allCases, id: \.self) { k in
                                Button(k.label, systemImage: k.systemImage) { m.setKind(k); try? context.save() }
                            }
                            Button(m.note == nil ? "Add note" : "Edit note", systemImage: "note.text") {
                                noteDraft = m.note ?? ""
                                editing = m
                            }
                            Button("Delete marker", systemImage: "trash", role: .destructive) {
                                context.delete(m)
                                try? context.save()
                            }
                        } label: {
                            Image(systemName: "ellipsis.circle").foregroundStyle(.secondary)
                        }
                        .accessibilityLabel("Marker options")
                    }
                    .padding(.vertical, 2)
                }
            }
            .padding(12)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .alert("Note", isPresented: Binding(get: { editing != nil }, set: { if !$0 { editing = nil } })) {
            TextField("Note", text: $noteDraft)
            Button("Save") { editing?.setNote(noteDraft); try? context.save(); editing = nil }
            Button("Cancel", role: .cancel) { editing = nil }
        }
    }
}

/// A marker shown in the transcript at its time.
struct MarkerInlineRow: View {
    let marker: MomentMarker
    let meeting: Meeting

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: marker.markerKind.systemImage).foregroundStyle(marker.markerKind.color)
            Text(marker.markerKind.label).font(.caption.weight(.semibold))
            Text(marker.offset(in: meeting).clock).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            if let n = marker.note { Text(n).font(.caption).foregroundStyle(.secondary).lineLimit(2) }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
        .background(marker.markerKind.color.opacity(0.08), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

// MARK: - Meeting: study guide

struct StudySection: View {
    let meeting: Meeting
    let working: String?
    let progress: MeetingAI.StudyProgress?
    let failures: [String]
    let onMake: () -> Void
    let onOpen: () -> Void

    var body: some View {
        let has = !meeting.studyMaterials.isEmpty
        let tests = meeting.markers.filter { $0.markerKind == .test }.count
        SectionBlock(title: "Study") {
            VStack(alignment: .leading, spacing: 10) {
                if has {
                    Button(action: onOpen) {
                        Label("Open study guide", systemImage: "graduationcap")
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(Theme.ai)
                    .accessibilityIdentifier("study-open")
                } else {
                    Text("Notes, key terms, flashcards, a practice quiz and questions to ask, made from this transcript by your AI"
                         + (tests > 0 ? ", with extra weight on the \(tests) moment\(tests == 1 ? "" : "s") you marked On the test." : "."))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                if let working {
                    HStack(spacing: 8) {
                        ProgressView()
                        Text(progress?.label ?? working).font(.footnote).foregroundStyle(.secondary)
                    }
                    if let p = progress, p.total > 0 {
                        ProgressView(value: Double(p.done), total: Double(p.total)).tint(Theme.ai)
                            .accessibilityLabel("Study guide progress")
                    }
                }
                ForEach(failures, id: \.self) { f in
                    Label(f, systemImage: "exclamationmark.triangle").font(.footnote).foregroundStyle(.orange)
                }
                Button(has ? "Remake study guide" : "Make study guide", systemImage: "sparkles", action: onMake)
                    .buttonStyle(.bordered)
                    .tint(Theme.ai)
                    .disabled(working != nil)
                    .font(.subheadline)
                    .accessibilityIdentifier("study-make")
            }
            .padding(14)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
    }
}

struct StudyGuideView: View {
    let meeting: Meeting
    let onJump: (Date) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var part: StudyKind = .summary

    private func load<T: Decodable>(_ kind: StudyKind, _ t: T.Type) -> T? {
        meeting.studyMaterial(kind).flatMap { StudyParse.decode(t, $0.json) }
    }

    private func jump(_ ms: Int) {
        dismiss()
        onJump(meeting.startedAt.addingTimeInterval(Double(ms) / 1000))
    }

    var body: some View {
        let summary = load(.summary, StudySummary.self)
        let terms = load(.terms, StudyTerms.self)
        let cards = load(.flashcards, StudyCards.self)
        let quiz = load(.quiz, StudyQuiz.self)
        let asks = load(.questions, StudyAsks.self)
        NavigationStack {
            VStack(spacing: 0) {
                Picker("Part", selection: $part) {
                    ForEach(StudyKind.allCases, id: \.self) { Text($0.label).tag($0) }
                }
                .pickerStyle(.segmented)
                .padding()
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        switch part {
                        case .summary:
                            if let summary {
                                if let t = summary.title { Text(t).font(.title3.weight(.semibold)) }
                                ForEach(Array(summary.sections.enumerated()), id: \.offset) { _, s in
                                    VStack(alignment: .leading, spacing: 6) {
                                        Text(s.heading).font(.headline)
                                        ForEach(Array(s.bullets.enumerated()), id: \.offset) { _, b in
                                            Text("• " + b).font(.callout)
                                        }
                                    }
                                }
                            } else { missing }
                        case .terms:
                            if let terms {
                                ForEach(Array(terms.terms.enumerated()), id: \.offset) { _, t in
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(t.term).font(.headline)
                                        Text(t.definition).font(.callout).foregroundStyle(.secondary)
                                    }
                                }
                            } else { missing }
                        case .flashcards:
                            if let cards, !cards.cards.isEmpty { FlashcardsView(cards: cards.cards) } else { missing }
                        case .quiz:
                            if let quiz, !quiz.questions.isEmpty { QuizView(questions: quiz.questions, onJump: jump) } else { missing }
                        case .questions:
                            asksView(asks)
                        }
                    }
                    .textSelection(.enabled)
                    .frame(maxWidth: 720, alignment: .leading)
                    .padding(.horizontal, 20)
                    .padding(.bottom, 24)
                    .frame(maxWidth: .infinity)
                }
            }
            .background(Theme.background)
            .navigationTitle("Study guide")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
                ToolbarItem(placement: .primaryAction) {
                    Menu {
                        if let cards, !cards.cards.isEmpty {
                            ShareLink(item: StudyExportFile.csv(cards.cards, title: meeting.title),
                                      preview: SharePreview("Flashcards (CSV)")) {
                                Label("Flashcards for Anki or Quizlet (CSV)", systemImage: "rectangle.on.rectangle")
                            }
                        }
                        ShareLink(item: StudyExportFile.markdown(markdown(summary, terms, cards, quiz, asks), title: meeting.title),
                                  preview: SharePreview("Study guide (Markdown)")) {
                            Label("Study guide (Markdown)", systemImage: "doc.text")
                        }
                    } label: {
                        Image(systemName: "square.and.arrow.up")
                    }
                    .accessibilityLabel("Export")
                }
            }
        }
    }

    private var missing: some View {
        Text("This part wasn't made. Close this and use Remake study guide.").font(.footnote).foregroundStyle(.secondary)
    }

    @ViewBuilder private func asksView(_ asks: StudyAsks?) -> some View {
        let confused = meeting.orderedMarkers.filter { $0.markerKind == .question }
        if asks == nil && confused.isEmpty { missing }
        ForEach(Array((asks?.questions ?? []).enumerated()), id: \.offset) { _, q in
            HStack(alignment: .firstTextBaseline) {
                Text("• " + q.question).font(.callout)
                Spacer(minLength: 8)
                if let at = q.atMs {
                    Button(StudyParse.clock(at)) { jump(at) }.font(.caption.monospacedDigit())
                }
            }
        }
        if !confused.isEmpty {
            Text("You marked as confusing").font(.headline).padding(.top, 8)
            ForEach(confused) { m in
                HStack(alignment: .firstTextBaseline) {
                    Text("• " + (m.note ?? "No note")).font(.callout)
                    Spacer(minLength: 8)
                    Button(m.offset(in: meeting).clock) { dismiss(); onJump(m.at) }.font(.caption.monospacedDigit())
                }
            }
        }
    }

    private func markdown(_ s: StudySummary?, _ t: StudyTerms?, _ c: StudyCards?, _ q: StudyQuiz?, _ a: StudyAsks?) -> String {
        StudyExport.guideMarkdown(
            title: meeting.title,
            when: meeting.startedAt.formatted(date: .complete, time: .shortened),
            summary: s, terms: t, cards: c, quiz: q, asks: a,
            marks: meeting.orderedMarkers.map { .init(ms: Int(($0.offset(in: meeting) * 1000).rounded()), kind: $0.markerKind, note: $0.note) })
    }
}

struct FlashcardsView: View {
    let cards: [StudyCards.Card]
    @State private var deck: FlashcardDeckState

    init(cards: [StudyCards.Card]) {
        self.cards = cards
        _deck = State(initialValue: FlashcardDeckState(count: cards.count))
    }

    var body: some View {
        VStack(spacing: 14) {
            HStack {
                Text("\(deck.known.count) of \(cards.count) known" + (deck.round > 1 ? " · round \(deck.round)" : ""))
                    .font(.footnote).foregroundStyle(.secondary)
                Spacer()
                Button("Shuffle") { var g = SystemRandomNumberGenerator(); deck.shuffle(using: &g) }.disabled(deck.isDone)
                Button("Start over") { deck = FlashcardDeckState(count: cards.count) }
            }
            .font(.footnote)
            if let i = deck.current, cards.indices.contains(i) {
                let c = cards[i]
                Button {
                    withAnimation(.easeInOut(duration: 0.2)) { deck.flip() }
                } label: {
                    VStack(spacing: 10) {
                        Text(deck.flipped ? "ANSWER" : "QUESTION").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                        Text(deck.flipped ? c.back : c.front).font(.title3).multilineTextAlignment(.center)
                        if !deck.flipped { Text("Tap to flip").font(.caption2).foregroundStyle(.tertiary) }
                    }
                    .frame(maxWidth: .infinity, minHeight: 200)
                    .padding(20)
                    .background(deck.flipped ? Theme.card : Theme.card.opacity(0.6), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).stroke(deck.flipped ? Theme.ai : Theme.hairline))
                }
                .buttonStyle(.plain)
                .accessibilityLabel(deck.flipped ? "Answer: \(c.back)" : "Question: \(c.front)")
                .accessibilityHint("Flips the card")
                .accessibilityIdentifier("flashcard")
                HStack(spacing: 12) {
                    Button("Again") { deck.mark(known: false) }.buttonStyle(.bordered)
                    Button("Known") { deck.mark(known: true) }.buttonStyle(.borderedProminent).tint(Theme.ai)
                }
                .disabled(!deck.flipped)
            } else {
                VStack(spacing: 10) {
                    Text("All \(cards.count) cards known.").font(.headline)
                    Button("Study again") { deck = FlashcardDeckState(count: cards.count) }.buttonStyle(.bordered)
                }
                .padding(.vertical, 30)
            }
        }
    }
}

struct QuizView: View {
    let questions: [StudyQuiz.Item]
    let onJump: (Int) -> Void
    @State private var state: QuizRunState

    init(questions: [StudyQuiz.Item], onJump: @escaping (Int) -> Void) {
        self.questions = questions
        self.onJump = onJump
        _state = State(initialValue: QuizRunState(count: questions.count))
    }

    var body: some View {
        if state.finished {
            let s = state.score(in: questions)
            VStack(spacing: 12) {
                Text("\(s.correct) of \(s.total) right").font(.title2.weight(.semibold))
                Button("Start over") { state = QuizRunState(count: questions.count) }.buttonStyle(.bordered)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 30)
            .accessibilityElement(children: .combine)
        } else if questions.indices.contains(state.pos) {
            let q = questions[state.pos]
            let picked = state.picked[state.pos]
            VStack(alignment: .leading, spacing: 12) {
                Text("Question \(state.pos + 1) of \(questions.count)").font(.footnote).foregroundStyle(.secondary)
                Text(q.question).font(.headline)
                ForEach(Array(q.choices.enumerated()), id: \.offset) { i, choice in
                    Button {
                        state.pick(i, in: questions)
                    } label: {
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Text(StudyExport.letter(i)).font(.callout.monospaced()).foregroundStyle(.secondary)
                            Text(choice).frame(maxWidth: .infinity, alignment: .leading)
                            if picked != nil && i == q.answer { Image(systemName: "checkmark.circle.fill").foregroundStyle(.green) }
                            if picked == i && i != q.answer { Image(systemName: "xmark.circle.fill").foregroundStyle(.red) }
                        }
                        .padding(12)
                        .background(Theme.card, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    }
                    .buttonStyle(.plain)
                    .disabled(picked != nil)
                    .accessibilityAddTraits(picked == i ? .isSelected : [])
                }
                if let picked {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(picked == q.answer ? "Right." : "Not quite: the answer is \(StudyExport.letter(q.answer)).")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(picked == q.answer ? .green : .orange)
                        if !q.explanation.isEmpty { Text(q.explanation).font(.subheadline) }
                        if let at = q.atMs {
                            Button("Jump to this moment (\(StudyParse.clock(at)))") { onJump(at) }.font(.subheadline)
                        }
                    }
                    Button(state.pos + 1 >= questions.count ? "See score" : "Next question") { state.next(in: questions) }
                        .buttonStyle(.borderedProminent)
                        .tint(Theme.ai)
                }
            }
        }
    }
}
