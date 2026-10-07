// Timed recording + classes: Tauri commands and events
// (src-tauri/src/timed_recording.rs, classes.rs).

import { invoke } from "@tauri-apps/api/core";
import { isOffline } from "./offline";

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
    /** "15" | "30" | "60" | "90" | "none" */
    defaultDuration: string;
    /** Most recent first */
    recentClasses: string[];
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
    if (isOffline()) return { defaultDuration: "none", recentClasses: [] };
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

/** Set or clear (null) a meeting's class; returns the stored name. */
export async function setMeetingClass(meetingId: string, className: string | null): Promise<string | null> {
    return invoke<string | null>("set_meeting_class", { meetingId, className });
}

export async function listRecentClasses(): Promise<string[]> {
    if (isOffline()) return [];
    return invoke<string[]>("list_recent_classes");
}

/** True exactly once: show the "check your school's policy" notice. */
export async function takeClassRecordingNotice(): Promise<boolean> {
    if (isOffline()) return false;
    return invoke<boolean>("take_class_recording_notice");
}

export const CLASS_RECORDING_NOTICE =
    "Many schools require the instructor's permission to record a class, and some require classmates' consent. Check your school's policy.";
