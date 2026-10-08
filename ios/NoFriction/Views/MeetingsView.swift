import SwiftData
import SwiftUI

/// Past recordings (the Recordings tab). iPhone: a list that pushes detail. iPad: list + detail.
struct MeetingsView: View {
    @Query(sort: \Meeting.startedAt, order: .reverse) private var meetings: [Meeting]
    @Environment(RecordingSession.self) private var session
    @State private var selection: Meeting?
    @State private var query = ""
    /// Notebook filter; nil = All
    @State private var notebookFilter: String?
    /// Topic filter (a `TopicIndex.Group.key`); nil = All (docs/TOPICS_AND_CHAT.md)
    @State private var topicFilter: String?
    /// Group by: Date · Notebook · Topic (remembered)
    @AppStorage("recordingsGroupBy") private var groupByRaw = RecordingsGroupBy.date.rawValue

    private var groupBy: RecordingsGroupBy {
        get { RecordingsGroupBy(rawValue: groupByRaw) ?? .date }
        nonmutating set { groupByRaw = newValue.rawValue }
    }

    /// Topics across every saved recording, near-duplicates merged
    private var topicIndex: TopicIndex { TopicIndex(entries: Meeting.topicEntries(meetings)) }

    /// The topic filter, unless its last recording was deleted
    private var activeTopic: TopicIndex.Group? { topicFilter.flatMap { topicIndex.group(forKey: $0) } }

    /// Notebooks of saved recordings, most recent first
    private var notebooks: [String] { Notebook.recent(meetings.map { ($0.courseName, $0.startedAt) }, limit: 50) }

    /// The filter, unless its last recording was deleted
    private var activeNotebook: String? {
        guard let notebookFilter, notebooks.contains(where: { $0.caseInsensitiveCompare(notebookFilter) == .orderedSame }) else { return nil }
        return notebookFilter
    }

    private var shown: [Meeting] {
        let topicIDs = activeTopic.map { Set($0.meetingIDs) }
        let past = meetings.filter {
            $0.id != session.meeting?.id && Notebook.matches($0.courseName, filter: activeNotebook)
                && (topicIDs?.contains($0.id) ?? true)
        }
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return past }
        return past.filter { m in
            m.title.lowercased().contains(q)
                || (m.courseName?.lowercased().contains(q) ?? false)
                || m.people.contains { $0.person.displayName.lowercased().contains(q) || ($0.person.company?.lowercased().contains(q) ?? false) }
                || m.topics.contains { $0.label.lowercased().contains(q) }
                || m.transcriptText.lowercased().contains(q)
        }
    }

    /// Sections by the Group by choice (`RecordingsGrouping`)
    private var sections: [RecordingsGrouping.Section] {
        RecordingsGrouping.sections(shown, by: groupBy, topics: topicIndex)
    }

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                CalendarConnectCard()
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                if !notebooks.isEmpty {
                    NotebookFilterBar(notebooks: notebooks, selection: $notebookFilter)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                }
                if !topicIndex.groups.isEmpty {
                    TopicFilterBar(groups: topicIndex.groups, selection: $topicFilter)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                }
                ForEach(sections) { section in
                    Section(section.title) {
                        ForEach(section.meetings) { meeting in
                            NavigationLink(value: meeting) { MeetingRow(meeting: meeting) }
                        }
                    }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Theme.background)
            // Neutral selection on iPad; yellow fill made secondary text unreadable
            .tint(Color.white.opacity(0.14))
            .navigationTitle("Recordings")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Menu {
                        Picker(Topic.groupByTitle, selection: Binding(get: { groupBy }, set: { groupBy = $0 })) {
                            ForEach(RecordingsGroupBy.allCases) { g in
                                Label(g.label, systemImage: g.systemImage).tag(g)
                            }
                        }
                    } label: {
                        Label("\(Topic.groupByTitle): \(groupBy.label)", systemImage: "line.3.horizontal.decrease.circle")
                    }
                    .accessibilityIdentifier("recordings-group-by")
                }
            }
            .searchable(text: $query, prompt: "Titles, people, topics, or anything said")
            .overlay {
                if meetings.isEmpty {
                    ContentUnavailableView("No recordings yet", systemImage: "waveform",
                                           description: Text("Meetings, classes and everything else you record appear here with their transcript and photos."))
                } else if shown.isEmpty && !query.isEmpty {
                    ContentUnavailableView.search(text: query)
                } else if shown.isEmpty, let activeNotebook {
                    ContentUnavailableView("No recordings in \(activeNotebook)", systemImage: "book.closed")
                } else if shown.isEmpty, let activeTopic {
                    ContentUnavailableView("No recordings about \(activeTopic.label)", systemImage: "tag")
                }
            }
        } detail: {
            if let selection {
                MeetingDetailView(meeting: selection)
            } else {
                ContentUnavailableView("Select a recording", systemImage: "rectangle.stack")
                    .background(Theme.background)
            }
        }
    }
}

