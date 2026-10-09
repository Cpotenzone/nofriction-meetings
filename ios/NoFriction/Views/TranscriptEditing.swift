import SwiftData
import SwiftUI

// Editing UI for Delete / Strike from the record (docs/REDACTION.md).

// MARK: - Markers

/// "Stricken from the record": a dark bar with when it covered, when it was
/// stricken and the reason. Not selectable; there is nothing behind it.
struct StrickenBar: View {
    let redaction: Redaction?

    var body: some View {
        HStack(spacing: 10) {
            Rectangle().fill(Theme.accent).frame(width: 3).accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                Label("Stricken from the record", systemImage: "eye.slash")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.92))
                if let caption = redaction.map(StrikeCaption.text) {
                    Text(caption)
                        .font(.caption2.monospacedDigit())
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            .padding(.vertical, 9)
            Spacer(minLength: 0)
        }
        .background(Color.black, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(Theme.hairline))
        .accessibilityElement(children: .combine)
        .accessibilityHint("The original was permanently removed")
        .accessibilityIdentifier("stricken-marker")
    }
}

enum StrikeCaption {
    /// `10:42–10:43 · stricken Oct 1, 2026 · "privileged"`
    static func text(_ r: Redaction) -> String {
        var parts: [String] = []
        if let a = r.coveredFrom {
            let from = a.formatted(date: .omitted, time: .shortened)
            let to = (r.coveredTo ?? a).formatted(date: .omitted, time: .shortened)
            parts.append(from == to ? from : "\(from)–\(to)")
        }
        parts.append("stricken " + r.createdAt.formatted(date: .abbreviated, time: .omitted))
        if let reason = r.reason, !reason.isEmpty { parts.append("“\(reason)”") }
        return parts.joined(separator: " · ")
    }
}

/// A struck screen: hatched placeholder card with the label and capture time.
struct StrickenScreenCard: View {
    let redaction: Redaction

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            Color.black
                .aspectRatio(4 / 3, contentMode: .fit)
                .overlay {
                    Canvas { ctx, size in
                        var path = Path()
                        let step: CGFloat = 10
                        var x: CGFloat = -size.height
                        while x < size.width {
                            path.move(to: CGPoint(x: x, y: size.height))
                            path.addLine(to: CGPoint(x: x + size.height, y: 0))
                            x += step
                        }
                        ctx.stroke(path, with: .color(.white.opacity(0.07)), lineWidth: 2)
                    }
                }
                .overlay {
                    VStack(spacing: 4) {
                        Image(systemName: "eye.slash").font(.headline)
                        Text("Stricken from the record").font(.caption.weight(.semibold))
                        if let reason = redaction.reason, !reason.isEmpty {
                            Text("“\(reason)”").font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                        }
                    }
                    .multilineTextAlignment(.center)
                    .padding(8)
                }
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Theme.hairline))
            if let at = redaction.coveredFrom {
                Text(at.formatted(date: .omitted, time: .shortened))
                    .font(.caption2.monospacedDigit())
                    .padding(.horizontal, 6).padding(.vertical, 3)
                    .background(.white.opacity(0.08), in: Capsule())
                    .padding(6)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("stricken-screen")
    }
}

// MARK: - Transcript rows

/// One transcript line in the meeting view: text runs and markers, a
/// checkbox in select mode, tap to edit its words.
struct EditableTranscriptRow: View {
    let segment: Segment
    let meeting: Meeting
    let selecting: Bool
    let selected: Bool
    /// Time column grows with Dynamic Type so "10:42" never wraps
    @ScaledMetric(relativeTo: .caption2) var timeWidth: CGFloat = 52

