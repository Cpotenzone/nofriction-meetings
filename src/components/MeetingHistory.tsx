// The Recordings list: one search field at the top (titles, people,
// topics, or anything said; ⌘K focuses it), the Notebooks chips, and the
// recordings by day. Deleting a recording is undoable for a few seconds.

import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import * as tauri from "../lib/tauri";
import type { Meeting, CalendarMatchEvent } from "../lib/tauri";
import EmptyState from "./EmptyState";
import ErrorState from "./ErrorState";
import { CalendarIcon, SearchIcon, TrashIcon } from "./icons";
import { withFallback, mockMeetings } from "../lib/offline";
import { NotebookFilterChips, useRecentNotebooks } from "./Notebook";
import { kindLabel, parseKind } from "../lib/recordingKind";
import { groupByDay } from "../lib/topicsLogic";
import { groupHits, SEARCH_PROMPT, type SearchGroup, type SearchHit } from "../lib/searchLogic";
import { onSearchFocus, takeSearchFocus } from "../lib/navigation";
import { useRecordPicker } from "./RecordPicker";
import { useUndoDelete } from "./UndoToast";

/** "Class" / "Personal" tag; meetings (the default) get none. */
function KindTag({ meeting }: { meeting: Meeting }) {
    const kind = parseKind(meeting.recording_kind);
    if (kind === "meeting") return null;
    return <span className="kind-tag">{kindLabel(kind)}</span>;
}

interface MeetingHistoryProps {
    onSelectMeeting: (meetingId: string) => void;
    /** A line that matched: open the recording at that moment */
    onOpenAt: (meetingId: string, ms: number) => void;
    selectedMeetingId: string | null;
    compact?: boolean;
    refreshKey?: number; // Increment to trigger reload
}

const formatDate = (dateStr: string) =>
    new Date(dateStr).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
const formatTime = (dateStr: string) =>
    new Date(dateStr).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", hour12: true });
const formatDuration = (seconds: number | null) => {
    if (!seconds) return "";
    const mins = Math.floor(seconds / 60);
    if (mins >= 60) return `${Math.floor(mins / 60)}h ${mins % 60}m`;
    return `${mins}m`;
};
const clock = (ms: number) => {
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    return `${m}:${String(s % 60).padStart(2, "0")}`;
};

