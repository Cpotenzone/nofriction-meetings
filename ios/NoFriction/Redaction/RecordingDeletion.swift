import Foundation
import SwiftData

/// Delete recording: everything of one recording, everywhere on this
/// iPhone (docs/REDACTION.md "Deleting a whole meeting"). Used by the
/// recording's Delete and by Sync when the Mac deleted it (docs/SYNC.md).
@MainActor
enum RecordingDeletion {
    static func delete(_ meeting: Meeting, context: ModelContext, redactions: RedactionCenter?, screenCapture: ScreenCaptureCenter?) {
        redactions?.discardPending(for: meeting)
        if let name = meeting.audioFileName { try? FileManager.default.removeItem(at: Storage.audio.appending(path: name)) }
        // An Apple Watch recording: any copy still staged from the watch goes
        // too, and the import log keeps a re-delivery from bringing it back
        if let id = meeting.sourceRecordingID.flatMap(UUID.init(uuidString:)) { WatchInbox.shared.remove(id) }
        for s in meeting.snapshots { try? FileManager.default.removeItem(at: s.fileURL) }
        // Screen capture: app audio still waiting to be transcribed, and any
        // broadcast files of this recording still in the shared container
        screenCapture?.purge(meeting)
        // Chat answers that cited this recording go with it (docs/TOPICS_AND_CHAT.md)
        ChatStore.purge(meetingID: meeting.id, title: meeting.title, deleted: true, context: context)
        context.delete(meeting)
        try? context.save()
    }
}
