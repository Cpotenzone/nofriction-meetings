// Editing transcripts/screens + "Strike from the record" (docs/REDACTION.md)
//
// Transcript text from the UI commands keeps strike markers as tokens:
// `⟦strickenid<32 hex>⟧`. They carry only the record id; the record holds
// when/where/why, never what. Offsets sent to the backend are UTF-16 code
// units (plain JS string indexes).

import { invoke } from "@tauri-apps/api/core";

export const STRICKEN_LABEL = "Stricken from the record";
export const STRICKEN_PLACEHOLDER = "[stricken from the record]";
export const SCREEN_STRICKEN_PLACEHOLDER = "[screen stricken from the record]";

export interface RedactionRecord {
    id: string;
    meeting_id: string;
    kind: "words" | "line" | "screen";
    action: "delete" | "strike";
    media_start: string | null;
    media_end: string | null;
    created_at: string;
    reason: string | null;
    transcript_id: number | null;
    item_count: number;
}

export interface PendingDelete {
    id: string;
    meeting_id: string;
    kind: string;
    undo_seconds: number;
}

export interface ActionOutcome {
    record: RedactionRecord | null;
    warnings: string[];
    backups_purged: number;
    backups_deleted: number;
    video_chunks_blanked: number;
}

/** A Delete whose undo window ended but that couldn't be applied yet (kept
 *  pending and retried; holds no content). */
export interface FailedDelete {
    id: string;
    meeting_id: string;
    kind: "words" | "line" | "screen";
    item_count: number;
    created_at: string;
    failed_at: string;
    failure: string;
}

const MARKER_RE = /⟦strickenid([0-9a-f]{32})⟧/g;

/** Plain text for places that don't draw the marker bar. */
export function renderPlain(text: string): string {
    return text.includes("strickenid") ? text.replace(MARKER_RE, STRICKEN_PLACEHOLDER) : text;
}

export type LineToken =
    | { kind: "word"; text: string; start: number; end: number; index: number }
    | { kind: "space"; text: string }
    | { kind: "marker"; id: string };

/** Split a line into word tokens (whitespace-delimited, like the backend
 *  snaps to), whitespace, and strike markers. `index` counts words only. */
export function tokenizeLine(text: string): LineToken[] {
    const out: LineToken[] = [];
    const re = /\s+|\S+/g;
    let m: RegExpExecArray | null;
    let index = 0;
    while ((m = re.exec(text)) !== null) {
        const t = m[0];
        if (/^\s/.test(t)) {
            out.push({ kind: "space", text: t });
            continue;
        }
        const mk = /^⟦strickenid([0-9a-f]{32})⟧$/.exec(t);
        if (mk) {
            out.push({ kind: "marker", id: mk[1] });
            continue;
        }
        out.push({ kind: "word", text: t, start: m.index, end: m.index + t.length, index: index++ });
    }
    return out;
}

export function lineWords(text: string) {
    return tokenizeLine(text).filter((t): t is Extract<LineToken, { kind: "word" }> => t.kind === "word");
}

/** Mirror of the backend Delete edit, used to hide words during the undo
 *  window (the backend applies the real edit when the window ends). */
export function applyLocalDelete(text: string, start: number, end: number): string {
    const tokens = tokenizeLine(text);
    const kept: string[] = [];
    for (const t of tokens) {
        if (t.kind === "word") {
            if (t.end <= start || t.start >= end) kept.push(t.text);
        } else if (t.kind === "marker") {
            kept.push(`⟦strickenid${t.id}⟧`);
        }
    }
    return kept.join(" ");
}

function hm(iso: string | null): string | null {
    if (!iso) return null;
    const d = new Date(iso);
    if (isNaN(d.getTime())) return null;
    return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/** `10:42–10:43 · stricken Oct 1, 2026 · "privileged"` */
export function markerCaption(r: RedactionRecord): string {
    const a = hm(r.media_start);
    const b = hm(r.media_end);
    const parts: string[] = [];
    if (a && b && a !== b) parts.push(`${a}–${b}`);
    else if (a) parts.push(a);
    const when = new Date(r.created_at);
    if (!isNaN(when.getTime())) {
        parts.push(`stricken ${when.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" })}`);
    }
    if (r.kind === "screen" && r.item_count > 1) parts.push(`${r.item_count} screens`);
    if (r.reason) parts.push(`"${r.reason}"`);
    return parts.join(" · ");
}

// ── Commands ────────────────────────────────────────────────────────────

export const redactionApi = {
    deleteWords: (meetingId: string, transcriptId: number, startChar: number, endChar: number, expectedText?: string) =>
        invoke<PendingDelete>("delete_transcript_words", { meetingId, transcriptId, startChar, endChar, expectedText }),
    strikeWords: (meetingId: string, transcriptId: number, startChar: number, endChar: number, reason: string | null, expectedText?: string) =>
        invoke<ActionOutcome>("strike_transcript_words", { meetingId, transcriptId, startChar, endChar, reason, expectedText }),
    deleteLine: (meetingId: string, transcriptId: number) =>
        invoke<PendingDelete>("delete_transcript_line", { meetingId, transcriptId }),
    strikeLine: (meetingId: string, transcriptId: number, reason: string | null) =>
        invoke<ActionOutcome>("strike_transcript_line", { meetingId, transcriptId, reason }),
    deleteScreens: (meetingId: string, ids: string[]) =>
        invoke<PendingDelete>("delete_screens", { meetingId, ids }),
    strikeScreens: (meetingId: string, ids: string[], reason: string | null) =>
        invoke<ActionOutcome>("strike_screens", { meetingId, ids, reason }),
    undo: (id: string) => invoke<void>("undo_redaction", { id }),
    commit: (id: string) => invoke<ActionOutcome | null>("commit_redaction", { id }),
    list: (meetingId: string) => invoke<RedactionRecord[]>("list_redactions", { meetingId }),
    listFailed: (meetingId: string) => invoke<FailedDelete[]>("list_failed_redactions", { meetingId }),
    retryFailed: (meetingId: string) => invoke<FailedDelete[]>("retry_failed_redactions", { meetingId }),
    previewWords: (meetingId: string, transcriptId: number, startChar?: number, endChar?: number) =>
        invoke<string[]>("preview_strike_words", { meetingId, transcriptId, startChar, endChar }),
    previewScreens: (meetingId: string, ids: string[]) =>
        invoke<string[]>("preview_strike_screens", { meetingId, ids }),
    aiStatus: (meetingId: string) =>
        invoke<{ has_notes: boolean; notes_stale: boolean; has_study: boolean; study_stale: boolean }>(
            "get_meeting_ai_status",
            { meetingId },
        ),
};