    var body: some View {
        let pieces = RedactionText.pieces(segment.text)
        HStack(alignment: .firstTextBaseline, spacing: 14) {
            if selecting {
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(selected ? Theme.accent : .secondary)
                    .opacity(hasWords(pieces) ? 1 : 0.25)
                    .accessibilityLabel(selected ? "Selected" : "Not selected")
            }
            Text(segment.start.formatted(date: .omitted, time: .shortened))
                .font(.system(.caption2, design: .monospaced))
                .foregroundStyle(.tertiary)
                .frame(width: timeWidth, alignment: .trailing)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Array(pieces.enumerated()), id: \.offset) { _, piece in
                    switch piece {
                    case .text(let t):
                        Text(t).font(.body).lineSpacing(3)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    case .marker(let id):
                        StrickenBar(redaction: meeting.redaction(id: id))
                            .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] }
                    }
                }
            }
        }
        .contentShape(Rectangle())
        .accessibilityAddTraits(selecting && selected ? .isSelected : [])
    }

    private func hasWords(_ pieces: [RedactionText.Piece]) -> Bool {
        pieces.contains { if case .text = $0 { return true } else { return false } }
    }
}

// MARK: - Word editor

/// Tap words to select a run inside the line (tap a second word to extend,
/// tap the selection again to clear), or select the whole line. Then Delete or
/// Strike from the record…
struct WordEditorSheet: View {
    let segment: Segment
    let meeting: Meeting
    @Environment(\.dismiss) private var dismiss
    @Environment(\.modelContext) private var context
    @Environment(RedactionCenter.self) private var redactions
    @State private var range: ClosedRange<Int>?
    @State private var wholeLine = false
    @State private var confirmStrike = false

    private var tokens: [RedactionText.Token] { RedactionText.tokens(segment.text) }

