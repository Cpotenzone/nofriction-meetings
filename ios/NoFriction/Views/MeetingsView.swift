import SwiftData
import SwiftUI

/// Past recordings (the Recordings tab). iPhone: a list that pushes detail. iPad: list + detail.
/// Notebook chips are the one filter; People is a section under the list
/// (topics stay a search facet and live on the recording).
struct MeetingsView: View {
    @Query(sort: \Meeting.startedAt, order: .reverse) private var meetings: [Meeting]
    @Query(filter: #Predicate<Person> { !$0.isSelf }) private var people: [Person]
    @Environment(RecordingSession.self) private var session
    @State private var selection: Meeting?
    @State private var query = ""
    /// Notebook filter; nil = All
    @State private var notebookFilter: String?

    /// Notebooks of saved recordings, most recent first
    private var notebooks: [String] { Notebook.recent(meetings.map { ($0.courseName, $0.startedAt) }, limit: 50) }

    /// The filter, unless its last recording was deleted
    private var activeNotebook: String? {
        guard let notebookFilter, notebooks.contains(where: { $0.caseInsensitiveCompare(notebookFilter) == .orderedSame }) else { return nil }
        return notebookFilter
    }

    private var shown: [Meeting] {
        let past = meetings.filter {
            $0.id != session.meeting?.id && Notebook.matches($0.courseName, filter: activeNotebook)
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

    /// Sections by day, newest first (`RecordingsGrouping`)
    private var sections: [RecordingsGrouping.Section] { RecordingsGrouping.byDate(shown) }

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
                if !people.isEmpty && query.isEmpty {
                    Section {
                        NavigationLink {
                            PeopleListView()
                        } label: {
                            Label {
                                Text("People")
                            } icon: {
                                Image(systemName: "person.2").foregroundStyle(Theme.accent)
                            }
                            .badge(people.count)
                        }
                        .accessibilityLabel("People, \(people.count)")
                        .accessibilityHint("Everyone from your calendar invites, with their recordings")
                        .accessibilityIdentifier("recordings-people")
                    }
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
            .searchable(text: $query, prompt: "Titles, people, topics, or anything said")
            .overlay {
                if meetings.isEmpty {
                    ContentUnavailableView("No recordings yet", systemImage: "waveform",
                                           description: Text("Meetings, classes and everything else you record appear here with their transcript and photos."))
                } else if shown.isEmpty && !query.isEmpty {
                    ContentUnavailableView.search(text: query)
                } else if shown.isEmpty, let activeNotebook {
                    ContentUnavailableView("No recordings in \(activeNotebook)", systemImage: "book.closed")
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

/// Sections of the Recordings list (pure; unit-tested through `byDate(_:)`).
enum RecordingsGrouping {
    struct Section: Identifiable {
        var title: String
        var meetings: [Meeting]
        var id: String { title }
    }

    /// By day, newest first; the order inside a day is the order given.
    @MainActor
    static func byDate(_ meetings: [Meeting]) -> [Section] {
        let grouped = Dictionary(grouping: meetings) { Calendar.current.startOfDay(for: $0.startedAt) }
        return grouped.keys.sorted(by: >).map {
            Section(title: $0.formatted(.dateTime.weekday(.wide).month(.wide).day()), meetings: grouped[$0]!)
        }
    }
}
