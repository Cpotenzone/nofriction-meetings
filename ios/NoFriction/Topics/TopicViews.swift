import SwiftData
import SwiftUI

// Topic UI (docs/TOPICS_AND_CHAT.md): the editor in a recording's Notes.
// Topics are a search facet in Recordings and a scope in Chat; the list has
// no topic chips (Notebooks are the one filter).

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
                Text(Topic.filterTitle)
                    .font(.footnote.weight(.semibold))
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
