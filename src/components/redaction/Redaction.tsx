// Editing transcripts/screens + "Strike from the record" UI pieces
// (docs/REDACTION.md): word-token selection, marker rendering, the Strike
// confirmation, and the Delete undo toast.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
    STRICKEN_LABEL,
    lineWords,
    markerCaption,
    redactionApi,
    tokenizeLine,
    type FailedDelete,
    type MsRange,
    type RedactionRecord,
    type TimeRangePreview,
    type VideoJob,
    type VideoJobEvent,
} from "../../lib/redaction";
import { clockAt, durationLabel, parseClock, spanLabel } from "../../lib/timelineSelection";
import "./Redaction.css";

// ── Markers ─────────────────────────────────────────────────────────────

/** Dark bar shown where words were stricken. Not editable, not selectable,
 *  nothing behind it. */
export function StrickenMarker({ record }: { record?: RedactionRecord }) {
    const caption = record ? markerCaption(record) : "";
    return (
        <span
            className="rd-marker"
            role="note"
            aria-label={caption ? `${STRICKEN_LABEL}, ${caption}` : STRICKEN_LABEL}
            contentEditable={false}
            suppressContentEditableWarning
        >
            <span className="rd-marker-label">{STRICKEN_LABEL}</span>
            {caption && <span className="rd-marker-caption">{caption}</span>}
        </span>
    );
}

/** Hatched placeholder that replaces a stricken screen's thumbnail. */
export function ScreenStrickenCard({
    record,
    className = "",
    style,
}: {
    record: RedactionRecord;
    className?: string;
    style?: React.CSSProperties;
}) {
    return (
        <div
            className={`rd-screen-card ${className}`}
            style={style}
            role="note"
            aria-label={`${STRICKEN_LABEL}, ${markerCaption(record)}`}
        >
            <span className="rd-marker-label">{STRICKEN_LABEL}</span>
            <span className="rd-marker-caption">{markerCaption(record)}</span>
        </div>
    );
}

/** Read-only line: words as text, markers as bars. */
export function TranscriptText({ text, records }: { text: string; records?: Map<string, RedactionRecord> }) {
    const tokens = useMemo(() => tokenizeLine(text), [text]);
    return (
        <>
            {tokens.map((t, i) =>
                t.kind === "marker" ? (
                    <StrickenMarker key={i} record={records?.get(t.id)} />
                ) : (
                    <span key={i}>{t.text}</span>
                ),
            )}
        </>
    );
}

// ── Word selection ──────────────────────────────────────────────────────

export interface SelPos {
    li: number; // line index in the rendered list
    wi: number; // word index within the line
}

export function useWordSelection() {
    const [anchor, setAnchor] = useState<SelPos | null>(null);
    const [focus, setFocus] = useState<SelPos | null>(null);
    const dragging = useRef(false);

    const clear = useCallback(() => {
        setAnchor(null);
        setFocus(null);
    }, []);

    useEffect(() => {
        const up = () => {
            dragging.current = false;
        };
        const key = (e: KeyboardEvent) => {
            if (e.key === "Escape") clear();
        };
        window.addEventListener("mouseup", up);
        window.addEventListener("keydown", key);
        return () => {
            window.removeEventListener("mouseup", up);
            window.removeEventListener("keydown", key);
        };
    }, [clear]);

    const range = useMemo(() => {
        if (!anchor || !focus) return null;
        const before = anchor.li < focus.li || (anchor.li === focus.li && anchor.wi <= focus.wi);
        return before ? { from: anchor, to: focus } : { from: focus, to: anchor };
    }, [anchor, focus]);

    const begin = useCallback(
        (pos: SelPos, shift: boolean) => {
            if (shift && anchor) {
                setFocus(pos);
            } else {
                setAnchor(pos);
                setFocus(pos);
            }
            dragging.current = true;
        },
        [anchor],
    );

    const extend = useCallback((pos: SelPos) => {
        if (dragging.current) setFocus(pos);
    }, []);

    const selectLine = useCallback((li: number, nWords: number) => {
        if (nWords <= 0) return;
        setAnchor({ li, wi: 0 });
        setFocus({ li, wi: nWords - 1 });
    }, []);

    /** Select one word (not a drag: moving the pointer doesn't extend it) */
    const selectWord = useCallback((pos: SelPos) => {
        dragging.current = false;
        setAnchor(pos);
        setFocus(pos);
    }, []);

    /** Selected [first, last] word indexes in line `li`, if any */
    const wordRangeForLine = useCallback(
        (li: number, nWords: number): [number, number] | null => {
            if (!range || li < range.from.li || li > range.to.li || nWords === 0) return null;
            const ws = li === range.from.li ? range.from.wi : 0;
            const we = li === range.to.li ? Math.min(range.to.wi, nWords - 1) : nWords - 1;
            return ws <= we ? [ws, we] : null;
        },
        [range],
    );

    return { range, begin, extend, clear, selectLine, selectWord, wordRangeForLine };
}

export interface WordSegment {
    transcriptId: number;
    lineText: string;
    start: number; // UTF-16
    end: number;
    wholeLine: boolean;
    words: string;
    wordCount: number;
}

/** Turn a selection over rendered lines into per-line backend ranges. */
export function selectionSegments(
    lines: { id: number; text: string }[],
    wordRangeForLine: (li: number, n: number) => [number, number] | null,
): WordSegment[] {
    const out: WordSegment[] = [];
    lines.forEach((line, li) => {
        const words = lineWords(line.text);
        const r = wordRangeForLine(li, words.length);
        if (!r) return;
        const picked = words.slice(r[0], r[1] + 1);
        if (picked.length === 0) return;
        out.push({
            transcriptId: line.id,
            lineText: line.text,
            start: picked[0].start,
            end: picked[picked.length - 1].end,
            wholeLine: r[0] === 0 && r[1] === words.length - 1 && !line.text.includes("strickenid"),
            words: picked.map((w) => w.text).join(" "),
            wordCount: picked.length,
        });
    });
    return out;
}

