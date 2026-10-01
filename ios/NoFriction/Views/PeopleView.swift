import SwiftData
import SwiftUI

/// Everyone from your meeting invites, most recent first.
struct PeopleView: View {
    @Query(filter: #Predicate<Person> { !$0.isSelf }) private var people: [Person]
    @State private var query = ""
    @State private var onlyUnlinked = false

    private var shown: [Person] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        return people
            .filter { !onlyUnlinked || $0.linkedinURL == nil }
            .filter { q.isEmpty || $0.displayName.lowercased().contains(q) || $0.email.contains(q) || ($0.company?.lowercased().contains(q) ?? false) }
            .sorted { ($0.meetings.first?.startedAt ?? .distantPast) > ($1.meetings.first?.startedAt ?? .distantPast) }
    }

    var body: some View {
        NavigationStack {
            List {
                CalendarConnectCard()
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))
                if !people.isEmpty {
                    Toggle("No LinkedIn yet", isOn: $onlyUnlinked)
                        .font(.subheadline)
                }
                ForEach(shown) { person in
                    NavigationLink {
                        PersonDetailView(person: person)
                    } label: {
                        PersonRow(person: person, role: nil, showsMeetingCount: true)
                    }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Theme.background)
            .navigationTitle("People")
            .searchable(text: $query, prompt: "Name, company or email")
            .overlay {
                if people.isEmpty {
                    ContentUnavailableView("No people yet", systemImage: "person.2",
                                           description: Text("Connect your calendar and everyone on your meeting invites appears here."))
                }
            }
        }
    }
}

struct PersonDetailView: View {
    @Bindable var person: Person

    var body: some View {
        List {
            Section {
                PersonRow(person: person, role: nil)
            }
            Section("Meetings") {
                ForEach(person.meetings) { meeting in
                    NavigationLink {
                        MeetingDetailView(meeting: meeting)
                    } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(meeting.title)
                            Text(meeting.startedAt.formatted(date: .abbreviated, time: .shortened))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(Theme.background)
        .navigationTitle(person.displayName)
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Avatar, name, company — and the LinkedIn link, one tap to open or add.
struct PersonRow: View {
    @Bindable var person: Person
    let role: String?
    var showsMeetingCount = false
    @Environment(\.openURL) private var openURL
    @Environment(\.modelContext) private var context
    @State private var editing = false

    var body: some View {
        HStack(spacing: 12) {
            Text(person.initials)
                .font(.caption.weight(.semibold))
                .frame(width: 36, height: 36)
                .background(Theme.card, in: Circle())
                .overlay(Circle().stroke(Theme.hairline))
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(person.displayName).font(.body.weight(.medium)).lineLimit(1)
                    if role == "organizer" {
                        Text("ORGANIZER").font(.caption2.weight(.semibold)).foregroundStyle(Theme.accent)
                    }
                }
                Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            linkedIn
        }
        .padding(.vertical, 6)
        .sheet(isPresented: $editing) {
            LinkedInSheet(person: person)
                .presentationDetents([.medium])
        }
    }

    private var subtitle: String {
        var parts = [person.company, person.email].compactMap { $0 }
        if showsMeetingCount {
            let n = person.attendances.count
            parts.append("\(n) meeting\(n == 1 ? "" : "s")")
        }
        return parts.joined(separator: " · ")
    }

    @ViewBuilder private var linkedIn: some View {
        if let link = person.linkedinURL, let url = URL(string: link) {
            Button { openURL(url) } label: {
                Text("in")
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 30, height: 30)
                    .background(Theme.linkedIn, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            }
            .buttonStyle(.borderless)
            .contextMenu {
                Button("Change LinkedIn Link", systemImage: "pencil") { editing = true }
                Button("Remove LinkedIn Link", systemImage: "trash", role: .destructive) {
                    person.linkedinURL = nil
                    try? context.save()
                }
            }
            .accessibilityLabel("Open \(person.displayName)'s LinkedIn")
        } else {
            Button { editing = true } label: {
                Label("LinkedIn", systemImage: "plus")
                    .font(.caption.weight(.medium))
                    .padding(.horizontal, 10).padding(.vertical, 6)
                    .background(Theme.card, in: Capsule())
                    .overlay(Capsule().stroke(Theme.hairline))
            }
            .buttonStyle(.borderless)
            .accessibilityLabel("Add \(person.displayName)'s LinkedIn")
        }
    }
}

/// Find on LinkedIn → copy the profile link → paste here.
private struct LinkedInSheet: View {
    @Bindable var person: Person
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @Environment(\.modelContext) private var context
    @State private var text = ""
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Button {
                        openURL(LinkedIn.searchURL(name: person.name, email: person.email, company: person.company))
                    } label: {
                        Label("Search LinkedIn for \(person.displayName)", systemImage: "magnifyingglass")
                    }
                } footer: {
                    Text("Find their profile, tap Share → Copy Link, then come back and paste it below.")
                }
                Section {
                    TextField("linkedin.com/in/…", text: $text)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .onSubmit(save)
                    PasteButton(payloadType: String.self) { strings in
                        if let s = strings.first { text = s; save() }
                    }
                } footer: {
                    if let error { Text(error).foregroundStyle(Theme.accent) }
                }
            }
            .navigationTitle("LinkedIn")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Save", action: save).disabled(text.isEmpty) }
            }
            .onAppear { text = person.linkedinURL ?? "" }
        }
    }

    private func save() {
        do {
            person.linkedinURL = try LinkedIn.normalize(text)
            try? context.save()
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// Shown until calendar access is granted.
struct CalendarConnectCard: View {
    @Environment(\.modelContext) private var context
    @Environment(\.openURL) private var openURL
    @State private var authorized = CalendarService.shared.isAuthorized
    @State private var result: String?

    var body: some View {
        if !authorized || result != nil {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: "calendar")
                    .font(.title3)
                    .foregroundStyle(Theme.accent)
                VStack(alignment: .leading, spacing: 4) {
                    Text(authorized ? "Calendar connected" : "Connect your calendar").font(.subheadline.weight(.semibold))
                    Text(result ?? "Meetings get their real names and attendees, so you can link each person's LinkedIn.")
                        .font(.caption).foregroundStyle(.secondary)
                    if !authorized {
                        Button(CalendarService.shared.isDenied ? "Open Settings" : "Connect") { Task { await connect() } }
                            .buttonStyle(.borderedProminent)
                            .controlSize(.small)
                            .foregroundStyle(.black)
                            .padding(.top, 4)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(14)
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .tint(Theme.accent)   // lists may use a neutral selection tint
        }
    }

    private func connect() async {
        if CalendarService.shared.isDenied {
            if let url = URL(string: UIApplication.openSettingsURLString) { openURL(url) }
            return
        }
        authorized = await CalendarService.shared.requestAccess()
        if authorized {
            let n = MeetingLinker.backfill(in: context)
            result = n > 0 ? "Linked \(n) meeting\(n == 1 ? "" : "s") to your calendar." : "New recordings will pick up their calendar event."
        }
    }
}