    private var target: EditTarget? {
        if wholeLine { return .lines([segment]) }
        guard let range else { return nil }
        return .words(segment, range)
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(segment.start.formatted(date: .omitted, time: .shortened))
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                    FlowLayout(spacing: 6, lineSpacing: 8) {
                        ForEach(Array(tokens.enumerated()), id: \.offset) { i, token in
                            tokenView(i, token)
                        }
                    }
                    HStack {
                        Button(wholeLine ? "Whole line selected" : "Select whole line", systemImage: "text.justify.left") {
                            wholeLine.toggle()
                            range = nil
                        }
                        .accessibilityIdentifier("edit-whole-line")
                        Spacer()
                        if range != nil || wholeLine {
                            Button("Clear") { range = nil; wholeLine = false }
                        }
                    }
                    .font(.subheadline)
                    .buttonStyle(.borderless)
                    Text("Tap a word, then another to select the words between. Stricken parts can't be selected.")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .background(Theme.background)
            .navigationTitle("Edit line")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
            }
            .safeAreaInset(edge: .bottom) {
                EditActionBar(enabled: target != nil, onDelete: {
                    guard let target else { return }
                    redactions.delete(target, in: meeting, context: context)
                    dismiss()
                }, onStrike: { confirmStrike = true })
            }
            .navigationDestination(isPresented: $confirmStrike) {
                if let target {
                    StrikeConfirmView(target: target, meeting: meeting) { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }

    @ViewBuilder
    private func tokenView(_ i: Int, _ token: RedactionText.Token) -> some View {
        switch token.kind {
        case .marker:
            Label("Stricken", systemImage: "eye.slash")
                .accessibilityLabel("Stricken from the record, can't be selected")
                .font(.caption.weight(.semibold))
                .padding(.horizontal, 8).padding(.vertical, 5)
                .background(Color.black, in: RoundedRectangle(cornerRadius: 6))
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(Theme.hairline))
                .foregroundStyle(.secondary)
        case .word:
            let on = wholeLine || (range?.contains(i) ?? false)
            Text(token.text)
                .font(.body)
                .padding(.horizontal, 6).padding(.vertical, 4)
                .background(on ? Theme.accent : Theme.card, in: RoundedRectangle(cornerRadius: 6))
                .foregroundStyle(on ? Color.black : Color.primary)
                .onTapGesture { tap(i) }
                .accessibilityAddTraits(.isButton)
                .accessibilityAddTraits(on ? .isSelected : [])
                .accessibilityValue("Word \(wordNumber(i)) of \(wordCount)")
                .accessibilityHint(on ? "Selected. Double-tap to change the selection."
                                      : range?.count == 1 ? "Selects the words from the first selected word to this one"
                                                         : "Selects this word")
                .accessibilityIdentifier("word-\(i)")
        }
    }

    private var wordCount: Int { tokens.filter(\.isWord).count }
    private func wordNumber(_ i: Int) -> Int { tokens[...i].filter(\.isWord).count }

    private func tap(_ i: Int) {
        wholeLine = false
        guard let r = range else { range = i...i; return }
        if r == i...i { range = nil; return }
        if r.count == 1 {
            let candidate = min(r.lowerBound, i)...max(r.lowerBound, i)
            range = RedactionText.isWordsOnly(tokens, candidate) ? candidate : i...i
        } else {
            range = i...i
        }
    }
}

/// Delete · Strike from the record… (bottom of the editor / selection modes)
struct EditActionBar: View {
    let enabled: Bool
    var cancel: (() -> Void)? = nil
    let onDelete: () -> Void
    let onStrike: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            // One row; at large text sizes the actions stack so none is cut off
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) {
                    if let cancel { cancelButton(cancel) }
                    Spacer(minLength: 0)
                    deleteButton
                    strikeButton
                }
                VStack(spacing: 8) {
                    strikeButton.frame(maxWidth: .infinity)
                    HStack(spacing: 12) {
                        if let cancel { cancelButton(cancel) }
                        Spacer(minLength: 0)
                        deleteButton
                    }
                }
            }
        }
        .font(.subheadline.weight(.medium))
        .disabled(!enabled)
        .padding(.horizontal, 16).padding(.vertical, 10)
        .background(.bar)
    }

    private func cancelButton(_ cancel: @escaping () -> Void) -> some View {
        Button("Cancel", action: cancel).buttonStyle(.bordered).tint(.secondary).fixedSize()
    }

    private var deleteButton: some View {
        Button("Delete", systemImage: "trash", role: .destructive, action: onDelete)
            .buttonStyle(.bordered)
            .fixedSize()
            .accessibilityHint("Removes the selection. You can undo for a few seconds.")
            .accessibilityIdentifier("edit-delete")
    }

    private var strikeButton: some View {
        Button("Strike from the record…", systemImage: "eye.slash", action: onStrike)
            .buttonStyle(.borderedProminent)
            .tint(Theme.accent)
            .foregroundStyle(.black)
            .fixedSize(horizontal: true, vertical: false)
            .accessibilityHint("Permanently destroys the selection, audio included, and leaves a marker")
            .accessibilityIdentifier("edit-strike")
    }
}

// MARK: - Strike confirmation

/// Names exactly what will be destroyed, takes an optional reason, and says
/// what the app can't reach. No undo.
struct StrikeConfirmView: View {
    let target: EditTarget
    let meeting: Meeting
    let onDone: () -> Void
    @Environment(\.modelContext) private var context
    @Environment(RedactionCenter.self) private var redactions
    @State private var reason = ""
    @State private var working = false
    @State private var error: String?

