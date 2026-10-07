import SwiftData
import SwiftUI

/// Past meetings. iPhone: a list that pushes detail. iPad: list + detail.
struct MeetingsView: View {
    @Query(sort: \Meeting.startedAt, order: .reverse) private var meetings: [Meeting]
    @Environment(RecordingSession.self) private var session
    @State private var selection: Meeting?
    @State private var query = ""
    /// Class filter; nil = All
    @State private var classFilter: String?

    /// Classes of saved meetings, most recent first
    private var classes: [String] { ClassNames.recent(meetings.map { ($0.courseName, $0.startedAt) }, limit: 50) }

    /// The filter, unless its last meeting was deleted
    private var activeClass: String? {
        guard let classFilter, classes.contains(where: { $0.caseInsensitiveCompare(classFilter) == .orderedSame }) else { return nil }
        return classFilter
    }

    private var shown: [Meeting] {
        let past = meetings.filter { $0.id != session.meeting?.id && ClassNames.matches($0.courseName, filter: activeClass) }
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return past }
        return past.filter { m in
            m.title.lowercased().contains(q)
                || (m.courseName?.lowercased().contains(q) ?? false)
                || m.people.contains { $0.person.displayName.lowercased().contains(q) || ($0.person.company?.lowercased().contains(q) ?? false) }
                || m.transcriptText.lowercased().contains(q)
        }
    }

    /// Grouped by day, newest first
    private var sections: [(day: Date, meetings: [Meeting])] {
        let grouped = Dictionary(grouping: shown) { Calendar.current.startOfDay(for: $0.startedAt) }
        return grouped.keys.sorted(by: >).map { ($0, grouped[$0]!) }
    }

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                CalendarConnectCard()
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                if !classes.isEmpty {
                    ClassFilterBar(classes: classes, selection: $classFilter)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                }
                ForEach(sections, id: \.day) { section in
                    Section(section.day.formatted(.dateTime.weekday(.wide).month(.wide).day())) {
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
            .navigationTitle("Meetings")
            .searchable(text: $query, prompt: "Titles, people, or anything said")
            .overlay {
                if meetings.isEmpty {
                    ContentUnavailableView("No meetings yet", systemImage: "waveform",
                                           description: Text("Recordings you make appear here with their transcript, photos and attendees."))
                } else if shown.isEmpty && !query.isEmpty {
                    ContentUnavailableView.search(text: query)
                } else if shown.isEmpty, let activeClass {
                    ContentUnavailableView("No recordings in \(activeClass)", systemImage: "graduationcap")
                }
            }
        } detail: {
            if let selection {
                MeetingDetailView(meeting: selection)
            } else {
                ContentUnavailableView("Select a meeting", systemImage: "rectangle.stack")
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
            if let className = meeting.courseName {
                Label(className, systemImage: "graduationcap")
                    .font(.caption)
                    .foregroundStyle(Theme.accent)
                    .lineLimit(1)
                    .accessibilityLabel("Class: \(className)")
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
