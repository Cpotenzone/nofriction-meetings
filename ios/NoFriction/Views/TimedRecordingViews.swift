import SwiftData
import SwiftUI

// Timed recording, type and notebook (docs/TIMED_RECORDING_AND_NOTEBOOKS.md):
// the Record sheet ("What is it?", "How long?", Notebook), the time-left row
// and warning on the Record screen, notebook chips, the library filter and
// the type and notebook fields on a recording.

/// Before recording: what it is, how long, and an optional notebook.
struct RecordPlanSheet: View {
    let onStart: (RecordingLimit, RecordingKind, String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var kind: RecordingKind = RecordingKindStore.remembered()
    @State private var choice: RecordingLimit = RecordingLimitStore.remembered()
    @State private var notebook = ""
    @Query(filter: #Predicate<Meeting> { $0.courseName != nil }, sort: \Meeting.startedAt, order: .reverse)
    private var notebookMeetings: [Meeting]

    private var recents: [String] { Notebook.recent(notebookMeetings.map { ($0.courseName, $0.startedAt) }) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(RecordingKind.pickerTitle)
                            .font(.title2.weight(.semibold))
                            .accessibilityAddTraits(.isHeader)
                        Picker(RecordingKind.pickerTitle, selection: $kind) {
                            ForEach(RecordingKind.allCases) { k in
                                Text(k.label).tag(k)
                            }
                        }
                        .pickerStyle(.segmented)
                        .accessibilityIdentifier("record-kind")
                    }
                    Text("How long?")
                        .font(.headline)
                        .accessibilityAddTraits(.isHeader)
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 64), spacing: 10)], spacing: 10) {
                        ForEach(RecordingLimit.choices, id: \.self) { c in
                            LengthChoice(limit: c, selected: choice == c) { choice = c }
                        }
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        (Text("\(Notebook.label) ").font(.subheadline.weight(.semibold)) + Text("optional").font(.subheadline).foregroundStyle(.secondary))
                        TextField(kind.notebookPlaceholder, text: $notebook)
                            .textInputAutocapitalization(.words)
                            .autocorrectionDisabled()
                            .submitLabel(.go)
                            .onSubmit(start)
                            .padding(12)
                            .background(Theme.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                            .accessibilityLabel(Notebook.label)
                            .accessibilityIdentifier("record-notebook-field")
                        NotebookChips(notebooks: Notebook.suggestions(notebook, recents: recents), selected: Notebook.normalize(notebook)) {
                            notebook = $0
                        }
                    }
                }
                .padding(24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .safeAreaInset(edge: .bottom) {
                Button(action: start) {
                    Text("Record").frame(maxWidth: .infinity).padding(.vertical, 6)
                }
                .buttonStyle(.borderedProminent)
                .tint(Theme.recordingStrong)
                .padding(20)
                .background(.bar)
                .accessibilityLabel("Record")
                .accessibilityHint("Starts recording with these choices")
                .accessibilityIdentifier("record-plan-start")
            }
            .background(Theme.background)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        // Full height: the red button is never below the fold on first open
        .presentationDetents([.large])
    }

    private func start() {
        RecordingLimitStore.remember(choice)
        RecordingKindStore.remember(kind)
        let name = Notebook.canonical(notebook, existing: recents)
        dismiss()
        onStart(choice, kind, name)
    }
}

private struct LengthChoice: View {
    let limit: RecordingLimit
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 2) {
                Text(limit.shortLabel)
                    .font(.title2.weight(.semibold))
                    .monospacedDigit()
                Text(limit.minutes == nil ? "No limit" : "min")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            .frame(maxWidth: .infinity, minHeight: 64)
            .background(selected ? Theme.accent.opacity(0.16) : Theme.card,
                        in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .stroke(selected ? Theme.accent : Color.clear, lineWidth: 2)
            }
            .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(limit.spoken)
        .accessibilityAddTraits(selected ? [.isButton, .isSelected] : .isButton)
        .accessibilityIdentifier("record-length-\(limit.storageValue)")
    }
}

/// One-tap notebook chips.
struct NotebookChips: View {
    let notebooks: [String]
    let selected: String?
    let pick: (String) -> Void

    var body: some View {
        if !notebooks.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(notebooks, id: \.self) { c in
                        let on = selected?.caseInsensitiveCompare(c) == .orderedSame
                        Button { pick(c) } label: {
                            Text(c)
                                .font(.subheadline)
                                .lineLimit(1)
                                .padding(.horizontal, 12).padding(.vertical, 7)
                                .background(on ? Theme.accent.opacity(0.18) : Theme.card, in: Capsule())
                                .overlay { Capsule().stroke(on ? Theme.accent : Color.clear, lineWidth: 1) }
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
                    }
                }
            }
            .accessibilityLabel("Recent notebooks")
        }
    }
}

/// Library filter ("Notebooks"): All, then each notebook.
struct NotebookFilterBar: View {
    let notebooks: [String]
    @Binding var selection: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(Notebook.filterTitle.uppercased())
                .font(.caption2.weight(.semibold))
                .tracking(0.8)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    chip("All", on: selection == nil) { selection = nil }
                    ForEach(notebooks, id: \.self) { c in
                        chip(c, on: Notebook.matches(c, filter: selection) && selection != nil) {
                            selection = selection == c ? nil : c
                        }
                    }
                }
                .padding(.vertical, 2)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Notebook.filterTitle)
        .accessibilityIdentifier("notebook-filter")
    }

    private func chip(_ title: String, on: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline.weight(on ? .semibold : .regular))
                .lineLimit(1)
                .padding(.horizontal, 12).padding(.vertical, 7)
                .background(on ? Theme.accent.opacity(0.18) : Theme.card, in: Capsule())
                .overlay { Capsule().stroke(on ? Theme.accent : Color.clear, lineWidth: 1) }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
    }
}

