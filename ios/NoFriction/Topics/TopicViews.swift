import SwiftData
import SwiftUI

// Topic UI (docs/TOPICS_AND_CHAT.md): the Topics chip row in the Recordings
// list, the chips on a row, and the editor in a recording's Notes.

/// Recordings list: **Topics** chips beside the Notebooks row. `selection`
/// is a group key (`TopicIndex.Group.key`).
struct TopicFilterBar: View {
    let groups: [TopicIndex.Group]
    @Binding var selection: String?
    /// Most chips shown; the rest are reachable by grouping by Topic
    static let maxChips = 20

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(Topic.filterTitle.uppercased())
                .font(.caption2.weight(.semibold))
                .tracking(0.8)
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    chip("All", on: selection == nil) { selection = nil }
                    ForEach(groups.prefix(Self.maxChips)) { g in
                        chip(g.count > 1 ? "\(g.label) · \(g.count)" : g.label, on: selection == g.key) {
                            selection = selection == g.key ? nil : g.key
                        }
                        .accessibilityLabel("\(Topic.label): \(g.label), \(g.count) recordings")
                    }
                }
                .padding(.vertical, 2)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Topic.filterTitle)
        .accessibilityIdentifier("topic-filter")
    }

    private func chip(_ title: String, on: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline.weight(on ? .semibold : .regular))
                .lineLimit(1)
                .padding(.horizontal, 12).padding(.vertical, 7)
                .background(on ? Theme.ai.opacity(0.22) : Theme.card, in: Capsule())
                .overlay { Capsule().stroke(on ? Theme.ai : Color.clear, lineWidth: 1) }
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(on ? [.isButton, .isSelected] : .isButton)
    }
}

/// Up to two topic chips on a Recordings row.
struct TopicChipsRow: View {
    let topics: [MeetingTopic]

    var body: some View {
        if !topics.isEmpty {
            HStack(spacing: 6) {
                ForEach(topics.prefix(Topic.chipsPerRow)) { t in
                    TopicChip(label: t.label)
                }
                if topics.count > Topic.chipsPerRow {
                    Text("+\(topics.count - Topic.chipsPerRow)").font(.caption2).foregroundStyle(.tertiary)
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("\(Topic.filterTitle): " + topics.map(\.label).joined(separator: ", "))
            .accessibilityIdentifier("meeting-row-topics")
        }
    }
}

struct TopicChip: View {
    let label: String
    var body: some View {
        Text(label)
            .font(.caption2.weight(.medium))
            .lineLimit(1)
            .padding(.horizontal, 8).padding(.vertical, 3)
            .foregroundStyle(Theme.ai)
            .background(Theme.ai.opacity(0.14), in: Capsule())
    }
}

/// In a recording's Notes: its topics, with rename / remove (context menu
/// or long press), add, and **Find topics**.
struct TopicsEditor: View {
    @Bindable var meeting: Meeting
    /// An AI request is running (any kind); the Find button waits
    let busy: Bool
    /// Why the last Find topics failed, if it did
    let error: String?
    let onFind: () -> Void
    @Environment(\.modelContext) private var context
    @State private var adding = false
    @State private var renaming: MeetingTopic?
    @State private var draft = ""
    @State private var editError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(Topic.filterTitle.uppercased())
                    .font(.caption2.weight(.semibold))
                    .tracking(0.8)
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
                Button(meeting.topics.isEmpty ? "Find topics" : "Find again", systemImage: "sparkles", action: onFind)
                    .font(.caption.weight(.semibold))
                    .tint(Theme.ai)
                    .disabled(busy || meeting.segments.isEmpty)
                    .accessibilityIdentifier("topics-find")
            }
            FlowLayout(spacing: 8) {
                ForEach(meeting.orderedTopics) { t in
                    Menu {
                        Button("Rename…", systemImage: "pencil") { draft = t.label; renaming = t }
                        Button("Remove", systemImage: "minus.circle", role: .destructive) {
                            TopicStore.remove(t, from: meeting, context: context)
                        }
                    } label: {
                        HStack(spacing: 4) {
                            Text(t.label).lineLimit(1)
                            if t.isUser { Image(systemName: "person.fill").font(.system(size: 8)).accessibilityHidden(true) }
                        }
                        .font(.subheadline)
                        .padding(.horizontal, 11).padding(.vertical, 6)
                        .foregroundStyle(Theme.ai)
                        .background(Theme.ai.opacity(0.14), in: Capsule())
                    }
                    .accessibilityLabel("\(Topic.label): \(t.label)" + (t.isUser ? ", yours" : ""))
                    .accessibilityHint("Rename or remove")
                }
                Button { draft = ""; editError = nil; adding = true } label: {
                    Label("Add", systemImage: "plus").font(.subheadline)
                        .padding(.horizontal, 11).padding(.vertical, 6)
                        .background(Theme.card, in: Capsule())
                        .overlay { Capsule().stroke(Theme.hairline, lineWidth: 1) }
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add topic")
                .accessibilityIdentifier("topics-add")
            }
            if meeting.topics.isEmpty && error == nil {
                Text(meeting.segments.isEmpty ? "Topics are named from the transcript." : "Named when notes are made, or tap Find topics.")
                    .font(.caption).foregroundStyle(.tertiary)
            }
            if let error {
                Label(error, systemImage: "exclamationmark.triangle").font(.footnote).foregroundStyle(.orange)
            }
            if let editError {
                Text(editError).font(.footnote).foregroundStyle(.orange)
            }
        }
        .alert("Add topic", isPresented: $adding) {
            TextField("e.g. Q4 roadmap", text: $draft)
            Button("Add") { add() }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("A short noun phrase. Your topics are kept when topics are found again.")
        }
        .alert("Rename topic", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("Topic", text: $draft)
            Button("Save") {
                if let t = renaming { TopicStore.rename(t, to: draft, meeting: meeting, context: context) }
                renaming = nil
            }
            Button("Cancel", role: .cancel) { renaming = nil }
        }
    }

    private func add() {
        do {
            _ = try TopicStore.add(draft, to: meeting, context: context)
            editError = nil
        } catch {
            editError = error.localizedDescription
        }
    }
}
