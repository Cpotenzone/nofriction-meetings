// noFriction Meetings - Meeting History Component
// Past meetings list with selection

import { useState, useEffect } from "react";
import * as tauri from "../lib/tauri";
import type { Meeting, CalendarMatchEvent } from "../lib/tauri";
import EmptyState from "./EmptyState";
import { CalendarIcon, BrainIcon, TrashIcon } from "./icons";
import { withFallback, mockMeetings } from "../lib/offline";

interface MeetingHistoryProps {
    onSelectMeeting: (meetingId: string) => void;
    selectedMeetingId: string | null;
    compact?: boolean;
    refreshKey?: number; // Increment to trigger reload
}

export function MeetingHistory({ onSelectMeeting, selectedMeetingId, compact = false, refreshKey = 0 }: MeetingHistoryProps) {
    const [meetings, setMeetings] = useState<Meeting[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [calendarMatches, setCalendarMatches] = useState<Record<string, CalendarMatchEvent>>({});
    const [dismissedMatches, setDismissedMatches] = useState<Set<string>>(new Set());
    const [renamingId, setRenamingId] = useState<string | null>(null);

    useEffect(() => {
        loadMeetings();
    }, [refreshKey]); // Reload when refreshKey changes

    const loadMeetings = async () => {
        setIsLoading(true);
        try {
            const data = await withFallback(() => tauri.getMeetings(50), mockMeetings);
            setMeetings(data);
            // Check recent meetings (last 5) for calendar overlap
            checkCalendarOverlaps(data.slice(0, 5));
        } catch (err) {
            console.error("Failed to load meetings:", err);
        } finally {
            setIsLoading(false);
        }
    };

    const checkCalendarOverlaps = async (recentMeetings: Meeting[]) => {
        for (const meeting of recentMeetings) {
            // Skip if already has a calendar event linked or title doesn't look auto-generated
            if (meeting.calendar_event_id) continue;
            try {
                const match = await tauri.matchRecordingToCalendar(meeting.id);
                if (match) {
                    setCalendarMatches(prev => ({ ...prev, [meeting.id]: match }));
                }
            } catch {
                // Calendar access may not be available, silently skip
            }
        }
    };

    const handleRename = async (meetingId: string, match: CalendarMatchEvent) => {
        setRenamingId(meetingId);
        try {
            // Build title from calendar event
            const attendeeStr = match.attendee_names?.slice(0, 3).join(', ') || '';
            const newTitle = attendeeStr
                ? `${match.event_title} (${attendeeStr})`
                : match.event_title;
            await tauri.updateMeetingTitle(meetingId, newTitle);
            // Update local state
            setMeetings(prev => prev.map(m =>
                m.id === meetingId ? { ...m, title: newTitle, calendar_event_id: match.event_id } : m
            ));
            setCalendarMatches(prev => {
                const next = { ...prev };
                delete next[meetingId];
                return next;
            });
        } catch (err) {
            console.error('Failed to rename meeting:', err);
        } finally {
            setRenamingId(null);
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
                <EmptyState
                    icon={<CalendarIcon size={44} strokeWidth={1.5} />}
                    title="No recordings yet"
                    message="Hit START CAPTURE in the top bar during your next meeting. Every screen frame and every word lands here, ready to rewind."
                />
            </div>
        );
    }

    // Compact mode for sidebar
    if (compact) {
        return (
            <div className="compact-meeting-list" style={{ overflowY: 'auto', maxHeight: '100%' }}>
                {meetings.map((meeting) => (
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
                {meetings.map((meeting) => {
                    const match = calendarMatches[meeting.id];
                    const isDismissed = dismissedMatches.has(meeting.id);
                    return (
                        <div
                            key={meeting.id}
                            className={`meeting-item ${selectedMeetingId === meeting.id ? "selected" : ""}`}
                            onClick={() => onSelectMeeting(meeting.id)}
                        >
                            {/* Calendar Overlap Banner */}
                            {match && !isDismissed && (
                                <div
                                    onClick={(e) => e.stopPropagation()}
                                    style={{
                                        padding: '8px 12px',
                                        marginBottom: '8px',
                                        borderRadius: '6px',
                                        background: 'rgba(74, 222, 128, 0.08)',
                                        border: '1px solid rgba(74, 222, 128, 0.2)',
                                        fontSize: '0.75rem',
                                        display: 'flex',
                                        alignItems: 'center',
                                        gap: '8px',
                                    }}
                                >
                                    <span style={{ flex: 1 }}>
                                        📅 This recording overlaps with <strong>{match.event_title}</strong>
                                        {match.attendee_count > 0 && ` (${match.attendee_count} attendees)`}
                                        {' — '}Rename?
                                    </span>
                                    <button
                                        className="btn btn-primary"
                                        style={{ fontSize: '0.7rem', padding: '3px 10px' }}
                                        disabled={renamingId === meeting.id}
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            handleRename(meeting.id, match);
                                        }}
                                    >
                                        {renamingId === meeting.id ? '...' : '✓ Rename'}
                                    </button>
                                    <button
                                        className="btn btn-ghost"
                                        style={{ fontSize: '0.7rem', padding: '3px 8px', opacity: 0.5 }}
                                        onClick={(e) => {
                                            e.stopPropagation();
                                            setDismissedMatches(prev => new Set([...prev, meeting.id]));
                                        }}
                                    >
                                        ✗
                                    </button>
                                </div>
                            )}
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
                                            .catch(err => console.error(err));
                                    }}
                                    title="Send to Intel Workflow"
                                    style={{ padding: "4px 8px", fontSize: "0.75rem", marginRight: "4px" }}
                                >
                                    <BrainIcon size={14} />
                                </button>
                                <button
                                    className="btn btn-ghost"
                                    onClick={(e) => handleDelete(e, meeting.id)}
                                    title="Delete meeting"
                                    style={{ padding: "4px 8px", fontSize: "0.75rem", opacity: 0.5 }}
                                >
                                    <TrashIcon size={14} />
                                </button>
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