private struct MeetingRow: View {
    let meeting: Meeting

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(meeting.title).font(.body.weight(.medium)).lineLimit(1)
                if meeting.isFromWatch { WatchBadge() }
            }
            HStack(spacing: 8) {
                Text([meeting.startedAt.formatted(date: .omitted, time: .shortened), meeting.duration?.minutesLabel]
                    .compactMap { $0 }.joined(separator: " · "))
                if !meeting.snapshots.isEmpty {
                    Label("\(meeting.snapshots.count)", systemImage: "photo").labelStyle(.titleAndIcon)
                }
                if let phase = meeting.importPhase {
                    Text(phase == .failed ? "Not transcribed" : "Transcribing…")
                        .foregroundStyle(phase == .failed ? .orange : Theme.accent)
                        .accessibilityIdentifier("meeting-row-import-state")
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            // Class / Personal (a meeting is the default and isn't tagged), and the notebook
            if meeting.kind != .meeting || meeting.courseName != nil {
                HStack(spacing: 10) {
                    if meeting.kind != .meeting {
                        Label(meeting.kind.label, systemImage: meeting.kind.systemImage)
                            .foregroundStyle(.secondary)
                    }
                    if let notebook = meeting.courseName {
                        Label(notebook, systemImage: "book.closed")
                            .foregroundStyle(Theme.accent)
                            .accessibilityLabel("\(Notebook.label): \(notebook)")
                    }
                }
                .font(.caption)
                .lineLimit(1)
            }
            TopicChipsRow(topics: meeting.orderedTopics)
            let names = meeting.people.prefix(3).map(\.person.displayName)
            if !names.isEmpty {
                Text(names.joined(separator: ", ") + (meeting.people.count > 3 ? " +\(meeting.people.count - 3)" : ""))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 2)
    }
}

/// Small Apple Watch mark for meetings recorded on the watch.
struct WatchBadge: View {
    var body: some View {
        Image(systemName: "applewatch")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
            .accessibilityLabel("Recorded on Apple Watch")
            .accessibilityIdentifier("watch-badge")
    }
}

/// "Group by" in the Recordings list: Date · Notebook · Topic (same words as the Mac).
enum RecordingsGroupBy: String, CaseIterable, Identifiable {
    case date, notebook, topic
    var id: String { rawValue }
    var label: String {
        switch self {
        case .date: "Date"
        case .notebook: Notebook.label
        case .topic: Topic.label
        }
    }
    var systemImage: String {
        switch self {
        case .date: "calendar"
        case .notebook: "book.closed"
        case .topic: "tag"
        }
    }
}

/// Sections of the Recordings list (pure; unit-tested through `sections(_:by:topics:)`).
enum RecordingsGrouping {
    struct Section: Identifiable {
        var title: String
        var meetings: [Meeting]
        var id: String { title }
    }

    static let noNotebook = "No notebook"
    static let noTopics = "No topics"

    /// Date: by day, newest first. Notebook: by name (most recent first),
    /// then "No notebook". Topic: a recording appears under each of its
    /// topics (biggest topic first), then "No topics".
    @MainActor
    static func sections(_ meetings: [Meeting], by grouping: RecordingsGroupBy, topics: TopicIndex) -> [Section] {
        switch grouping {
        case .date:
            let grouped = Dictionary(grouping: meetings) { Calendar.current.startOfDay(for: $0.startedAt) }
            return grouped.keys.sorted(by: >).map {
                Section(title: $0.formatted(.dateTime.weekday(.wide).month(.wide).day()), meetings: grouped[$0]!)
            }
        case .notebook:
            let names = Notebook.recent(meetings.map { ($0.courseName, $0.startedAt) }, limit: 500)
            var out = names.map { n in Section(title: n, meetings: meetings.filter { Notebook.matches($0.courseName, filter: n) }) }
            let none = meetings.filter { Notebook.normalize($0.courseName) == nil }
            if !none.isEmpty { out.append(Section(title: noNotebook, meetings: none)) }
            return out
        case .topic:
            let ids = Set(meetings.map(\.id))
            var out: [Section] = []
            var placed = Set<UUID>()
            for g in topics.groups {
                let members = meetings.filter { g.meetingIDs.contains($0.id) && ids.contains($0.id) }
                guard !members.isEmpty else { continue }
                out.append(Section(title: g.label, meetings: members))
                members.forEach { placed.insert($0.id) }
            }
            let none = meetings.filter { !placed.contains($0.id) }
            if !none.isEmpty { out.append(Section(title: noTopics, meetings: none)) }
            return out
        }
    }
}