/** A transcript line rendered as clickable word tokens. */
export function RedactableLine({
    text,
    li,
    selected,
    records,
    editable,
    onWordDown,
    onWordEnter,
}: {
    text: string;
    li: number;
    selected: [number, number] | null;
    records?: Map<string, RedactionRecord>;
    editable: boolean;
    onWordDown: (pos: SelPos, shift: boolean) => void;
    onWordEnter: (pos: SelPos) => void;
}) {
    const tokens = useMemo(() => tokenizeLine(text), [text]);
    return (
        <span className={`rd-line ${editable ? "rd-editable" : ""}`}>
            {tokens.map((t, i) => {
                if (t.kind === "space") return <span key={i}>{t.text}</span>;
                if (t.kind === "marker") return <StrickenMarker key={i} record={records?.get(t.id)} />;
                const sel = !!selected && t.index >= selected[0] && t.index <= selected[1];
                return (
                    <span
                        key={i}
                        data-wi={t.index}
                        className={`rd-word${sel ? " rd-sel" : ""}`}
                        onMouseDown={(e) => {
                            if (!editable || e.button !== 0) return;
                            e.preventDefault();
                            onWordDown({ li, wi: t.index }, e.shiftKey);
                        }}
                        onMouseEnter={() => editable && onWordEnter({ li, wi: t.index })}
                    >
                        {t.text}
                    </span>
                );
            })}
        </span>
    );
}

// ── Action bar ──────────────────────────────────────────────────────────

export function RedactionActionBar({
    label,
    onDelete,
    onStrike,
    onClear,
    extra,
    busy,
}: {
    label: string;
    onDelete: () => void;
    onStrike: () => void;
    onClear: () => void;
    extra?: React.ReactNode;
    busy?: boolean;
}) {
    return (
        <div className="rd-actionbar rd-actionbar-sticky" role="toolbar" aria-label="Edit selection">
            <span className="rd-actionbar-count">{label}</span>
            {extra}
            <button className="rd-btn" onClick={onDelete} disabled={busy}>
                Delete
            </button>
            <button className="rd-btn rd-btn-strike" onClick={onStrike} disabled={busy}>
                Strike from the record…
            </button>
            <button className="rd-btn rd-btn-ghost" onClick={onClear} aria-label="Clear selection" title="Clear (Esc)">
                ✕
            </button>
        </div>
    );
}

// ── Delete (undo toast) + Strike (confirmation) controller ──────────────

type StrikeRequest =
    | { type: "words"; segments: WordSegment[] }
    | { type: "screens"; ids: string[] }
    /** Whole transcript lines (the transcript pane, not linked to screens) */
    | { type: "lines"; ids: number[]; words: number };

/** Delete or Strike a fixed set of time ranges (a linked selection of
 *  screens and transcript lines), with the exact preview. */
export interface RangesRequest {
    mode: "delete" | "strike";
    ranges: MsRange[];
    /** Selected lines with no recorded time: removed whole, by id, in the
     *  same action */
    lineIds: number[];
    startedAt: string;
    /** Shown above the preview (e.g. why a Delete asks first) */
    note?: string;
    /** A Delete was queued: hide what it removes during the undo window */
    onDeleted?: (p: TimeRangePreview | null, lineIds: number[]) => void;
    /** A Strike finished */
    onStruck?: () => void;
}

/** Opens the time-range dialog (Delete or Strike a block of meeting time). */
export interface TimeRangeRequest {
    startMs: number;
    endMs: number;
    /** RFC3339 meeting start: times are shown and typed as wall-clock */
    startedAt: string;
    /** End of the meeting (ms from the start), to bound the inputs */
    maxMs: number;
    /** Called when a Delete is queued, to hide what it removes during the
     *  undo window */
    onDeleted?: (p: TimeRangePreview) => void;
}

interface ToastState {
    ids: string[];
    label: string;
    until: number;
}

function plural(n: number, one: string, many: string) {
    return `${n} ${n === 1 ? one : many}`;
}

/**
 * Owns the undo toast and the Strike confirmation for one meeting.
 * `onChanged` refetches the view (after commit, undo, strike, failure).
 */
