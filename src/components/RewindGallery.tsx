import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import * as tauri from "../lib/tauri";
import { friendlyAiError, isNoProviderError } from "../lib/ai";
import { AiSetupNotice } from "./AiSetupNotice";
import { applyLocalDelete, lineWords, redactionApi, renderPlain, type RedactionRecord, type TimeRangePreview } from "../lib/redaction";
import {
    EMPTY_SELECTION,
    clickScreen,
    clockAt,
    orderedIds,
    pruneSelection,
    rangeOfScreens,
    selectAll,
    selectLastMinutes,
    selectToEnd,
    spanLabel,
    summarize,
    type Selection,
} from "../lib/screenSelection";
import {
    RedactableLine,
    RedactionActionBar,
    ScreenStrickenCard,
    selectionSegments,
    useRedaction,
    useWordSelection,
} from "./redaction/Redaction";
import './RewindTab.css';

interface RewindGalleryProps {
    meetingId: string | null;
    isRecording: boolean;
}

type StripItem =
    | { kind: "frame"; ms: number; frame: tauri.TimelineFrame }
    | { kind: "stricken"; ms: number; record: RedactionRecord };

export function RewindGallery({ meetingId, isRecording }: RewindGalleryProps) {
    const [timeline, setTimeline] = useState<tauri.SyncedTimeline | null>(null);
    const [currentTime, setCurrentTime] = useState(0);
    const [selectedFrame, setSelectedFrame] = useState<tauri.TimelineFrame | null>(null);
    const [frameImage, setFrameImage] = useState<string | null>(null);
    const [thumbnails, setThumbnails] = useState<Map<string, string>>(new Map());
    const [isLoading, setIsLoading] = useState(false);
    const [reloadKey, setReloadKey] = useState(0);
    // Screen multi-select: ⌘-click toggles, Shift-click selects a range,
    // ⌘A selects all (lib/screenSelection.ts), or "Select screens" mode
    const [sel, setSel] = useState<Selection>(EMPTY_SELECTION);
    const [selectMode, setSelectMode] = useState(false);
    // "Last N minutes": the span itself, for "Delete time range"
    const [lastMinutes, setLastMinutes] = useState(12);
    const [rangeHint, setRangeHint] = useState<{ range: [number, number]; minutes: number } | null>(null);
    const [notesStale, setNotesStale] = useState(false);
    const [regenerating, setRegenerating] = useState(false);
    const [regenError, setRegenError] = useState<string | null>(null);
    const [needsAi, setNeedsAi] = useState(false);

    const galleryRef = useRef<HTMLDivElement>(null);
    const transcriptRef = useRef<HTMLDivElement>(null);

    const words = useWordSelection();
    const clearWords = words.clear;
    const reload = useCallback(() => setReloadKey((k) => k + 1), []);
    const redaction = useRedaction(meetingId, reload);

    // Load timeline data
    useEffect(() => {
        // Reset state immediately when meeting changes
        setTimeline(null);
        setSelectedFrame(null);
        setFrameImage(null);
        setThumbnails(new Map());
        setSel(EMPTY_SELECTION);
        setSelectMode(false);
        setRangeHint(null);
        clearWords();
    }, [meetingId, clearWords]);

    useEffect(() => {
        if (!meetingId) {
            return;
        }

        const loadTimeline = async () => {
            setIsLoading(true);
            try {
                const data = await tauri.getSyncedTimeline(meetingId);
                setTimeline(data);

                // Keep the selected frame if it still exists, else the first
                setSelectedFrame((prev) => {
                    const keep = prev && data.frames.find((f) => f.id === prev.id);
                    if (keep) return keep;
                    if (data.frames.length > 0) {
                        setCurrentTime(data.frames[0].timestamp_ms);
                        return data.frames[0];
                    }
                    return null;
                });
                setSel((prev) => pruneSelection(data.frames, prev));
            } catch (err) {
                console.error("Failed to load timeline:", err);
            } finally {
                setIsLoading(false);
            }
            redactionApi
                .aiStatus(meetingId)
                .then((s) => setNotesStale(s.notes_stale))
                .catch(() => setNotesStale(false));
        };

        loadTimeline();

        // Auto-refresh while recording
        let interval: ReturnType<typeof setInterval> | null = null;
        if (isRecording) {
            interval = setInterval(loadTimeline, 3000);
        }

        return () => {
            if (interval) clearInterval(interval);
        };
    }, [meetingId, isRecording, reloadKey]);

    // Edits change line text: drop any word selection on reload
    useEffect(() => {
        clearWords();
    }, [reloadKey, clearWords]);

    // Load selected frame image
    useEffect(() => {
        if (!selectedFrame) {
            setFrameImage(null);
            return;
        }

        const loadFrame = async () => {
            try {
                const base64 = await tauri.getFrameThumbnail(selectedFrame.id, false);
                setFrameImage(base64 ? `data:image/jpeg;base64,${base64}` : null);
            } catch (err) {
                console.error("Failed to load frame:", err);
            }
        };

        loadFrame();
    }, [selectedFrame]);

    // Load thumbnails progressively
    useEffect(() => {
        if (!timeline) return;

        const loadThumbnails = async () => {
            for (const frame of timeline.frames) {
                if (!thumbnails.has(frame.id)) {
                    try {
                        const base64 = await tauri.getFrameThumbnail(frame.id, true);
                        if (base64) {
                            setThumbnails((prev) => new Map(prev).set(frame.id, `data:image/jpeg;base64,${base64}`));
                        }
                    } catch (err) {
                        console.error(`Failed to load thumbnail for frame ${frame.id}:`, err);
                    }
                }
            }
        };

        loadThumbnails();
    }, [timeline]);

    // Handle timeline scrubbing
    const handleScrub = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        const time = parseInt(e.target.value, 10);
        setCurrentTime(time);

        // Find nearest frame
        if (timeline && timeline.frames.length > 0) {
            const nearestFrame = timeline.frames.reduce((prev: tauri.TimelineFrame, curr: tauri.TimelineFrame) =>
                Math.abs(curr.timestamp_ms - time) < Math.abs(prev.timestamp_ms - time) ? curr : prev
            );
            setSelectedFrame(nearestFrame);
        }
    }, [timeline]);

    // Find the nearest transcript for the current time
    const currentTranscript = timeline?.transcripts.reduce((best: tauri.TimelineTranscript | null, t: tauri.TimelineTranscript) => {
        if (t.timestamp_ms > currentTime) return best;
        if (!best) return t;
        return Math.abs(t.timestamp_ms - currentTime) < Math.abs(best.timestamp_ms - currentTime) ? t : best;
    }, null as tauri.TimelineTranscript | null);

    // Auto-scroll transcript panel to the active entry when currentTime changes
    useEffect(() => {
        if (!currentTranscript || !transcriptRef.current) return;
        const el = transcriptRef.current.querySelector(`[data-transcript-id="${currentTranscript.id}"]`);
        if (el) {
            el.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        }
    }, [currentTranscript?.id]);

    // Auto-scroll thumbnail gallery to the selected frame
    useEffect(() => {
        if (!selectedFrame || !galleryRef.current) return;
        const el = galleryRef.current.querySelector(`[data-frame-id="${selectedFrame.id}"]`);
        if (el) {
            el.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' });
        }
    }, [selectedFrame?.id]);

    // ── Editing ──────────────────────────────────────────────────────────
    const editable = !isRecording;
    const records = useMemo(
        () => new Map((timeline?.redactions ?? []).map((r) => [r.id, r] as const)),
        [timeline?.redactions],
    );
    const lines = useMemo(
        () => (timeline?.transcripts ?? []).map((t) => ({ id: Number(t.id), text: t.text })),
        [timeline?.transcripts],
    );
    const segments = useMemo(
        () => (words.range ? selectionSegments(lines, words.wordRangeForLine) : []),
        [lines, words.range, words.wordRangeForLine],
    );
    const selectedWordCount = segments.reduce((a, s) => a + s.wordCount, 0);

    const deleteSelectedWords = async () => {
        const segs = segments;
        if (await redaction.deleteWords(segs)) {
            // Hide during the undo window; the backend applies the edit after
            setTimeline((tl) => {
                if (!tl) return tl;
                const byId = new Map(segs.map((s) => [String(s.transcriptId), s] as const));
                return {
                    ...tl,
                    transcripts: tl.transcripts
                        .map((t) => {
                            const s = byId.get(t.id);
                            return s ? { ...t, text: applyLocalDelete(t.text, s.start, s.end) } : t;
                        })
                        .filter((t) => t.text.trim() !== ""),
                };
            });
            words.clear();
        }
    };

    // What is selected, in timeline order. The selection bar's count and
    // the ids sent to the backend are both exactly this list.
    const frames = useMemo(() => timeline?.frames ?? [], [timeline?.frames]);
    const selectedIds = useMemo(() => orderedIds(frames, sel), [frames, sel]);
    const summary = useMemo(() => summarize(frames, sel), [frames, sel]);

    const viewFrame = (frame: tauri.TimelineFrame) => {
        setSelectedFrame(frame);
        setCurrentTime(frame.timestamp_ms);
    };

    const onThumbClick = (e: React.MouseEvent, frame: tauri.TimelineFrame) => {
        // Keyboard shortcuts (⌘A, Delete) act on the grid once it's used
        galleryRef.current?.focus({ preventScroll: true });
        if (!editable) {
            viewFrame(frame);
            return;
        }
        const r = clickScreen(
            sel,
            frames,
            frame.id,
            { shift: e.shiftKey, toggle: e.metaKey || e.ctrlKey, selectMode },
            selectedFrame?.id ?? null,
        );
        setSel(r.sel);
        if (!r.view) setRangeHint(null);
        if (r.view) viewFrame(frame);
    };

    const clearSelection = () => {
        setSel(EMPTY_SELECTION);
        setSelectMode(false);
        setRangeHint(null);
    };

    const deleteSelectedScreens = async () => {
        const ids = selectedIds;
        if (ids.length === 0) return;
        if (await redaction.deleteScreens(ids)) {
            const gone = new Set(ids);
            setTimeline((tl) => (tl ? { ...tl, frames: tl.frames.filter((f) => !gone.has(f.id)) } : tl));
            setSelectedFrame((f) => (f && gone.has(f.id) ? null : f));
            clearSelection();
        }
    };

    // Hide what a time-range Delete removes during its undo window
    const hideRange = (p: TimeRangePreview) => {
        const screens = new Set(p.screen_ids);
        const lines = new Map(p.lines.map((l) => [String(l.transcript_id), l] as const));
        setTimeline((tl) =>
            tl
                ? {
                      ...tl,
                      frames: tl.frames.filter((f) => !screens.has(f.id)),
                      transcripts: tl.transcripts
                          .map((t) => {
                              const l = lines.get(t.id);
                              return l ? { ...t, text: applyLocalDelete(t.text, l.start, l.end) } : t;
                          })
                          .filter((t) => t.text.trim() !== ""),
                  }
                : tl,
        );
        setSelectedFrame((f) => (f && screens.has(f.id) ? null : f));
        words.clear();
        clearSelection();
    };

    const regenerateNotes = async () => {
        if (!meetingId) return;
        setRegenerating(true);
        setRegenError(null);
        setNeedsAi(false);
        try {
            // Same prompt as Recordings → Notes (persona's meeting_report)
            await tauri.generateMeetingReport(meetingId);
            setNotesStale(false);
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setRegenError(friendlyAiError(e));
        } finally {
            setRegenerating(false);
        }
    };

    // Format time as MM:SS
    const formatTime = (ms: number) => {
        const seconds = Math.floor(ms / 1000);
        const mins = Math.floor(seconds / 60);
        const secs = seconds % 60;
        return `${mins}:${secs.toString().padStart(2, "0")}`;
    };

    // Thumbnails plus hatched cards where screens were stricken
    const strip: StripItem[] = useMemo(() => {
        const items: StripItem[] = (timeline?.frames ?? []).map((frame) => ({ kind: "frame", ms: frame.timestamp_ms, frame }));
        const start = timeline?.started_at ? Date.parse(timeline.started_at) : NaN;
        for (const r of timeline?.redactions ?? []) {
            if (r.kind !== "screen") continue;
            const at = r.media_start ? Date.parse(r.media_start) : NaN;
            items.push({ kind: "stricken", ms: isNaN(at) || isNaN(start) ? Number.MAX_SAFE_INTEGER : Math.max(0, at - start), record: r });
        }
        return items.sort((a, b) => a.ms - b.ms);
    }, [timeline?.frames, timeline?.redactions, timeline?.started_at]);

    // Calculate max time
    const maxTime = timeline ? Math.max(
        ...timeline.frames.map((f: tauri.TimelineFrame) => f.timestamp_ms),
        ...timeline.transcripts.map((t: tauri.TimelineTranscript) => t.timestamp_ms + t.duration_seconds * 1000),
        1000
    ) : 1000;
    const meetingEndMs = Math.max(maxTime, (timeline?.duration_seconds ?? 0) * 1000);
    const startedAt = timeline?.started_at ?? "";

    const openTimeRange = () => {
        if (!startedAt) return;
        const r =
            rangeHint?.range ??
            (selectedIds.length ? rangeOfScreens(frames, selectedIds, meetingEndMs) : null) ??
            [currentTime, meetingEndMs];
        redaction.openTimeRange({
            startMs: r[0],
            endMs: r[1] > r[0] ? r[1] : r[0] + 60_000,
            startedAt,
            maxMs: meetingEndMs,
            onDeleted: hideRange,
        });
    };

    const selectLast = () => {
        const { sel: s, range } = selectLastMinutes(frames, meetingEndMs, lastMinutes);
        setSel(s);
        setRangeHint({ range, minutes: lastMinutes });
    };

    const onGridKey = (e: React.KeyboardEvent) => {
        if (!editable || redaction.busy) return;
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
            e.preventDefault();
            setSel(selectAll(frames));
            setRangeHint(null);
        } else if ((e.key === "Delete" || e.key === "Backspace") && selectedIds.length > 0) {
            e.preventDefault();
            deleteSelectedScreens();
        } else if (e.key === "Escape" && selectedIds.length > 0) {
            clearSelection();
        }
    };

    // "5 screens · 10:41–10:53" (exactly the screens that will be sent)
    const selectionLabel = (() => {
        if (!summary) return "";
        const n = `${summary.count} ${summary.count === 1 ? "screen" : "screens"}`;
        if (!startedAt) return n;
        const when =
            rangeHint
                ? `last ${rangeHint.minutes} min (${spanLabel(startedAt, rangeHint.range[0], rangeHint.range[1])})`
                : summary.firstMs === summary.lastMs
                  ? clockAt(startedAt, summary.firstMs)
                  : spanLabel(startedAt, summary.firstMs, summary.lastMs);
        return `${n} · ${when}${summary.groups > 1 && !rangeHint ? ` · ${summary.groups} separate groups` : ""}`;
    })();

    if (!meetingId) {
        return (
            <div className="rewind-empty">
                <div className="empty-state">
                    <div className="empty-state-icon">🎬</div>
                    <p className="empty-state-text">Select a past meeting to review</p>
                </div>
            </div>
        );
    }

    if (isLoading && !timeline) {
        return (
            <div className="rewind-loading">
                <div className="loading-spinner"></div>
                <p>Loading meeting timeline...</p>
            </div>
        );
    }

    return (
        <div className="rewind-gallery">
            {/* Top section: Frame preview + Transcripts */}
            <div className="rewind-main">
                {/* Frame preview */}
                <div className="rewind-frame-preview">
                    {frameImage ? (
                        <img src={frameImage} alt="Frame preview" />
                    ) : (
                        <div className="frame-placeholder">
                            <span>📷</span>
                            <p>No frame selected</p>
                        </div>
                    )}
                    <div className="frame-timestamp">
                        {selectedFrame && formatTime(selectedFrame.timestamp_ms)}
                    </div>
                </div>

                {/* Transcript panel */}
                <div className="rewind-transcripts scrollable" ref={transcriptRef}>
                    <h3>Transcripts</h3>
                    {notesStale && (
                        <div className="rd-stale" role="status">
                            <span>AI notes were made before an edit.</span>
                            <button className="rd-btn" onClick={regenerateNotes} disabled={regenerating}>
                                {regenerating ? "Regenerating…" : "Regenerate"}
                            </button>
                        </div>
                    )}
                    {needsAi && <AiSetupNotice feature="AI notes" compact />}
                    {regenError && <p className="rd-tip" role="alert">Couldn't regenerate the notes: {regenError}</p>}
                    {editable && (timeline?.transcripts.length ?? 0) > 0 && !words.range && (
                        <p className="rd-tip">Click a word to edit. Shift-click or drag to select more.</p>
                    )}
                    {timeline?.transcripts.length === 0 ? (
                        <p className="no-transcripts">No transcripts yet</p>
                    ) : (
                        <div className="transcript-entries">
                            {timeline?.transcripts.map((t: tauri.TimelineTranscript, li: number) => (
                                <div
                                    key={t.id}
                                    data-transcript-id={t.id}
                                    className={`transcript-entry ${t.id === currentTranscript?.id ? "active" : ""}`}
                                    onClick={() => {
                                        setCurrentTime(t.timestamp_ms);
                                        // Find frame at this time
                                        if (timeline.frames.length > 0) {
                                            const nearestFrame = timeline.frames.reduce((prev: tauri.TimelineFrame, curr: tauri.TimelineFrame) =>
                                                Math.abs(curr.timestamp_ms - t.timestamp_ms) < Math.abs(prev.timestamp_ms - t.timestamp_ms) ? curr : prev
                                            );
                                            setSelectedFrame(nearestFrame);
                                        }
                                    }}
                                >
                                    <span className="entry-time">{formatTime(t.timestamp_ms)}</span>
                                    <span
                                        className="entry-speaker"
                                        title={editable ? "Double-click to select the whole line" : undefined}
                                        onDoubleClick={() => editable && words.selectLine(li, lineWords(t.text).length)}
                                    >
                                        {t.speaker || "Speaker"}
                                    </span>
                                    <p className="entry-text">
                                        <RedactableLine
                                            text={t.text}
                                            li={li}
                                            selected={words.wordRangeForLine(li, lineWords(t.text).length)}
                                            records={records}
                                            editable={editable}
                                            onWordDown={words.begin}
                                            onWordEnter={words.extend}
                                        />
                                    </p>
                                </div>
                            ))}
                        </div>
                    )}
                    {editable && segments.length > 0 && (
                        <RedactionActionBar
                            label={`${selectedWordCount} ${selectedWordCount === 1 ? "word" : "words"}${segments.length > 1 ? ` · ${segments.length} lines` : ""}`}
                            extra={
                                segments.length === 1 && !segments[0].wholeLine ? (
                                    <button
                                        className="rd-btn rd-btn-ghost"
                                        onClick={() => {
                                            const li = lines.findIndex((l) => l.id === segments[0].transcriptId);
                                            if (li >= 0) words.selectLine(li, lineWords(lines[li].text).length);
                                        }}
                                    >
                                        Whole line
                                    </button>
                                ) : undefined
                            }
                            onDelete={deleteSelectedWords}
                            onStrike={() => redaction.strikeWords(segments)}
                            onClear={words.clear}
                        />
                    )}
                </div>
            </div>

            {/* Timeline scrubber */}
            <div className="rewind-timeline">
                <span className="timeline-time">{formatTime(0)}</span>
                <div className="timeline-track">
                    <input
                        type="range"
                        min={0}
                        max={maxTime}
                        value={currentTime}
                        onChange={handleScrub}
                        className="timeline-slider"
                    />
                    {/* Frame markers (selected screens in yellow, so a pick
                        scrolled out of the strip is still visible) */}
                    <div className="timeline-markers">
                        {timeline?.frames.map((f: tauri.TimelineFrame) => (
                            <div
                                key={f.id}
                                className={`timeline-marker frame-marker${sel.ids.has(f.id) ? " rd-marker-selected" : ""}`}
                                style={{ left: `${(f.timestamp_ms / maxTime) * 100}%` }}
                                title={`Frame at ${formatTime(f.timestamp_ms)}${sel.ids.has(f.id) ? " (selected)" : ""}`}
                            />
                        ))}
                        {/* Transcript markers */}
                        {timeline?.transcripts.filter((t: tauri.TimelineTranscript) => t.is_final).map((t: tauri.TimelineTranscript) => (
                            <div
                                key={t.id}
                                className="timeline-marker transcript-marker"
                                style={{ left: `${(t.timestamp_ms / maxTime) * 100}%` }}
                                title={renderPlain(t.text).slice(0, 50)}
                            />
                        ))}
                    </div>
                </div>
                <span className="timeline-time">{formatTime(maxTime)}</span>
            </div>

            {/* Thumbnail gallery (focusable: ⌘A selects all, Delete deletes) */}
            <div
                className="rewind-thumbnails scrollable"
                ref={galleryRef}
                tabIndex={0}
                onKeyDown={onGridKey}
                aria-label="Screens. Click to view, ⌘-click to pick, Shift-click to pick a range, ⌘A to pick all, Delete to delete"
                aria-multiselectable={editable}
                role="listbox"
            >
                {strip.map((item) =>
                    item.kind === "stricken" ? (
                        <ScreenStrickenCard key={`s-${item.record.id}`} record={item.record} className="thumbnail" />
                    ) : (
                        <div
                            key={item.frame.id}
                            data-frame-id={item.frame.id}
                            role="option"
                            className={`thumbnail ${item.frame.id === selectedFrame?.id ? "selected" : ""} ${sel.ids.has(item.frame.id) ? "rd-selected" : ""}`}
                            aria-selected={sel.ids.has(item.frame.id)}
                            onMouseDown={(e) => {
                                // Shift-click selects screens, not page text
                                if (e.shiftKey) e.preventDefault();
                            }}
                            onClick={(e) => onThumbClick(e, item.frame)}
                        >
                            {thumbnails.has(item.frame.id) ? (
                                <img src={thumbnails.get(item.frame.id)} alt={`Frame ${item.frame.frame_number}`} />
                            ) : (
                                <div className="thumbnail-loading">
                                    <span>🖼️</span>
                                </div>
                            )}
                            {sel.ids.has(item.frame.id) && <span className="rd-check">✓</span>}
                            <span className="thumbnail-time">{formatTime(item.frame.timestamp_ms)}</span>
                        </div>
                    )
                )}
            </div>

            {editable && summary && (
                <div style={{ padding: "0 12px 8px" }}>
                    <RedactionActionBar
                        label={selectionLabel}
                        extra={
                            <button
                                className="rd-btn rd-btn-ghost"
                                onClick={openTimeRange}
                                disabled={!startedAt}
                                title="Delete or strike everything in this span: screens, screen text, transcript and screen video"
                            >
                                Time range…
                            </button>
                        }
                        onDelete={deleteSelectedScreens}
                        onStrike={() => redaction.strikeScreens(selectedIds)}
                        onClear={clearSelection}
                    />
                </div>
            )}

            {/* Stats bar */}
            <div className="rewind-stats-bar">
                <span>📷 {timeline?.frames.length || 0} frames</span>
                <span>💬 {timeline?.transcripts.length || 0} transcripts</span>
                <span>⏱️ {formatTime(maxTime)} duration</span>
                {editable && (timeline?.frames.length ?? 0) > 0 && (
                    <span className="rd-selectbar" style={{ marginLeft: "auto" }}>
                        <button
                            className="rd-btn rd-btn-ghost"
                            style={{ padding: "0 6px" }}
                            onClick={() => {
                                if (selectMode) clearSelection();
                                else setSelectMode(true);
                            }}
                            title="Or ⌘-click thumbnails; Shift-click picks a range"
                        >
                            {selectMode ? "Done selecting" : "Select screens"}
                        </button>
                        <button
                            className="rd-btn rd-btn-ghost"
                            style={{ padding: "0 6px" }}
                            onClick={() => {
                                setSel(selectAll(frames));
                                setRangeHint(null);
                            }}
                            title="⌘A in the screen strip"
                        >
                            Select all
                        </button>
                        <button
                            className="rd-btn rd-btn-ghost"
                            style={{ padding: "0 6px" }}
                            onClick={() => {
                                setSel(selectToEnd(frames, selectedFrame?.id ?? sel.anchor));
                                setRangeHint(null);
                            }}
                            disabled={!selectedFrame && !sel.anchor}
                            title="From the screen you're viewing to the end of the meeting"
                        >
                            From here to the end
                        </button>
                        <label className="rd-hint" style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                            Last
                            <input
                                type="number"
                                min={1}
                                max={600}
                                value={lastMinutes}
                                onChange={(e) => setLastMinutes(Math.max(1, Math.min(600, Number(e.target.value) || 1)))}
                                aria-label="Minutes"
                            />
                            min
                        </label>
                        <button className="rd-btn rd-btn-ghost" style={{ padding: "0 6px" }} onClick={selectLast}>
                            Select
                        </button>
                        <button
                            className="rd-btn rd-btn-ghost"
                            style={{ padding: "0 6px" }}
                            onClick={openTimeRange}
                            disabled={!startedAt}
                            title="Delete or strike a block of time (picks its start and end)"
                        >
                            Time range…
                        </button>
                    </span>
                )}
            </div>

            {redaction.ui}
        </div>
    );
}
