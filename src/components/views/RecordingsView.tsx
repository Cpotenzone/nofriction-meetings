// Recordings: one search field, the list (filtered by Notebook), and the
// selected recording with its title block and four views: Rewind · Notes
// · Links · Review.

import { useEffect, useState } from "react";
import { onRecordingSeek, takeRecordingSeek } from "../../lib/navigation";
import { MeetingHistory } from "../MeetingHistory";
import { RewindGallery, type RewindSeek } from "../RewindGallery";
import { MeetingNotesPanel } from "../MeetingNotesPanel";
import { MeetingLinksPanel } from "../MeetingLinksPanel";
import { MeetingPeople } from "../People";
import { RecordingTitleBlock } from "../Notebook";
import { StudyPanel } from "../study/StudyPanel";
import { useRecordingKind } from "../../hooks/useRecordingKind";
import { guideTitle } from "../../lib/recordingKind";

type MeetingView = "rewind" | "notes" | "links" | "review";

interface RecordingsViewProps {
    onSelectMeeting: (id: string) => void;
    selectedMeetingId: string | null;
    refreshKey: number;
    isRecording: boolean;
    recordingMeetingId: string | null;
}

export function RecordingsView({ onSelectMeeting, selectedMeetingId, refreshKey, isRecording, recordingMeetingId }: RecordingsViewProps) {
    const [meetingView, setMeetingView] = useState<MeetingView>("rewind");
    // Bumped when a recording's type or notebook is edited, so the list and its filter refresh
    const [metaEdits, setMetaEdits] = useState(0);
    // Links → "first said at 12:03", Review → "Jump to this moment", Chat citations
    // and search results show the moment in Rewind
    const [seek, setSeek] = useState<RewindSeek | null>(null);
    const kind = useRecordingKind(selectedMeetingId);
    const jumpTo = (meetingId: string, ms: number) => {
        setSeek({ meetingId, ms, n: Date.now() });
        setMeetingView("rewind");
    };

    useEffect(() => {
        const pending = takeRecordingSeek(selectedMeetingId);
        if (pending) jumpTo(pending.meetingId, pending.ms);
        return onRecordingSeek((req) => {
            if (req.meetingId === selectedMeetingId) {
                takeRecordingSeek(selectedMeetingId);
                jumpTo(req.meetingId, req.ms);
            }
        });
    }, [selectedMeetingId]);

    // Opening the recording that is still being recorded shows it live
    const live = isRecording && selectedMeetingId !== null && selectedMeetingId === recordingMeetingId;

    const tabs: { id: MeetingView; label: string }[] = [
        { id: "rewind", label: "Rewind" },
        { id: "notes", label: "Notes" },
        { id: "links", label: "Links" },
        { id: "review", label: guideTitle(kind) },
    ];

    return (
        <div className={`recordings-view ${selectedMeetingId ? "has-selection" : ""}`}>
            <MeetingHistory
                onSelectMeeting={onSelectMeeting}
                selectedMeetingId={selectedMeetingId}
                refreshKey={refreshKey + metaEdits}
                compact={!!selectedMeetingId}
                onOpenAt={(id, ms) => {
                    onSelectMeeting(id);
                    jumpTo(id, ms);
                }}
            />
            {selectedMeetingId && (
                <div className="recording-detail">
                    <RecordingTitleBlock meetingId={selectedMeetingId} refreshKey={refreshKey} onSaved={() => setMetaEdits((n) => n + 1)} />
                    <MeetingPeople meetingId={selectedMeetingId} />
                    <div className="deck-tabs" role="tablist" aria-label="Recording view">
                        {tabs.map((t) => (
                            <button
                                key={t.id}
                                role="tab"
                                aria-selected={meetingView === t.id}
                                className={`deck-tab ${meetingView === t.id ? "active" : ""}`}
                                onClick={() => setMeetingView(t.id)}
                            >
                                {t.label}
                            </button>
                        ))}
                    </div>
                    <div className="recording-detail__body">
                        {meetingView === "rewind" ? (
                            <RewindGallery meetingId={selectedMeetingId} isRecording={live} seek={seek} />
                        ) : meetingView === "notes" ? (
                            <MeetingNotesPanel meetingId={selectedMeetingId} />
                        ) : meetingView === "links" ? (
                            <MeetingLinksPanel meetingId={selectedMeetingId} onJump={(ms) => jumpTo(selectedMeetingId, ms)} />
                        ) : (
                            <StudyPanel meetingId={selectedMeetingId} onJump={(ms) => jumpTo(selectedMeetingId, ms)} />
                        )}
                    </div>
                </div>
            )}
        </div>
    );
}