export function useRedaction(meetingId: string | null, onChanged: () => void) {
    const [toast, setToast] = useState<ToastState | null>(null);
    const [now, setNow] = useState(Date.now());
    const [message, setMessage] = useState<{ text: string; error: boolean } | null>(null);
    const [strike, setStrike] = useState<StrikeRequest | null>(null);
    const [failed, setFailed] = useState<FailedDelete[]>([]);
    const [retrying, setRetrying] = useState(false);
    const [range, setRange] = useState<TimeRangeRequest | null>(null);
    const [ranges, setRanges] = useState<RangesRequest | null>(null);
    // Screen video blanking runs in the background after a delete/strike
    const [videoJobs, setVideoJobs] = useState<VideoJob[]>([]);
    const [videoPercent, setVideoPercent] = useState<number | null>(null);
    const changedRef = useRef(onChanged);
    changedRef.current = onChanged;

    const refreshVideo = useCallback(async () => {
        if (!meetingId) {
            setVideoJobs([]);
            return;
        }
        try {
            setVideoJobs(await redactionApi.listVideoJobs(meetingId));
        } catch {
            /* not running inside Tauri */
        }
    }, [meetingId]);
    const refreshVideoRef = useRef(refreshVideo);
    refreshVideoRef.current = refreshVideo;
    useEffect(() => {
        setVideoPercent(null);
        refreshVideo();
    }, [refreshVideo]);

    // Deletes that couldn't be applied yet (kept pending by the backend,
    // retried at launch / next commit, or now via Retry)
    const refreshFailed = useCallback(async () => {
        if (!meetingId) {
            setFailed([]);
            return;
        }
        try {
            setFailed(await redactionApi.listFailed(meetingId));
        } catch {
            /* not running inside Tauri */
        }
    }, [meetingId]);
    const refreshFailedRef = useRef(refreshFailed);
    refreshFailedRef.current = refreshFailed;
    useEffect(() => {
        refreshFailed();
    }, [refreshFailed]);

    // Backend commits the pending delete when the window ends (or on quit)
    useEffect(() => {
        const offs: (() => void)[] = [];
        let disposed = false;
        (async () => {
            const a = await listen<{ meeting_id: string; warnings?: string[] }>("redaction_committed", (e) => {
                if (e.payload.meeting_id !== meetingId) return;
                if (e.payload.warnings && e.payload.warnings.length) {
                    setMessage({ text: e.payload.warnings.join(" "), error: true });
                }
                refreshFailedRef.current();
                refreshVideoRef.current();
                changedRef.current();
            });
            const v = await listen<VideoJobEvent>("video_blank_progress", (e) => {
                if (e.payload.meeting_id !== meetingId) return;
                if (e.payload.status === "running") {
                    setVideoPercent(e.payload.percent ?? 0);
                    return;
                }
                setVideoPercent(null);
                refreshVideoRef.current();
                // A strike marker stops saying "video pending"
                if (e.payload.status === "done") changedRef.current();
            });
            if (disposed) v();
            else offs.push(v);
            const b = await listen<{ meeting_id: string; error: string; retryable?: boolean }>("redaction_failed", (e) => {
                if (e.payload.meeting_id !== meetingId) return;
                setMessage({
                    text: e.payload.retryable
                        ? `Delete couldn't be completed yet: ${e.payload.error} It will be retried.`
                        : `Delete not applied: ${e.payload.error}`,
                    error: true,
                });
                refreshFailedRef.current();
                changedRef.current();
            });
            if (disposed) {
                a();
                b();
            } else {
                offs.push(a, b);
            }
        })().catch(() => {
            /* not running inside Tauri (tests / browser preview) */
        });
        return () => {
            disposed = true;
            offs.forEach((off) => off());
        };
    }, [meetingId]);

    useEffect(() => {
        if (!toast) return;
        const t = setInterval(() => {
            const n = Date.now();
            setNow(n);
            if (n >= toast.until) setToast(null);
        }, 250);
        return () => clearInterval(t);
    }, [toast]);

    useEffect(() => {
        if (!message || message.error) return;
        const t = setTimeout(() => setMessage(null), 4000);
        return () => clearTimeout(t);
    }, [message]);

    const showToast = useCallback((ids: string[], label: string, seconds: number) => {
        setToast((prev) => {
            // A newer delete replaces the toast: apply the older one now
            // rather than leave it undoable with no button.
            if (prev) prev.ids.forEach((id) => redactionApi.commit(id).catch(() => undefined));
            return { ids, label, until: Date.now() + seconds * 1000 };
        });
        setNow(Date.now());
    }, []);

    const deleteWords = useCallback(
        async (segments: WordSegment[]): Promise<boolean> => {
            if (!meetingId || segments.length === 0) return false;
            const ids: string[] = [];
            let seconds = 5;
            try {
                for (const s of segments) {
                    const p = s.wholeLine
                        ? await redactionApi.deleteLine(meetingId, s.transcriptId)
                        : await redactionApi.deleteWords(meetingId, s.transcriptId, s.start, s.end, s.lineText);
                    ids.push(p.id);
                    seconds = p.undo_seconds;
                }
            } catch (e) {
                // All-or-nothing from the user's view
                await Promise.all(ids.map((id) => redactionApi.undo(id).catch(() => undefined)));
                setMessage({ text: `Couldn't delete: ${e}`, error: true });
                changedRef.current();
                return false;
            }
            const n = segments.reduce((a, s) => a + s.wordCount, 0);
            showToast(ids, `Deleted ${plural(n, "word", "words")}`, seconds);
            return true;
        },
        [meetingId, showToast],
    );

    const deleteScreens = useCallback(
        async (ids: string[]): Promise<boolean> => {
            if (!meetingId || ids.length === 0) return false;
            try {
                const p = await redactionApi.deleteScreens(meetingId, ids);
                showToast([p.id], `Deleted ${plural(ids.length, "screen", "screens")}`, p.undo_seconds);
                return true;
            } catch (e) {
                setMessage({ text: `Couldn't delete: ${e}`, error: true });
                return false;
            }
        },
        [meetingId, showToast],
    );

    /** Whole transcript lines as one grouped action: one toast, one undo
     *  (all or nothing). */
    const deleteLines = useCallback(
        async (lineIds: number[]): Promise<boolean> => {
            if (!meetingId || lineIds.length === 0) return false;
            const ids: string[] = [];
            let seconds = 5;
            try {
                for (const id of lineIds) {
                    const p = await redactionApi.deleteLine(meetingId, id);
                    ids.push(p.id);
                    seconds = p.undo_seconds;
                }
            } catch (e) {
                await Promise.all(ids.map((id) => redactionApi.undo(id).catch(() => undefined)));
                setMessage({ text: `Couldn't delete: ${e}`, error: true });
                changedRef.current();
                return false;
            }
            showToast(ids, `Deleted ${plural(lineIds.length, "line", "lines")}`, seconds);
            return true;
        },
        [meetingId, showToast],
    );

    /** A linked selection: every range (one pending delete for all of them)
     *  plus any lines with no recorded time, under one toast and one undo. */
    const deleteRanges = useCallback(
        async (p: TimeRangePreview | null, lineIds: number[], startedAt: string): Promise<boolean> => {
            if (!meetingId) return false;
            const doRanges = !!p && !p.nothing && p.ranges.length > 0;
            if (!doRanges && lineIds.length === 0) {
                setMessage({ text: "Nothing was captured in that span.", error: false });
                return false;
            }
            const ids: string[] = [];
            let seconds = 5;
            try {
                if (doRanges && p) {
                    const pending = await redactionApi.deleteTimeRanges(meetingId, p.ranges, p.counts);
                    ids.push(pending.id);
                    seconds = pending.undo_seconds;
                }
                for (const id of lineIds) {
                    const pd = await redactionApi.deleteLine(meetingId, id);
                    ids.push(pd.id);
                    seconds = pd.undo_seconds;
                }
            } catch (e) {
                await Promise.all(ids.map((id) => redactionApi.undo(id).catch(() => undefined)));
                setMessage({ text: `Couldn't delete: ${e}`, error: true });
                changedRef.current();
                return false;
            }
            const what: string[] = [];
            if (doRanges && p?.counts.screens) what.push(plural(p.counts.screens, "screen", "screens"));
            const nLines = (doRanges && p ? p.counts.lines_whole + p.counts.lines_split : 0) + lineIds.length;
            if (nLines) what.push(plural(nLines, "line", "lines"));
            const when =
                doRanges && p
                    ? `${spanLabel(startedAt, p.start_ms, p.end_ms)}${p.ranges.length > 1 ? `, ${p.ranges.length} spans` : ""}`
                    : "";
            showToast(ids, `Deleted ${[when, what.length ? `(${what.join(", ")})` : ""].filter(Boolean).join(" ")}`, seconds);
            return true;
        },
        [meetingId, showToast],
    );

    const undo = useCallback(async () => {
        if (!toast) return;
        const ids = toast.ids;
        setToast(null);
        const results = await Promise.allSettled(ids.map((id) => redactionApi.undo(id)));
        const failed = results.find((r) => r.status === "rejected") as PromiseRejectedResult | undefined;
        if (failed) setMessage({ text: String(failed.reason), error: true });
        changedRef.current();
    }, [toast]);

    const dismissToast = useCallback(async () => {
        if (!toast) return;
        const ids = toast.ids;
        setToast(null);
        const results = await Promise.allSettled(ids.map((id) => redactionApi.commit(id)));
        const failedCommit = results.find((r) => r.status === "rejected") as PromiseRejectedResult | undefined;
        if (failedCommit) setMessage({ text: `Delete failed: ${failedCommit.reason}`, error: true });
        refreshFailed();
        changedRef.current();
    }, [toast, refreshFailed]);

    const retryFailed = useCallback(async () => {
        if (!meetingId) return;
        setRetrying(true);
        try {
            const left = await redactionApi.retryFailed(meetingId);
            setFailed(left);
            if (left.length) setMessage({ text: `Still couldn't complete: ${left[0].failure}`, error: true });
            else setMessage({ text: "Deleted.", error: false });
        } catch (e) {
            setMessage({ text: String(e), error: true });
        } finally {
            setRetrying(false);
            changedRef.current();
        }
    }, [meetingId]);

    const retryVideo = useCallback(async () => {
        if (!meetingId) return;
        try {
            setVideoJobs(await redactionApi.retryVideoJobs(meetingId));
        } catch (e) {
            setMessage({ text: String(e), error: true });
        }
    }, [meetingId]);

    /** Queue a time-range Delete (5-second undo, like every Delete). */
    const deleteTimeRange = useCallback(
        async (p: TimeRangePreview, startedAt: string): Promise<boolean> => {
            if (!meetingId) return false;
            try {
                const pending = await redactionApi.deleteTimeRange(meetingId, p.start_ms, p.end_ms, p.counts);
                const what: string[] = [];
                if (p.counts.screens) what.push(plural(p.counts.screens, "screen", "screens"));
                const lines = p.counts.lines_whole + p.counts.lines_split;
                if (lines) what.push(plural(lines, "line", "lines"));
                showToast(
                    [pending.id],
                    `Deleted ${spanLabel(startedAt, p.start_ms, p.end_ms)}${what.length ? ` (${what.join(", ")})` : ""}`,
                    pending.undo_seconds,
                );
                return true;
            } catch (e) {
                setMessage({ text: `Couldn't delete: ${e}`, error: true });
                return false;
            }
        },
        [meetingId, showToast],
    );

    const cancelFailed = useCallback(async () => {
        await Promise.allSettled(failed.map((f) => redactionApi.undo(f.id)));
        refreshFailed();
        changedRef.current();
    }, [failed, refreshFailed]);

    const videoFailed = videoJobs.filter((j) => j.status === "failed");
    const toolMissing = videoFailed.some((j) => j.tool_missing);
    const ui = (
        <>
            {(videoPercent !== null || videoJobs.length > 0) && (
                <div className={`rd-video${videoFailed.length && videoPercent === null ? " rd-video-failed" : ""}`} role="status" aria-live="polite">
                    {videoPercent !== null ? (
                        <>
                            <span>Removing from screen video… {videoPercent}%</span>
                            <span className="rd-video-bar" aria-hidden="true">
                                <span style={{ width: `${videoPercent}%` }} />
                            </span>
                        </>
                    ) : videoFailed.length > 0 ? (
                        <>
                            <span>
                                {toolMissing
                                    ? `ffmpeg isn't installed, so ${plural(videoFailed.length, "removed moment is", "removed moments are")} still in the screen video. Install it (brew install ffmpeg), then Retry.`
                                    : `The screen video still contains ${plural(videoFailed.length, "removed moment", "removed moments")}; blanking failed and will be retried.`}
                                {!toolMissing && videoFailed[videoFailed.length - 1].last_error && (
                                    <span className="rd-failed-reason"> — {videoFailed[videoFailed.length - 1].last_error}</span>
                                )}
                            </span>
                            <button className="rd-btn" onClick={retryVideo}>
                                Retry
                            </button>
                        </>
                    ) : (
                        <span>
                            Screen video: {plural(videoJobs.length, "removed moment", "removed moments")} waiting to be blanked…
                        </span>
                    )}
                </div>
            )}
            {range && meetingId && (
                <TimeRangeModal
                    meetingId={meetingId}
                    request={range}
                    onClose={() => setRange(null)}
                    onDelete={async (p) => {
                        if (await deleteTimeRange(p, range.startedAt)) {
                            range.onDeleted?.(p);
                            setRange(null);
                        }
                    }}
                    onStruck={(warnings) => {
                        setRange(null);
                        setMessage(
                            warnings.length
                                ? { text: `Stricken from the record. ${warnings.join(" ")}`, error: true }
                                : { text: "Stricken from the record.", error: false },
                        );
                        refreshVideo();
                        changedRef.current();
                    }}
                />
            )}
            {ranges && meetingId && (
                <RangesModal
                    meetingId={meetingId}
                    request={ranges}
                    onClose={() => setRanges(null)}
                    onDelete={async (p) => {
                        if (await deleteRanges(p, ranges.lineIds, ranges.startedAt)) {
                            ranges.onDeleted?.(p, ranges.lineIds);
                            setRanges(null);
                        }
                    }}
                    onStruck={(warnings) => {
                        setRanges(null);
                        setMessage(
                            warnings.length
                                ? { text: `Stricken from the record. ${warnings.join(" ")}`, error: true }
                                : { text: "Stricken from the record.", error: false },
                        );
                        ranges.onStruck?.();
                        refreshVideo();
                        changedRef.current();
                    }}
                />
            )}
            {failed.length > 0 && (
                <div className="rd-failed" role="alert">
                    <span>
                        {plural(failed.length, "deletion", "deletions")} couldn't be completed
                        <span className="rd-failed-reason"> — {failed[failed.length - 1].failure}</span>
                    </span>
                    <button className="rd-btn" onClick={retryFailed} disabled={retrying}>
                        {retrying ? "Retrying…" : "Retry"}
                    </button>
                    <button className="rd-btn rd-btn-ghost" onClick={cancelFailed} disabled={retrying} title="Keep the content">
                        Cancel delete
                    </button>
                </div>
            )}
            {strike && meetingId && (
                <StrikeConfirmModal
                    meetingId={meetingId}
                    request={strike}
                    onClose={() => setStrike(null)}
                    onDone={(warnings) => {
                        setStrike(null);
                        setMessage(
                            warnings.length
                                ? { text: `Stricken from the record. ${warnings.join(" ")}`, error: true }
                                : { text: "Stricken from the record.", error: false },
                        );
                        refreshVideo();
                        changedRef.current();
                    }}
                />
            )}
            {toast && (
                <div className="rd-toast" role="status" aria-live="polite">
                    <span>{toast.label}</span>
                    <button onClick={undo}>Undo</button>
                    <span className="rd-toast-count">{Math.max(0, Math.ceil((toast.until - now) / 1000))}s</span>
                    <button onClick={dismissToast} aria-label="Apply now" title="Apply now">
                        ✕
                    </button>
                </div>
            )}
            {!toast && message && (
                <div className={`rd-toast${message.error ? " rd-toast-error" : ""}`} role="status" aria-live="polite">
                    <span>{message.text}</span>
                    <button onClick={() => setMessage(null)} aria-label="Dismiss">
                        ✕
                    </button>
                </div>
            )}
        </>
    );

    return {
        ui,
        deleteWords,
        deleteScreens,
        deleteLines,
        deleteRanges,
        strikeWords: (segments: WordSegment[]) => segments.length && setStrike({ type: "words", segments }),
        strikeScreens: (ids: string[]) => ids.length && setStrike({ type: "screens", ids }),
        strikeLines: (ids: number[], words: number) => ids.length && setStrike({ type: "lines", ids, words }),
        /** Open the Delete / Strike a time range dialog */
        openTimeRange: (r: TimeRangeRequest) => setRange(r),
        /** Delete / Strike fixed ranges (a linked selection), with the preview */
        openRanges: (r: RangesRequest) => setRanges(r),
        /** A modal of this controller is open (keyboard shortcuts pause) */
        busy: !!strike || !!range || !!ranges,
    };
}

