// noFriction Meetings - Meeting History Component
// Past meetings list with selection

import { useState, useEffect } from "react";
import * as tauri from "../lib/tauri";
import type { Meeting } from "../lib/tauri";

interface MeetingHistoryProps {
    onSelectMeeting: (meetingId: string) => void;
    selectedMeetingId: string | null;
    compact?: boolean;
    refreshKey?: number; // Increment to trigger reload
}

export function MeetingHistory({ onSelectMeeting, selectedMeetingId, compact = false, refreshKey = 0 }: MeetingHistoryProps) {
    const [meetings, setMeetings] = useState<Meeting[]>([]);
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        loadMeetings();
    }, [refreshKey]); // Reload when refreshKey changes

    const loadMeetings = async () => {
        setIsLoading(true);
        try {
            const data = await tauri.getMeetings(50);
            setMeetings(data);
        } catch (err) {
            console.error("Failed to load meetings:", err);
        } finally {
            setIsLoading(false);
        }
    };

    const formatDate = (dateStr: string) => {
        const date = new Date(dateStr);
        return date.toLocaleDateString("en-US", {
            month: "short",
            day: "numeric",
            year: "numeric",
        });
    };

    const formatTime = (dateStr: string) => {
        const date = new Date(dateStr);
        return date.toLocaleTimeString("en-US", {
            hour: "numeric",
            minute: "2-digit",
            hour12: true,
        });
    };

    const formatDuration = (seconds: number | null) => {
        if (!seconds) return "";
        const mins = Math.floor(seconds / 60);
        if (mins >= 60) {
            const hrs = Math.floor(mins / 60);
            const remainingMins = mins % 60;
            return `${hrs}h ${remainingMins}m`;
        }
        return `${mins}m`;
    };

    const handleDelete = async (e: React.MouseEvent, meetingId: string) => {
        e.stopPropagation();
        if (confirm("Delete this meeting and all its transcripts?")) {
            try {
                await tauri.deleteMeeting(meetingId);
                setMeetings((prev) => prev.filter((m) => m.id !== meetingId));
            } catch (err) {
                console.error("Failed to delete meeting:", err);
            }
        }
    };

    if (isLoading) {
        if (compact) {
            return <div className="compact-loading">Loading...</div>;
        }
        return (
            <div className="meeting-history">
                <h3>Past Meetings</h3>
                <div className="empty-state">
                    <div className="empty-state-text">Loading...</div>
                </div>
            </div>
        );
    }

    if (meetings.length === 0) {
        if (compact) {
            return <div className="compact-empty">No meetings yet</div>;
        }
        return (
            <div className="meeting-history">
                <h3>Past Meetings</h3>
                <div className="empty-state">
                    <div className="empty-state-icon">📅</div>
                    <p className="empty-state-text">No meetings yet. Start recording to capture your first meeting!</p>
                </div>
            </div>
        );
    }

    // Compact mode for sidebar
    if (compact) {
        return (
            <div className="compact-meeting-list">
                {meetings.slice(0, 10).map((meeting) => (
                    <div
                        key={meeting.id}
                        className={`compact-meeting-item ${selectedMeetingId === meeting.id ? "selected" : ""}`}
                        onClick={() => onSelectMeeting(meeting.id)}
                    >
                        <div className="compact-meeting-title">{meeting.title}</div>
                        <div className="compact-meeting-date">
                            {formatDate(meeting.started_at)}
                        </div>
                    </div>
                ))}
            </div>
        );
    }

    return (
        <div className="meeting-history">
            <h3>Past Meetings ({meetings.length})</h3>
            <div className="meeting-list scrollable">
                {meetings.map((meeting) => (
                    <div
                        key={meeting.id}
                        className={`meeting-item ${selectedMeetingId === meeting.id ? "selected" : ""}`}
                        onClick={() => onSelectMeeting(meeting.id)}
                    >
                        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                            <div>
                                <div className="meeting-title">{meeting.title}</div>
                                <div className="meeting-date">
                                    {formatDate(meeting.started_at)} at {formatTime(meeting.started_at)}
                                    {meeting.duration_seconds && (
                                        <span> · {formatDuration(meeting.duration_seconds)}</span>
                                    )}
                                </div>
                            </div>
                            <button
                                className="btn btn-ghost"
                                onClick={(e) => {
                                    e.stopPropagation();
                                    // TODO: Add visual feedback for ingest trigger
                                    tauri.triggerMeetingIngest(meeting.id)
                                        .then(msg => console.log(msg))
                                        .catch(err => console.error(err));
                                }}
                                title="Send to Intel Workflow"
                                style={{ padding: "4px 8px", fontSize: "0.75rem", marginRight: "4px" }}
                            >
                                🧠
                            </button>
                            <button
                                className="btn btn-ghost"
                                onClick={(e) => handleDelete(e, meeting.id)}
                                title="Delete meeting"
                                style={{ padding: "4px 8px", fontSize: "0.75rem", opacity: 0.5 }}
                            >
                                🗑️
                            </button>
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}