/// On a recording: its type (Meeting / Class / Personal) and its notebook
/// (edit, pick a recent one, or clear).
struct RecordingKindNotebookField: View {
    @Bindable var meeting: Meeting
    @Environment(\.modelContext) private var context
    @State private var draft = ""
    @FocusState private var focused: Bool
    @Query(filter: #Predicate<Meeting> { $0.courseName != nil }, sort: \Meeting.startedAt, order: .reverse)
    private var notebookMeetings: [Meeting]

    private var recents: [String] { Notebook.recent(notebookMeetings.map { ($0.courseName, $0.startedAt) }) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Menu {
                Picker(RecordingKind.pickerTitle, selection: Binding(get: { meeting.kind }, set: { setKind($0) })) {
                    ForEach(RecordingKind.allCases) { k in
                        Label(k.label, systemImage: k.systemImage).tag(k)
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: meeting.kind.systemImage)
                    Text(meeting.kind.label)
                    Image(systemName: "chevron.up.chevron.down").font(.caption2)
                }
                .font(.subheadline)
                .foregroundStyle(.secondary)
            }
            .accessibilityLabel("\(RecordingKind.pickerTitle) \(meeting.kind.label)")
            .accessibilityHint("Changes the notes style and the third marker's name")
            .accessibilityIdentifier("meeting-kind")
            HStack(spacing: 8) {
                Image(systemName: "book.closed")
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
                TextField(Notebook.label, text: $draft, prompt: Text("Add to a notebook"))
                    .font(.subheadline)
                    .textInputAutocapitalization(.words)
                    .autocorrectionDisabled()
                    .submitLabel(.done)
                    .focused($focused)
                    .onSubmit(commit)
                    .accessibilityLabel(Notebook.label)
                    .accessibilityIdentifier("meeting-notebook-field")
                let others = recents.filter { $0.caseInsensitiveCompare(meeting.courseName ?? "") != .orderedSame }
                if !others.isEmpty || meeting.courseName != nil {
                    Menu {
                        ForEach(others, id: \.self) { c in
                            Button(c) { draft = c; commit() }
                        }
                        if meeting.courseName != nil {
                            Button("Remove from notebook", systemImage: "xmark", role: .destructive) { draft = ""; commit() }
                        }
                    } label: {
                        Image(systemName: "chevron.up.chevron.down")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityLabel("Choose a notebook")
                }
            }
        }
        .onAppear { draft = meeting.courseName ?? "" }
        .onChange(of: meeting.id) { _, _ in draft = meeting.courseName ?? "" }
        .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
    }

    private func setKind(_ kind: RecordingKind) {
        guard kind != meeting.kind || meeting.recordingKind == nil else { return }
        meeting.kind = kind
        try? context.save()
    }

    private func commit() {
        let name = Notebook.canonical(draft, existing: recents)
        draft = name ?? ""
        guard name != meeting.courseName else { return }
        meeting.courseName = name
        try? context.save()
    }
}

/// Record screen: when it stops, with +15 min / No limit.
struct TimeLimitRow: View {
    let deadline: Date
    let extend: () -> Void
    let removeLimit: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Label("Stops at \(shownDeadline.formatted(date: .omitted, time: .shortened))", systemImage: "timer")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .minimumScaleFactor(0.85)
            Spacer(minLength: 0)
            Button("+15 min", action: extend)
                .accessibilityHint("Adds 15 minutes before the recording stops")
                .accessibilityIdentifier("time-limit-extend")
            Button("No limit", action: removeLimit)
                .accessibilityHint("Keeps recording until you stop it")
                .accessibilityIdentifier("time-limit-remove")
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .font(.footnote.weight(.medium))
    }

    private var shownDeadline: Date {
        #if DEBUG
        // Film footage: on the 9:41 status-bar clock (FilmDemo); zero shift otherwise
        return deadline.addingTimeInterval(FilmDemo.displayShift)
        #else
        return deadline
        #endif
    }
}

/// "5 minutes left" on the Record screen (the notification carries the same choices).
struct TimeLimitBanner: View {
    let deadline: Date
    let extend: () -> Void
    let removeLimit: () -> Void
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: "timer").foregroundStyle(Theme.accent)
                    .accessibilityHidden(true)
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    let left = max(0, deadline.timeIntervalSince(ctx.date))
                    Text("\(left.clock) left — stops at \(deadline.formatted(date: .omitted, time: .shortened))")
                        .font(.footnote.weight(.semibold))
                        .monospacedDigit()
                }
                Spacer(minLength: 0)
                Button { dismiss() } label: { Image(systemName: "xmark").font(.footnote) }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Dismiss")
            }
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 10) { buttons }
                VStack(alignment: .leading, spacing: 8) { buttons }
            }
            .font(.footnote.weight(.medium))
        }
        .padding(12)
        .background(Theme.accent.opacity(0.1), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("time-limit-banner")
    }

    @ViewBuilder private var buttons: some View {
        Button("+15 min", action: extend)
            .buttonStyle(.borderedProminent)
            .foregroundStyle(.black)
            .fixedSize()
        Button("No limit", action: removeLimit)
            .buttonStyle(.bordered)
            .fixedSize()
    }
}

/// One-time, non-blocking: the first Class recording.
struct ClassNoticeBanner: View {
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "graduationcap.fill").foregroundStyle(Theme.accent)
                .accessibilityHidden(true)
            Text(ClassNotice.text).font(.footnote)
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
        .accessibilityIdentifier("class-notice")
    }
}
