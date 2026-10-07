import SwiftData
import SwiftUI

// Timed recording and classes (docs/TIMED_RECORDING_AND_CLASSES.md):
// the "How long?" sheet, the time-left row and warning on the Record
// screen, class chips, the library filter and the class field.

/// "How long?" before recording, with an optional class.
struct RecordPlanSheet: View {
    let onStart: (RecordingLimit, String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var choice: RecordingLimit = RecordingLimitStore.remembered()
    @State private var className = ""
    @Query(filter: #Predicate<Meeting> { $0.courseName != nil }, sort: \Meeting.startedAt, order: .reverse)
    private var classMeetings: [Meeting]

    private var recents: [String] { ClassNames.recent(classMeetings.map { ($0.courseName, $0.startedAt) }) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("How long?")
                            .font(.title2.weight(.semibold))
                            .accessibilityAddTraits(.isHeader)
                        Text("The recording stops by itself at the end. You can add time while it runs.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 64), spacing: 10)], spacing: 10) {
                        ForEach(RecordingLimit.choices, id: \.self) { c in
                            LengthChoice(limit: c, selected: choice == c) { choice = c }
                        }
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        (Text("Class ").font(.subheadline.weight(.semibold)) + Text("optional").font(.subheadline).foregroundStyle(.secondary))
                        TextField("e.g. BIO 101 — Cell Biology", text: $className)
                            .textInputAutocapitalization(.words)
                            .autocorrectionDisabled()
                            .submitLabel(.go)
                            .onSubmit(start)
                            .padding(12)
                            .background(Theme.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                            .accessibilityIdentifier("record-class-field")
                        ClassChips(classes: ClassNames.suggestions(className, recents: recents), selected: ClassNames.normalize(className)) {
                            className = $0
                        }
                        if ClassNames.normalize(className) != nil {
                            Text("Notes for a class are written as lecture notes.")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .padding(24)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .safeAreaInset(edge: .bottom) {
                Button(action: start) {
                    Text("Start recording").frame(maxWidth: .infinity).padding(.vertical, 6)
                }
                .buttonStyle(.borderedProminent)
                .tint(Theme.recordingStrong)
                .padding(20)
                .background(.bar)
                .accessibilityIdentifier("record-plan-start")
            }
            .background(Theme.background)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private func start() {
        RecordingLimitStore.remember(choice)
        let name = ClassNames.canonical(className, existing: recents)
        dismiss()
        onStart(choice, name)
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

/// One-tap class chips.
struct ClassChips: View {
    let classes: [String]
    let selected: String?
    let pick: (String) -> Void

    var body: some View {
        if !classes.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(classes, id: \.self) { c in
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
            .accessibilityLabel("Recent classes")
        }
    }
}

/// Library filter: All, then each class.
struct ClassFilterBar: View {
    let classes: [String]
    @Binding var selection: String?

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                chip("All", on: selection == nil) { selection = nil }
                ForEach(classes, id: \.self) { c in
                    chip(c, on: ClassNames.matches(c, filter: selection) && selection != nil) {
                        selection = selection == c ? nil : c
                    }
                }
            }
            .padding(.vertical, 2)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Filter by class")
        .accessibilityIdentifier("class-filter")
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

/// "Class" on a meeting: edit, pick a recent one, or clear.
struct MeetingClassField: View {
    @Bindable var meeting: Meeting
    @Environment(\.modelContext) private var context
    @State private var draft = ""
    @FocusState private var focused: Bool
    @Query(filter: #Predicate<Meeting> { $0.courseName != nil }, sort: \Meeting.startedAt, order: .reverse)
    private var classMeetings: [Meeting]

    private var recents: [String] { ClassNames.recent(classMeetings.map { ($0.courseName, $0.startedAt) }) }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "graduationcap")
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            TextField("Class", text: $draft, prompt: Text("Add to a class"))
                .font(.subheadline)
                .textInputAutocapitalization(.words)
                .autocorrectionDisabled()
                .submitLabel(.done)
                .focused($focused)
                .onSubmit(commit)
                .accessibilityLabel("Class")
                .accessibilityIdentifier("meeting-class-field")
            let others = recents.filter { $0.caseInsensitiveCompare(meeting.courseName ?? "") != .orderedSame }
            if !others.isEmpty || meeting.courseName != nil {
                Menu {
                    ForEach(others, id: \.self) { c in
                        Button(c) { draft = c; commit() }
                    }
                    if meeting.courseName != nil {
                        Button("Not a class", systemImage: "xmark", role: .destructive) { draft = ""; commit() }
                    }
                } label: {
                    Image(systemName: "chevron.up.chevron.down")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .accessibilityLabel("Choose a class")
            }
        }
        .onAppear { draft = meeting.courseName ?? "" }
        .onChange(of: meeting.id) { _, _ in draft = meeting.courseName ?? "" }
        .onChange(of: focused) { _, isFocused in if !isFocused { commit() } }
    }

    private func commit() {
        let name = ClassNames.canonical(draft, existing: recents)
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
            Label("Stops at \(deadline.formatted(date: .omitted, time: .shortened))", systemImage: "timer")
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

/// One-time, non-blocking: the first class recording.
struct ClassNoticeBanner: View {
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "graduationcap.fill").foregroundStyle(Theme.accent)
                .accessibilityHidden(true)
            Text(ClassNames.notice).font(.footnote)
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