    var body: some View {
        let summary = StrikeSummary(target, meeting: meeting)
        Form {
            Section {
                ForEach(summary.destroyed, id: \.self) { line in
                    Label {
                        Text(line)
                    } icon: {
                        Image(systemName: "xmark.circle").foregroundStyle(Theme.recording)
                    }
                    .font(.subheadline)
                }
            } header: {
                Text("This permanently destroys")
            } footer: {
                Text("A marker stays in its place: “Stricken from the record”, with the time it covered, today's date and your reason. It never shows what was removed.")
            }
            Section {
                TextField("Reason (optional), e.g. privileged", text: $reason)
                    .accessibilityIdentifier("strike-reason")
            } header: {
                Text("Reason")
            } footer: {
                if reasonLeaks(summary) {
                    Text("The reason can't contain the words being stricken.").foregroundStyle(.orange)
                }
            }
            Section("Outside the app") {
                ForEach(summary.caveats, id: \.self) { line in
                    Text(line).font(.footnote).foregroundStyle(.secondary)
                }
            }
            if let error {
                Section { Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.orange) }
            }
            Section {
                Button(role: .destructive) {
                    Task { await strike() }
                } label: {
                    HStack {
                        Spacer()
                        if working { ProgressView().padding(.trailing, 6) }
                        Text(working ? "Striking…" : "Strike from the record").fontWeight(.semibold)
                        Spacer()
                    }
                }
                .disabled(working || reasonLeaks(summary))
                .accessibilityIdentifier("strike-confirm")
            } footer: {
                Text("This can't be undone.")
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
        .navigationTitle("Strike from the record")
        .navigationBarTitleDisplayMode(.inline)
        .interactiveDismissDisabled(working)
    }

    private func reasonLeaks(_ summary: StrikeSummary) -> Bool {
        let r = reason.lowercased()
        guard !r.isEmpty else { return false }
        return summary.phrases.contains { p in
            let words = p.lowercased().split(whereSeparator: { !$0.isLetter && !$0.isNumber }).filter { $0.count >= 4 }
            return r.contains(p.lowercased()) || words.contains { r.contains($0) }
        }
    }

    private func strike() async {
        working = true
        error = nil
        do {
            let result = try await redactions.strike(target, reason: reason, in: meeting, context: context)
            if !result.storage.compacted {
                redactions.errorMessage = "Stricken. The database couldn't be compacted just now, so freed space may still hold the old text until the next edit."
            }
            working = false
            onDone()
        } catch {
            working = false
            self.error = error.localizedDescription
        }
    }
}

/// The confirmation's "exactly what will be destroyed" list.
struct StrikeSummary {
    var destroyed: [String] = []
    var caveats: [String] = []
    /// Shown text, used only to stop it being typed into the reason
    var phrases: [String] = []

    @MainActor
    init(_ target: EditTarget, meeting: Meeting) {
        guard let plan = try? RedactionEngine.plan(target, in: meeting) else { return }
        phrases = plan.phrases
        let time: (Date) -> String = { $0.formatted(date: .omitted, time: .shortened) }

        switch target {
        case .words(let s, _):
            let quote = plan.phrases.first ?? ""
            destroyed.append("“\(quote)” from the \(time(s.start)) line")
        case .lines:
            let segs = plan.changes.map(\.segment)
            if segs.count == 1, let s = segs.first {
                destroyed.append("The \(time(s.start)) line of the transcript")
            } else if let a = segs.first, let b = segs.last {
                destroyed.append("\(segs.count) lines of the transcript (\(time(a.start))–\(time(b.start)))")
            }
        case .screens(let snaps):
            let times = snaps.map(\.takenAt).sorted().map(time).joined(separator: ", ")
            destroyed.append(snaps.count == 1 ? "The photo taken at \(times), and its file" : "\(snaps.count) photos (\(times)) and their files")
        }

        let ranges = plan.audioRanges
        if !ranges.isEmpty, meeting.audioFileName != nil {
            if ranges.count <= 3 {
                let list = ranges.map { "\($0.lowerBound.clock)–\(max($0.upperBound, $0.lowerBound + 1).clock)" }.joined(separator: ", ")
                destroyed.append("Recording \(list), overwritten with silence")
            } else {
                let total = ranges.reduce(0) { $0 + ($1.upperBound - $1.lowerBound) }
                destroyed.append("\(ranges.count) stretches of the recording (\(total.clock) total), overwritten with silence")
            }
            if plan.changes.contains(where: { $0.segment.audioOffset == nil }) {
                destroyed.append("This recording predates word timings, so the whole line plus a second either side is silenced")
            } else if plan.changes.contains(where: { $0.segment.wordTimings.isEmpty }) {
                destroyed.append("No word timings for this line, so the whole line is silenced")
            }
        }
        if meeting.aiNotes != nil, !plan.changes.isEmpty {
            destroyed.append("Matching text in the notes (they're marked as made before an edit)")
        }
        if !meeting.studyMaterials.isEmpty, !plan.changes.isEmpty {
            destroyed.append("This recording's \(meeting.kind.guideTitle.lowercased()) (make it again after the strike)")
        }
        if !plan.changes.isEmpty {
            destroyed.append("Its text in search and in future exports and AI prompts")
        }
        destroyed.append("Freed space in the app's database (it's compacted after the strike)")

        caveats.append("Copies already shared, exported or sent to an AI endpoint can't be recalled.")
        caveats.append("iCloud and device backups made before now are outside the app's control.")
        if !plan.snapshots.isEmpty {
            caveats.append("Photos imported from your library stay in the Photos app.")
        }
        caveats.append("iOS manages freed flash storage; the app removes the content from its files and database but can't overwrite the flash itself.")
    }
}

// MARK: - Undo toast

struct RedactionToast: View {
    @Environment(RedactionCenter.self) private var redactions