// ── Time range: Delete / Strike a block of meeting time ────────────────

function TimeRangeModal({
    meetingId,
    request,
    onClose,
    onDelete,
    onStruck,
}: {
    meetingId: string;
    request: TimeRangeRequest;
    onClose: () => void;
    onDelete: (p: TimeRangePreview) => Promise<void>;
    onStruck: (warnings: string[]) => void;
}) {
    const { startedAt } = request;
    const [startStr, setStartStr] = useState(clockAt(startedAt, request.startMs));
    const [endStr, setEndStr] = useState(clockAt(startedAt, request.endMs));
    const [preview, setPreview] = useState<TimeRangePreview | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [mode, setMode] = useState<"delete" | "strike">("delete");
    const [reason, setReason] = useState("");
    const [busy, setBusy] = useState(false);

    const startMs = parseClock(startedAt, startStr, request.startMs);
    const endMs = parseClock(startedAt, endStr, request.endMs);
    const invalid =
        startMs === null || endMs === null
            ? "Enter times as HH:MM or HH:MM:SS"
            : endMs <= startMs
              ? "The end must be after the start"
              : null;

    // Exact counts for the range as typed (refreshed as the times change)
    useEffect(() => {
        if (invalid || startMs === null || endMs === null) {
            setPreview(null);
            return;
        }
        let cancelled = false;
        setLoading(true);
        const t = setTimeout(async () => {
            try {
                const p = await redactionApi.previewTimeRange(meetingId, startMs, endMs);
                if (!cancelled) {
                    setPreview(p);
                    setError(null);
                }
            } catch (e) {
                if (!cancelled) {
                    setPreview(null);
                    setError(String(e));
                }
            } finally {
                if (!cancelled) setLoading(false);
            }
        }, 200);
        return () => {
            cancelled = true;
            clearTimeout(t);
        };
    }, [meetingId, startMs, endMs, invalid]);

    useEffect(() => {
        const key = (e: KeyboardEvent) => {
            if (e.key === "Escape" && !busy) onClose();
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [busy, onClose]);

    const ready = !!preview && !preview.nothing && !loading && !invalid;
    const span = preview ? spanLabel(startedAt, preview.start_ms, preview.end_ms) : "";

    const doDelete = async () => {
        if (!preview) return;
        setBusy(true);
        await onDelete(preview);
        setBusy(false);
    };
    const doStrike = async () => {
        if (!preview) return;
        setBusy(true);
        setError(null);
        try {
            const out = await redactionApi.strikeTimeRange(
                meetingId,
                preview.start_ms,
                preview.end_ms,
                reason.trim() || null,
                preview.counts,
            );
            onStruck(out.warnings);
        } catch (e) {
            setError(String(e));
            setBusy(false);
        }
    };
    const isNote = (s: string) =>
        s.startsWith("Files already exported") || s.startsWith("Time Machine") || s.startsWith("No meeting audio");

    return (
        <div className="rd-overlay" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
            <div className="rd-modal" role="dialog" aria-modal="true" aria-labelledby="rd-range-title">
                <h2 id="rd-range-title">
                    {mode === "strike" ? "Strike this time range from the record?" : "Delete a time range"}
                </h2>
                <p className="rd-modal-lead">
                    {mode === "strike"
                        ? 'Permanently destroys everything captured in this span and leaves a "Stricken from the record" marker with the time and your reason. It can\'t be undone.'
                        : "Removes everything captured in this span: screens, screen text, transcript lines and the screen video. You can undo for 5 seconds."}
                </p>
                <div className="rd-range-times">
                    <label className="rd-field">
                        From
                        <input
                            value={startStr}
                            onChange={(e) => setStartStr(e.target.value)}
                            placeholder="10:41:00"
                            aria-label="Start time"
                            disabled={busy}
                        />
                    </label>
                    <label className="rd-field">
                        To
                        <input
                            value={endStr}
                            onChange={(e) => setEndStr(e.target.value)}
                            placeholder="10:53:00"
                            aria-label="End time"
                            disabled={busy}
                        />
                    </label>
                    {startMs !== null && endMs !== null && !invalid && (
                        <span className="rd-hint rd-range-len">{durationLabel(endMs - startMs)}</span>
                    )}
                </div>
                {invalid && <p className="rd-error">{invalid}</p>}
                <h3>What will be {mode === "strike" ? "destroyed" : "removed"}</h3>
                {preview ? (
                    preview.nothing ? (
                        <p className="rd-hint">Nothing was captured in that span.</p>
                    ) : (
                        <ul className="rd-list">
                            {preview.items.map((it, i) => (
                                <li key={i} className={isNote(it) ? "rd-note" : undefined}>
                                    {it}
                                </li>
                            ))}
                        </ul>
                    )
                ) : (
                    !error && !invalid && <p className="rd-hint">Checking…</p>
                )}
                {mode === "strike" && (
                    <label className="rd-field">
                        Reason (optional)
                        <input
                            value={reason}
                            maxLength={120}
                            placeholder="e.g. privileged"
                            onChange={(e) => setReason(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && ready && !busy && doStrike()}
                            disabled={busy}
                            autoFocus
                        />
                        <span className="rd-hint">Shown on the marker. Don't include the words being removed.</span>
                    </label>
                )}
                {error && <p className="rd-error">{error}</p>}
                <div className="rd-modal-actions">
                    <button className="rd-btn" onClick={mode === "strike" ? () => setMode("delete") : onClose} disabled={busy}>
                        {mode === "strike" ? "Back" : "Cancel"}
                    </button>
                    {mode === "delete" ? (
                        <>
                            <button className="rd-btn" onClick={() => setMode("strike")} disabled={busy || !ready}>
                                Strike from the record…
                            </button>
                            <button className="rd-btn rd-btn-strike" onClick={doDelete} disabled={busy || !ready}>
                                {busy ? "Deleting…" : `Delete ${span}`}
                            </button>
                        </>
                    ) : (
                        <button className="rd-btn rd-btn-strike" onClick={doStrike} disabled={busy || !ready}>
                            {busy ? "Striking…" : `Strike ${span} from the record`}
                        </button>
                    )}
                </div>
            </div>
        </div>
    );
}

function StrikeConfirmModal({
    meetingId,
    request,
    onClose,
    onDone,
}: {
    meetingId: string;
    request: StrikeRequest;
    onClose: () => void;
    onDone: (warnings: string[]) => void;
}) {
    const [items, setItems] = useState<string[] | null>(null);
    const [reason, setReason] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const inputRef = useRef<HTMLInputElement>(null);

    useEffect(() => {
        let cancelled = false;
        (async () => {
            try {
                let list: string[];
                if (request.type === "words") {
                    const segs = request.segments;
                    const first = segs[0];
                    const preview = await redactionApi.previewWords(
                        meetingId,
                        first.transcriptId,
                        first.wholeLine ? undefined : first.start,
                        first.wholeLine ? undefined : first.end,
                    );
                    if (segs.length > 1) {
                        const n = segs.reduce((a, s) => a + s.wordCount, 0);
                        list = [`${plural(n, "word", "words")} from ${segs.length} transcript lines, and from search`, ...preview.slice(1)];
                    } else {
                        list = preview;
                    }
                } else if (request.type === "lines") {
                    // The shared lines (AI outputs, logs, backups…) from one
                    // line's preview; the first line counts the whole selection
                    const preview = await redactionApi.previewWords(meetingId, request.ids[0]);
                    list = [
                        `${plural(request.ids.length, "transcript line", "transcript lines")} (${plural(request.words, "word", "words")}), and from search`,
                        ...preview.slice(1),
                    ];
                } else {
                    list = await redactionApi.previewScreens(meetingId, request.ids);
                }
                if (!cancelled) setItems(list);
            } catch (e) {
                if (!cancelled) setError(String(e));
            }
        })();
        inputRef.current?.focus();
        return () => {
            cancelled = true;
        };
    }, [meetingId, request]);

    useEffect(() => {
        const key = (e: KeyboardEvent) => {
            if (e.key === "Escape" && !busy) onClose();
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [busy, onClose]);

    const confirm = async () => {
        setBusy(true);
        setError(null);
        const r = reason.trim() || null;
        const warnings: string[] = [];
        try {
            if (request.type === "words") {
                for (const s of request.segments) {
                    const out = s.wholeLine
                        ? await redactionApi.strikeLine(meetingId, s.transcriptId, r)
                        : await redactionApi.strikeWords(meetingId, s.transcriptId, s.start, s.end, r, s.lineText);
                    warnings.push(...out.warnings);
                }
            } else if (request.type === "lines") {
                let done = 0;
                for (const id of request.ids) {
                    try {
                        const out = await redactionApi.strikeLine(meetingId, id, r);
                        warnings.push(...out.warnings);
                        done++;
                    } catch (e) {
                        // Strikes can't be undone: report the ones that were
                        // made and refresh the view instead of hiding them
                        if (done === 0) throw e;
                        warnings.push(`Stopped after ${done} of ${request.ids.length} lines: ${e}`);
                        break;
                    }
                }
            } else {
                const out = await redactionApi.strikeScreens(meetingId, request.ids, r);
                warnings.push(...out.warnings);
            }
            onDone([...new Set(warnings)]);
        } catch (e) {
            setError(String(e));
            setBusy(false);
        }
    };

    // The last notices are fixed one-liners; render them de-emphasized
    const isNote = (s: string) =>
        s.startsWith("Files already exported") || s.startsWith("Time Machine") || s.startsWith("No meeting audio");

    return (
        <div className="rd-overlay" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
            <div className="rd-modal" role="dialog" aria-modal="true" aria-labelledby="rd-strike-title">
                <h2 id="rd-strike-title">Strike from the record?</h2>
                <p className="rd-modal-lead">
                    This permanently destroys the selection everywhere the app keeps it and leaves a
                    "Stricken from the record" marker with the time and your reason. It can't be undone.
                </p>
                {request.type === "words" && (
                    <div className="rd-quote" aria-label="Selected words">
                        {request.segments.map((s) => s.words).join(" … ")}
                    </div>
                )}
                <h3>What will be destroyed</h3>
                {items ? (
                    <ul className="rd-list">
                        {items.map((it, i) => (
                            <li key={i} className={isNote(it) ? "rd-note" : undefined}>
                                {it}
                            </li>
                        ))}
                    </ul>
                ) : (
                    !error && <p className="rd-hint">Checking…</p>
                )}
                <label className="rd-field">
                    Reason (optional)
                    <input
                        ref={inputRef}
                        value={reason}
                        maxLength={120}
                        placeholder="e.g. privileged"
                        onChange={(e) => setReason(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && items && !busy && confirm()}
                        disabled={busy}
                    />
                    <span className="rd-hint">Shown on the marker. Don't include the words being removed.</span>
                </label>
                {error && <p className="rd-error">{error}</p>}
                <div className="rd-modal-actions">
                    <button className="rd-btn" onClick={onClose} disabled={busy}>
                        Cancel
                    </button>
                    <button className="rd-btn rd-btn-strike" onClick={confirm} disabled={busy || !items}>
                        {busy ? "Striking…" : "Strike from the record"}
                    </button>
                </div>
            </div>
        </div>
    );
}

// ── Fixed ranges: a linked selection of screens and transcript lines ───

function RangesModal({
    meetingId,
    request,
    onClose,
    onDelete,
    onStruck,
}: {
    meetingId: string;
    request: RangesRequest;
    onClose: () => void;
    onDelete: (p: TimeRangePreview | null) => Promise<void>;
    onStruck: (warnings: string[]) => void;
}) {
    const { startedAt, ranges, lineIds } = request;
    const [preview, setPreview] = useState<TimeRangePreview | null>(null);
    const [loading, setLoading] = useState(ranges.length > 0);
    const [error, setError] = useState<string | null>(null);
    const [mode, setMode] = useState(request.mode);
    const [reason, setReason] = useState("");
    const [busy, setBusy] = useState(false);

    // Exact totals per kind for all the ranges together
    useEffect(() => {
        if (ranges.length === 0) return;
        let cancelled = false;
        (async () => {
            try {
                const p = await redactionApi.previewTimeRanges(meetingId, ranges);
                if (!cancelled) setPreview(p);
            } catch (e) {
                if (!cancelled) setError(String(e));
            } finally {
                if (!cancelled) setLoading(false);
            }
        })();
        return () => {
            cancelled = true;
        };
    }, [meetingId, ranges]);

    useEffect(() => {
        const key = (e: KeyboardEvent) => {
            if (e.key === "Escape" && !busy) onClose();
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [busy, onClose]);

    const shown = preview?.ranges ?? ranges;
    const hasRanges = !!preview && !preview.nothing;
    const ready = !loading && !error && (hasRanges || lineIds.length > 0);
    const isNote = (s: string) =>
        s.startsWith("Files already exported") || s.startsWith("Time Machine") || s.startsWith("No meeting audio");

    const doDelete = async () => {
        setBusy(true);
        await onDelete(hasRanges ? preview : null);
        setBusy(false);
    };
    const doStrike = async () => {
        setBusy(true);
        setError(null);
        const r = reason.trim() || null;
        const warnings: string[] = [];
        let started = false;
        try {
            if (hasRanges && preview) {
                const out = await redactionApi.strikeTimeRanges(meetingId, preview.ranges, r, preview.counts);
                warnings.push(...out.warnings);
                started = true;
            }
            for (const id of lineIds) {
                const out = await redactionApi.strikeLine(meetingId, id, r);
                warnings.push(...out.warnings);
                started = true;
            }
            onStruck([...new Set(warnings)]);
        } catch (e) {
            // Whatever was already stricken stays stricken (no undo): say so
            if (started) onStruck([...new Set([...warnings, `Not everything was stricken: ${e}`])]);
            else {
                setError(String(e));
                setBusy(false);
            }
        }
    };

    const spans = shown.map((r) => spanLabel(startedAt, r.start_ms, r.end_ms)).filter(Boolean);
    return (
        <div className="rd-overlay" onMouseDown={(e) => e.target === e.currentTarget && !busy && onClose()}>
            <div className="rd-modal" role="dialog" aria-modal="true" aria-labelledby="rd-ranges-title">
                <h2 id="rd-ranges-title">
                    {mode === "strike" ? "Strike the selection from the record?" : "Delete the selection?"}
                </h2>
                <p className="rd-modal-lead">
                    {mode === "strike"
                        ? 'Permanently destroys everything captured in the selected time (screens, screen text, transcript and screen video) and leaves a "Stricken from the record" marker for each span, with its time and your reason. It can\'t be undone.'
                        : "Removes everything captured in the selected time: screens, screen text, transcript lines and the screen video. You can undo for 5 seconds."}
                </p>
                {request.note && <p className="rd-hint">{request.note}</p>}
                {spans.length > 0 && (
                    <p className="rd-hint rd-range-spans">
                        {spans.length === 1 ? "Span" : `${spans.length} spans`}: {spans.slice(0, 6).join(", ")}
                        {spans.length > 6 ? `, and ${spans.length - 6} more` : ""}
                    </p>
                )}
                <h3>What will be {mode === "strike" ? "destroyed" : "removed"}</h3>
                {loading ? (
                    !error && <p className="rd-hint">Checking…</p>
                ) : (
                    <ul className="rd-list">
                        {lineIds.length > 0 && (
                            <li>
                                {plural(lineIds.length, "transcript line", "transcript lines")} with no recorded time,
                                removed whole
                            </li>
                        )}
                        {preview && preview.nothing && lineIds.length === 0 && (
                            <li className="rd-note">Nothing was captured in that time.</li>
                        )}
                        {hasRanges &&
                            preview?.items.map((it, i) => (
                                <li key={i} className={isNote(it) ? "rd-note" : undefined}>
                                    {it}
                                </li>
                            ))}
                    </ul>
                )}
                {mode === "strike" && (
                    <label className="rd-field">
                        Reason (optional)
                        <input
                            value={reason}
                            maxLength={120}
                            placeholder="e.g. privileged"
                            onChange={(e) => setReason(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && ready && !busy && doStrike()}
                            disabled={busy}
                            autoFocus
                        />
                        <span className="rd-hint">Shown on the markers. Don't include the words being removed.</span>
                    </label>
                )}
                {error && <p className="rd-error">{error}</p>}
                <div className="rd-modal-actions">
                    <button className="rd-btn" onClick={onClose} disabled={busy}>
                        Cancel
                    </button>
                    {mode === "delete" ? (
                        <>
                            <button className="rd-btn" onClick={() => setMode("strike")} disabled={busy || !ready}>
                                Strike from the record…
                            </button>
                            <button className="rd-btn rd-btn-strike" onClick={doDelete} disabled={busy || !ready}>
                                {busy ? "Deleting…" : "Delete"}
                            </button>
                        </>
                    ) : (
                        <button className="rd-btn rd-btn-strike" onClick={doStrike} disabled={busy || !ready}>
                            {busy ? "Striking…" : "Strike from the record"}
                        </button>
                    )}
                </div>
            </div>
        </div>
    );
}
