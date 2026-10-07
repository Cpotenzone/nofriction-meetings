// The Record sheet's Tauri commands and events: timed recording, the
// recording type and notebooks (src-tauri/src/timed_recording.rs,
// recording_kind.rs, notebooks.rs).

import { invoke } from "@tauri-apps/api/core";
import { isOffline } from "./offline";
import type { RecordingKind } from "./recordingKind";

/** The running plan (`timed-recording-changed` / `-warning` payload). */
export interface TimedStatus {
    meetingId: string;
    startedAt: string;
    /** null: no limit */
    plannedMinutes: number | null;
    deadline: string | null;
    remainingSeconds: number | null;
    warned: boolean;
}

export interface RecordPrefs {
    /** "meeting" | "class" | "personal" */
    defaultKind: string;
    /** "15" | "30" | "60" | "90" | "none" */
    defaultDuration: string;
    /** Most recent first */
    recentNotebooks: string[];
}

export interface TimedAutoStop {
    meetingId: string;
    plannedMinutes: number | null;
}

export const TIMED_EVENTS = {
    changed: "timed-recording-changed",
    warning: "timed-recording-warning",
    autoStop: "timed-recording-auto-stop",
} as const;

export async function getRecordPrefs(): Promise<RecordPrefs> {
    if (isOffline()) return { defaultKind: "meeting", defaultDuration: "none", recentNotebooks: [] };
    return invoke<RecordPrefs>("get_record_prefs");
}

export async function getTimedRecordingStatus(): Promise<TimedStatus | null> {
    if (isOffline()) return null;
    return invoke<TimedStatus | null>("get_timed_recording_status");
}

/** "+15 min" for the recording the UI shows (a stale id is refused). */
export async function extendTimedRecording(meetingId: string | null): Promise<TimedStatus> {
    return invoke<TimedStatus>("extend_timed_recording", { meetingId });
}

/** "No limit" */
export async function removeTimedRecordingLimit(meetingId: string | null): Promise<TimedStatus> {
    return invoke<TimedStatus>("remove_timed_recording_limit", { meetingId });
}

/** Set or clear (null) a recording's notebook; returns the stored name. */
export async function setMeetingNotebook(meetingId: string, notebook: string | null): Promise<string | null> {
    return invoke<string | null>("set_meeting_notebook", { meetingId, notebook });
}

export async function listRecentNotebooks(): Promise<string[]> {
    if (isOffline()) return [];
    return invoke<string[]>("list_recent_notebooks");
}

/** Change a recording's type afterwards; returns the stored value. */
export async function setMeetingRecordingKind(meetingId: string, kind: RecordingKind): Promise<RecordingKind> {
    return invoke<RecordingKind>("set_meeting_recording_kind", { meetingId, kind });
}

/**
 * Emitted once ever, when the first Class-type recording starts (any start
 * path): show the "check your school's policy" notice.
 */
export const CLASS_NOTICE_EVENT = "class-recording-notice";

export const CLASS_RECORDING_NOTICE =
    "Many schools require the instructor's permission to record a class, and some require classmates' consent. Check your school's policy.";
