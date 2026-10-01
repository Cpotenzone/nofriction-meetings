import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import * as tauri from "../lib/tauri";
import { withAiConsent, friendlyAiError } from "../lib/ai";
import { applyLocalDelete, lineWords, redactionApi, renderPlain, type RedactionRecord } from "../lib/redaction";
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
    // Screen multi-select (⌘/Shift-click, or "Select screens" mode)
    const [selectedScreens, setSelectedScreens] = useState<Set<string>>(new Set());
    const [selectMode, setSelectMode] = useState(false);
    const [notesStale, setNotesStale] = useState(false);
    const [regenerating, setRegenerating] = useState(false);

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
        setSelectedScreens(new Set());
        setSelectMode(false);
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
                setSelectedScreens((prev) => new Set([...prev].filter((id) => data.frames.some((f) => f.id === id))));
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

    const toggleScreen = (id: string) =>
        setSelectedScreens((prev) => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
        });

    const deleteSelectedScreens = async () => {
        const ids = [...selectedScreens];
        if (await redaction.deleteScreens(ids)) {
            setTimeline((tl) => (tl ? { ...tl, frames: tl.frames.filter((f) => !selectedScreens.has(f.id)) } : tl));
            setSelectedFrame((f) => (f && selectedScreens.has(f.id) ? null : f));
            setSelectedScreens(new Set());
        }
    };

    const regenerateNotes = async () => {
        if (!meetingId) return;
        setRegenerating(true);
        try {
            await withAiConsent(() => invoke("generate_meeting_notes", { meetingId }));
            setNotesStale(false);
        } catch (e) {
            console.error("Failed to regenerate notes:", friendlyAiError(e));
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
                    {/* Frame markers */}
                    <div className="timeline-markers">
                        {timeline?.frames.map((f: tauri.TimelineFrame) => (
                            <div
                                key={f.id}
                                className="timeline-marker frame-marker"
                                style={{ left: `${(f.timestamp_ms / maxTime) * 100}%` }}
                                title={`Frame at ${formatTime(f.timestamp_ms)}`}
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

            {/* Thumbnail gallery */}
            <div className="rewind-thumbnails scrollable" ref={galleryRef}>
                {strip.map((item) =>
                    item.kind === "stricken" ? (
                        <ScreenStrickenCard key={`s-${item.record.id}`} record={item.record} className="thumbnail" />
                    ) : (
                        <div
                            key={item.frame.id}
                            data-frame-id={item.frame.id}
                            className={`thumbnail ${item.frame.id === selectedFrame?.id ? "selected" : ""} ${selectedScreens.has(item.frame.id) ? "rd-selected" : ""}`}
                            aria-pressed={selectedScreens.has(item.frame.id)}
                            onClick={(e) => {
                                if (editable && (selectMode || e.metaKey || e.ctrlKey || e.shiftKey)) {
                                    toggleScreen(item.frame.id);
                                    return;
                                }
                                setSelectedFrame(item.frame);
                                setCurrentTime(item.frame.timestamp_ms);
                            }}
                        >
                            {thumbnails.has(item.frame.id) ? (
                                <img src={thumbnails.get(item.frame.id)} alt={`Frame ${item.frame.frame_number}`} />
                            ) : (
                                <div className="thumbnail-loading">
                                    <span>🖼️</span>
                                </div>
                            )}
                            {selectedScreens.has(item.frame.id) && <span className="rd-check">✓</span>}
                            <span className="thumbnail-time">{formatTime(item.frame.timestamp_ms)}</span>
                        </div>
                    )
                )}
            </div>

            {editable && selectedScreens.size > 0 && (
                <div style={{ padding: "0 12px 8px" }}>
                    <RedactionActionBar
                        label={`${selectedScreens.size} ${selectedScreens.size === 1 ? "screen" : "screens"} selected`}
                        onDelete={deleteSelectedScreens}
                        onStrike={() => redaction.strikeScreens([...selectedScreens])}
                        onClear={() => {
                            setSelectedScreens(new Set());
                            setSelectMode(false);
                        }}
                    />
                </div>
            )}

            {/* Stats bar */}
            <div className="rewind-stats-bar">
                <span>📷 {timeline?.frames.length || 0} frames</span>
                <span>💬 {timeline?.transcripts.length || 0} transcripts</span>
                <span>⏱️ {formatTime(maxTime)} duration</span>
                {editable && (timeline?.frames.length ?? 0) > 0 && (
                    <button
                        className={`rd-btn rd-btn-ghost`}
                        style={{ marginLeft: "auto", padding: "0 6px" }}
                        onClick={() => {
                            setSelectMode((m) => !m);
                            if (selectMode) setSelectedScreens(new Set());
                        }}
                        title="Or ⌘-click thumbnails"
                    >
                        {selectMode ? "Done selecting" : "Select screens"}
                    </button>
                )}
            </div>

            {redaction.ui}
        </div>
    );
}
