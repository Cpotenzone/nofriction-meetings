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
    type RedactionRecord,
} from "../../lib/redaction";
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

    return { range, begin, extend, clear, selectLine, wordRangeForLine };
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
    | { type: "screens"; ids: string[] };

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
    const changedRef = useRef(onChanged);
    changedRef.current = onChanged;

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
                changedRef.current();
            });
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

    const cancelFailed = useCallback(async () => {
        await Promise.allSettled(failed.map((f) => redactionApi.undo(f.id)));
        refreshFailed();
        changedRef.current();
    }, [failed, refreshFailed]);

    const ui = (
        <>
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
        strikeWords: (segments: WordSegment[]) => segments.length && setStrike({ type: "words", segments }),
        strikeScreens: (ids: string[]) => ids.length && setStrike({ type: "screens", ids }),
    };
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
            } else {
                const out = await redactionApi.strikeScreens(meetingId, request.ids, r);
                warnings.push(...out.warnings);
            }
            onDone(warnings);
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