    var body: some View {
        Group {
            if let p = redactions.pending {
                toast {
                    Image(systemName: "trash").accessibilityHidden(true)
                    Text(p.label)
                    Spacer(minLength: 8)
                    Button("Undo") { redactions.undo() }
                        .fontWeight(.semibold)
                        .foregroundStyle(Theme.accent)
                        .accessibilityIdentifier("undo-button")
                }
            } else if let message = redactions.errorMessage {
                toast {
                    Image(systemName: "exclamationmark.triangle").foregroundStyle(.orange)
                    Text(message).lineLimit(3)
                    Spacer(minLength: 8)
                    Button { redactions.errorMessage = nil } label: { Image(systemName: "xmark") }
                        .accessibilityLabel("Dismiss")
                }
            }
        }
        .animation(.easeOut(duration: 0.2), value: redactions.pending?.recordID)
    }

    private func toast<C: View>(@ViewBuilder _ content: () -> C) -> some View {
        HStack(spacing: 10) { content() }
            .font(.subheadline)
            .padding(.horizontal, 16).padding(.vertical, 12)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Theme.hairline))
            .shadow(color: .black.opacity(0.4), radius: 12, y: 4)
            .padding(.horizontal, 16)
            .frame(maxWidth: 520)
            .transition(.move(edge: .bottom).combined(with: .opacity))
            .accessibilityIdentifier("undo-toast")
    }
}

// MARK: - Flow layout

/// Wraps word tokens like text.
struct FlowLayout: Layout {
    var spacing: CGFloat = 6
    var lineSpacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(width: proposal.width ?? .infinity, subviews: subviews)
        let width = rows.map(\.width).max() ?? 0
        let height = rows.reduce(0) { $0 + $1.height } + CGFloat(max(rows.count - 1, 0)) * lineSpacing
        return CGSize(width: min(width, proposal.width ?? width), height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for i in row.items {
                let size = subviews[i].sizeThatFits(.unspecified)
                subviews[i].place(at: CGPoint(x: x, y: y + (row.height - size.height) / 2), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + lineSpacing
        }
    }

    private struct Row { var items: [Int] = []; var width: CGFloat = 0; var height: CGFloat = 0 }

    private func arrange(width: CGFloat, subviews: Subviews) -> [Row] {
        var rows: [Row] = []
        var row = Row()
        for (i, view) in subviews.enumerated() {
            let size = view.sizeThatFits(.unspecified)
            let needed = row.items.isEmpty ? size.width : row.width + spacing + size.width
            if needed > width, !row.items.isEmpty {
                rows.append(row)
                row = Row()
            }
            row.width = row.items.isEmpty ? size.width : row.width + spacing + size.width
            row.height = max(row.height, size.height)
            row.items.append(i)
        }
        if !row.items.isEmpty { rows.append(row) }
        return rows
    }
}