export function MeetingHistory({ onSelectMeeting, onOpenAt, selectedMeetingId, compact = false, refreshKey = 0 }: MeetingHistoryProps) {
    const [meetings, setMeetings] = useState<Meeting[]>([]);
    const [isLoading, setIsLoading] = useState(true);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [calendarMatches, setCalendarMatches] = useState<Record<string, CalendarMatchEvent>>({});
    const [dismissedMatches, setDismissedMatches] = useState<Set<string>>(new Set());
    const [renamingId, setRenamingId] = useState<string | null>(null);
    // Notebook filter (null = All); chips come from the notebooks recordings have
    const [notebookFilter, setNotebookFilter] = useState<string | null>(null);
    const notebooks = useRecentNotebooks(refreshKey);
    const recordPicker = useRecordPicker();
    // The one search
    const [query, setQuery] = useState("");
    const [hits, setHits] = useState<SearchHit[] | null>(null);
    const [searchError, setSearchError] = useState<string | null>(null);
    const searchRef = useRef<HTMLInputElement>(null);
    // Delete: gone at once, Undo for a few seconds
    const undoDelete = useUndoDelete((e) => setLoadError(`Couldn't delete the recording: ${String(e)}`));

    useEffect(() => {
        loadMeetings();
    }, [refreshKey, notebookFilter]); // eslint-disable-line react-hooks/exhaustive-deps

    // ⌘K / View → Search Recordings: the cursor lands here
    useEffect(() => {
        const focus = () => searchRef.current?.focus();
        if (takeSearchFocus()) focus();
        return onSearchFocus(focus);
    }, []);

    // Search as you type (a short pause, so every keystroke doesn't hit the database)
    useEffect(() => {
        const q = query.trim();
        if (!q) {
            setHits(null);
            setSearchError(null);
            return;
        }
        let live = true;
        const t = window.setTimeout(() => {
            invoke<SearchHit[]>("search_recordings", { query: q })
                .then((h) => live && setHits(h))
                .catch((e) => live && setSearchError(String(e)));
        }, 160);
        return () => {
            live = false;
            window.clearTimeout(t);
        };
    }, [query]);

    const loadMeetings = async () => {
        setIsLoading(true);
        setLoadError(null);
        try {
            const data = await withFallback(() => tauri.getMeetings(50, notebookFilter), mockMeetings);
            setMeetings(data);
            // Check recent meetings (last 5) for calendar overlap
            checkCalendarOverlaps(data.slice(0, 5));
        } catch (err) {
            console.error("Failed to load meetings:", err);
            setLoadError(err instanceof Error ? err.message : String(err));
        } finally {
            setIsLoading(false);
        }
    };

    const checkCalendarOverlaps = async (recentMeetings: Meeting[]) => {
        for (const meeting of recentMeetings) {
            if (meeting.calendar_event_id) continue;
            try {
                const match = await tauri.matchRecordingToCalendar(meeting.id);
                if (match) setCalendarMatches((prev) => ({ ...prev, [meeting.id]: match }));
            } catch {
                // Calendar access may not be available
            }
        }
    };

    const handleRename = async (meetingId: string, match: CalendarMatchEvent) => {
        setRenamingId(meetingId);
        try {
            const attendeeStr = match.attendee_names?.slice(0, 3).join(", ") || "";
            const newTitle = attendeeStr ? `${match.event_title} (${attendeeStr})` : match.event_title;
            await tauri.updateMeetingTitle(meetingId, newTitle);
            setMeetings((prev) => prev.map((m) => (m.id === meetingId ? { ...m, title: newTitle, calendar_event_id: match.event_id } : m)));
            setCalendarMatches((prev) => {
                const next = { ...prev };
                delete next[meetingId];
                return next;
            });
        } catch (err) {
            console.error("Failed to rename meeting:", err);
        } finally {
            setRenamingId(null);
        }
    };

    const handleDelete = (e: React.MouseEvent, meeting: Meeting) => {
        e.stopPropagation();
        const before = meetings;
        setMeetings((prev) => prev.filter((m) => m.id !== meeting.id));
        undoDelete.start(
            `Deleted "${meeting.title}"`,
            async () => {
                await tauri.deleteMeeting(meeting.id);
            },
            () => setMeetings(before),
        );
    };

    const byId = useMemo(() => new Map(meetings.map((m) => [m.id, m])), [meetings]);
    const groups: SearchGroup[] | null = useMemo(
        () => (hits ? groupHits(hits, meetings.map((m) => m.id)) : null),
        [hits, meetings],
    );

    const searchField = (
        <div className="rec-search">
            <SearchIcon size={14} />
            <input
                ref={searchRef}
                className="rec-search__input"
                type="search"
                placeholder={SEARCH_PROMPT}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                    if (e.key === "Escape") {
                        setQuery("");
                        (e.target as HTMLInputElement).blur();
                    }
                }}
                aria-label="Search recordings"
                spellCheck={false}
            />
            {!query && <kbd className="rec-search__kbd">⌘K</kbd>}
        </div>
    );

    if (isLoading && meetings.length === 0) {
        return (
            <div className={`meeting-history ${compact ? "is-compact" : ""}`}>
                {searchField}
                <div className="compact-loading">Loading…</div>
            </div>
        );
    }

    if (loadError && meetings.length === 0) {
        return (
            <div className={`meeting-history ${compact ? "is-compact" : ""}`}>
                {searchField}
                <ErrorState
                    title="Couldn't load your recordings"
                    message="Your recordings are safe on this Mac. Try again; if it keeps happening, quit and reopen the app."
                    onRetry={loadMeetings}
                />
            </div>
        );
    }

    // Search results replace the list while the field has text
    if (groups) {
        return (
            <div className={`meeting-history ${compact ? "is-compact" : ""}`}>
                {searchField}
                {searchError && <p className="rec-search__error" role="alert">{searchError}</p>}
                {groups.length === 0 ? (
                    <p className="rec-search__none">Nothing matches "{query.trim()}".</p>
                ) : (
                    <div className="meeting-list scrollable">
                        {groups.map((g) => {
                            const m = byId.get(g.meetingId);
                            if (!m) return null;
                            return (
                                <div
                                    key={g.meetingId}
                                    className={`meeting-item ${selectedMeetingId === m.id ? "selected" : ""}`}
                                    onClick={() => onSelectMeeting(m.id)}
                                >
                                    <div className="meeting-title">
                                        {m.title}
                                        <KindTag meeting={m} />
                                        {m.class_name && <span className="class-tag" title={m.class_name}>{m.class_name}</span>}
                                    </div>
                                    <div className="meeting-date">
                                        {formatDate(m.started_at)}
                                        {g.why.length > 0 && <span> · {g.why.join(" · ")}</span>}
                                    </div>
                                    {g.lines.length > 0 && (
                                        <ul className="rec-search__lines">
                                            {g.lines.map((l, i) => (
                                                <li key={i}>
                                                    <button
                                                        type="button"
                                                        className="rec-search__line"
                                                        onClick={(e) => {
                                                            e.stopPropagation();
                                                            onOpenAt(m.id, l.ms ?? 0);
                                                        }}
                                                        title="Open the recording at this moment"
                                                    >
                                                        {l.ms !== null && <span className="rec-search__at">{clock(l.ms)}</span>}
                                                        <span className="rec-search__text">{l.text}</span>
                                                    </button>
                                                </li>
                                            ))}
                                            {g.moreLines > 0 && <li className="rec-search__more">and {g.moreLines} more</li>}
                                        </ul>
                                    )}
                                </div>
                            );
                        })}
                    </div>
                )}
                {undoDelete.toast}
            </div>
        );
    }

    const filterChips = <NotebookFilterChips notebooks={notebooks} value={notebookFilter} onChange={setNotebookFilter} />;

    if (meetings.length === 0 && notebookFilter) {
        return (
            <div className={`meeting-history ${compact ? "is-compact" : ""}`}>
                {searchField}
                {filterChips}
                <p className="rec-search__none">No recordings in {notebookFilter}.</p>
                {undoDelete.toast}
            </div>
        );
    }

    if (meetings.length === 0) {
        return (
            <div className={`meeting-history ${compact ? "is-compact" : ""}`}>
                {searchField}
                <EmptyState
                    icon={<CalendarIcon size={44} strokeWidth={1.5} />}
                    title="No recordings yet"
                    message="Record a meeting, a class or anything else. The transcript and screens land here, ready to rewind, edit and turn into notes."
                    action={{ label: "Record", onClick: recordPicker.open }}
                />
                {undoDelete.toast}
            </div>
        );
    }

    // Compact: the list beside an open recording
    if (compact) {
        return (
            <div className="meeting-history is-compact">
                {searchField}
                {filterChips}
                <div className="compact-meeting-list scrollable">
                    {meetings.map((meeting) => (
                        <div
                            key={meeting.id}
                            className={`compact-meeting-item ${selectedMeetingId === meeting.id ? "selected" : ""}`}
                            onClick={() => onSelectMeeting(meeting.id)}
                        >
                            <div className="compact-meeting-title">{meeting.title}</div>
                            <KindTag meeting={meeting} />
                            {meeting.class_name && <span className="class-tag" title={meeting.class_name}>{meeting.class_name}</span>}
                            <div className="compact-meeting-date">{formatDate(meeting.started_at)}</div>
                        </div>
                    ))}
                </div>
                {undoDelete.toast}
            </div>
        );
    }

    const groupsByDay = groupByDay(meetings);

    return (
        <div className="meeting-history">
            {searchField}
            {filterChips}
            <div className="meeting-list scrollable">
                {groupsByDay.map((group) => (
                    <div key={group.key} className="meeting-group">
                        <div className="group-head">
                            {group.label}
                            <span className="group-head__count">{group.meetings.length}</span>
                        </div>
                        {group.meetings.map((meeting) => {
                            const match = calendarMatches[meeting.id];
                            const isDismissed = dismissedMatches.has(meeting.id);
                            return (
                                <div
                                    key={meeting.id}
                                    className={`meeting-item ${selectedMeetingId === meeting.id ? "selected" : ""}`}
                                    onClick={() => onSelectMeeting(meeting.id)}
                                >
                                    {match && !isDismissed && (
                                        <div className="cal-match" onClick={(e) => e.stopPropagation()}>
                                            <CalendarIcon size={13} />
                                            <span className="cal-match__text">
                                                This recording overlaps with <strong>{match.event_title}</strong>
                                                {match.attendee_count > 0 && ` (${match.attendee_count} people)`}. Name it after the event?
                                            </span>
                                            <button
                                                className="btn-secondary cal-match__btn"
                                                disabled={renamingId === meeting.id}
                                                onClick={(e) => {
                                                    e.stopPropagation();
                                                    handleRename(meeting.id, match);
                                                }}
                                            >
                                                {renamingId === meeting.id ? "…" : "Rename"}
                                            </button>
                                            <button
                                                className="cal-match__dismiss"
                                                aria-label="Not now"
                                                title="Not now"
                                                onClick={(e) => {
                                                    e.stopPropagation();
                                                    setDismissedMatches((prev) => new Set([...prev, meeting.id]));
                                                }}
                                            >
                                                ✕
                                            </button>
                                        </div>
                                    )}
                                    <div className="meeting-item__row">
                                        <div>
                                            <div className="meeting-title">
                                                {meeting.title}
                                                <KindTag meeting={meeting} />
                                                {meeting.class_name && !notebookFilter && (
                                                    <span className="class-tag" title={meeting.class_name}>{meeting.class_name}</span>
                                                )}
                                            </div>
                                            <div className="meeting-date">
                                                {formatDate(meeting.started_at)} at {formatTime(meeting.started_at)}
                                                {meeting.duration_seconds ? <span> · {formatDuration(meeting.duration_seconds)}</span> : null}
                                            </div>
                                        </div>
                                        <button
                                            className="meeting-item__delete"
                                            onClick={(e) => handleDelete(e, meeting)}
                                            title="Delete recording"
                                            aria-label={`Delete ${meeting.title}`}
                                        >
                                            <TrashIcon size={14} />
                                        </button>
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                ))}
            </div>
            {undoDelete.toast}
        </div>
    );
}
