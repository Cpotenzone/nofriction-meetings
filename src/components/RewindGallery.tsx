import { useState, useEffect, useRef, useCallback, useMemo } from "react";
import * as tauri from "../lib/tauri";
import { friendlyAiError, isNoProviderError } from "../lib/ai";
import { AiSetupNotice } from "./AiSetupNotice";
import {
    applyPreviewLines,
    lineWords,
    redactionApi,
    renderPlain,
    applyLocalDelete,
    type MsRange,
    type RedactionRecord,
    type TimeRangePreview,
} from "../lib/redaction";
import {
    EMPTY_LINKED,
    EMPTY_SELECTION,
    clickItem,
    clockAt,
    lineSpan,
    linkSelection,
    matchesPreview,
    orderedIds,
    plural,
    pruneSelection,
    runSpans,
    screenSpans,
    selectAll,
    selectLastMinutes,
    selectToEnd,
    spanLabel,
    type LineItem,
    type Selection,
    type Span,
} from "../lib/timelineSelection";
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

type Pane = "screens" | "lines";

/** An exact span picked as such: Last N minutes (`minutes` set), From here
 *  to the end, Select all. */
interface Hint {
    span: Span;
    minutes?: number;
}

// "Linked" (screens and transcript selected together by time) is on by
// default and remembered per user. Storage can be unavailable: then it's on.
const LINKED_KEY = "nf.recordings.linkedSelection";
function readLinked(): boolean {
    try {
        return localStorage.getItem(LINKED_KEY) !== "0";
    } catch {
        return true;
    }
}
function writeLinked(on: boolean) {
    try {
        localStorage.setItem(LINKED_KEY, on ? "1" : "0");
    } catch {
        /* storage unavailable: the choice lasts for this session only */
    }
}

export function RewindGallery({ meetingId, isRecording }: RewindGalleryProps) {
    const [timeline, setTimeline] = useState<tauri.SyncedTimeline | null>(null);
    const [currentTime, setCurrentTime] = useState(0);
    const [selectedFrame, setSelectedFrame] = useState<tauri.TimelineFrame | null>(null);
    const [frameImage, setFrameImage] = useState<string | null>(null);
    const [thumbnails, setThumbnails] = useState<Map<string, string>>(new Map());
    const [isLoading, setIsLoading] = useState(false);
    const [reloadKey, setReloadKey] = useState(0);
    // Selection (lib/timelineSelection.ts): each pane's picks, the pane the
    // user last selected in, and whether the panes are linked by time
    const [screenSel, setScreenSel] = useState<Selection>(EMPTY_SELECTION);
    const [lineSel, setLineSel] = useState<Selection>(EMPTY_SELECTION);
    const [origin, setOrigin] = useState<Pane | null>(null);
    const [linked, setLinked] = useState(readLinked);
    const [hint, setHint] = useState<Hint | null>(null);
    const [selectMode, setSelectMode] = useState(false);
    // Word-level editing (the token picker): double-click a line, or "Edit words"
    const [wordMode, setWordMode] = useState(false);
    const [lastMinutes, setLastMinutes] = useState(12);
    const [notesStale, setNotesStale] = useState(false);
    const [regenerating, setRegenerating] = useState(false);
    const [regenError, setRegenError] = useState<string | null>(null);
    const [needsAi, setNeedsAi] = useState(false);

    const galleryRef = useRef<HTMLDivElement>(null);
    const transcriptRef = useRef<HTMLDivElement>(null);
    const linesRef = useRef<HTMLDivElement>(null);
    const acting = useRef(false);

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
        setScreenSel(EMPTY_SELECTION);
        setLineSel(EMPTY_SELECTION);
        setOrigin(null);
        setHint(null);
        setSelectMode(false);
        setWordMode(false);
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
                setScreenSel((prev) => pruneSelection(data.frames, prev));
                setLineSel((prev) => pruneSelection(data.transcripts, prev));
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

    const nearestFrame = useCallback(
        (ms: number) =>
            timeline && timeline.frames.length > 0
                ? timeline.frames.reduce((prev: tauri.TimelineFrame, curr: tauri.TimelineFrame) =>
                      Math.abs(curr.timestamp_ms - ms) < Math.abs(prev.timestamp_ms - ms) ? curr : prev,
                  )
                : null,
        [timeline],
    );

    // Handle timeline scrubbing
    const handleScrub = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        const time = parseInt(e.target.value, 10);
        setCurrentTime(time);
        const f = nearestFrame(time);
        if (f) setSelectedFrame(f);
    }, [nearestFrame]);

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

    // ── Timeline data for selection ──────────────────────────────────────
    const editable = !isRecording;
    const frames = useMemo(() => timeline?.frames ?? [], [timeline?.frames]);
    const transcripts = useMemo(() => timeline?.transcripts ?? [], [timeline?.transcripts]);
    const records = useMemo(
        () => new Map((timeline?.redactions ?? []).map((r) => [r.id, r] as const)),
        [timeline?.redactions],
    );
    // Words left per line (a line holding only strike markers has none and
    // can't be selected: there is nothing in it to remove)
    const wordCounts = useMemo(() => new Map(transcripts.map((t) => [t.id, lineWords(t.text).length] as const)), [transcripts]);
    const lineItems: LineItem[] = useMemo(
        () =>
            transcripts
                .filter((t) => (wordCounts.get(t.id) ?? 0) > 0)
                .map((t) => ({
                    id: t.id,
                    timestamp_ms: t.timestamp_ms,
                    end_ms: t.end_ms ?? null,
                    word_mids_ms: t.word_mids_ms ?? null,
                    words: wordCounts.get(t.id),
                })),
        [transcripts, wordCounts],
    );

    const maxTime = useMemo(
        () =>
            timeline
                ? Math.max(
                      ...timeline.frames.map((f) => f.timestamp_ms),
                      ...timeline.transcripts.map((t) => Math.max(t.end_ms ?? 0, t.timestamp_ms + t.duration_seconds * 1000)),
                      1000,
                  )
                : 1000,
        [timeline],
    );
    const meetingEndMs = Math.max(maxTime, (timeline?.duration_seconds ?? 0) * 1000);
    const startedAt = timeline?.started_at ?? "";
    const sSpans = useMemo(() => screenSpans(frames, meetingEndMs), [frames, meetingEndMs]);

    // What is selected. Linked: everything the time spans remove, in both
    // panes. Not linked: the picks of the pane the user selected in. The
    // counts in the bar and the ids sent are these same lists.
    const link = useMemo(
        () =>
            linked
                ? linkSelection({ screens: frames, lines: lineItems, screenSel, lineSel, meetingEndMs, hint: hint?.span ?? null })
                : EMPTY_LINKED,
        [linked, frames, lineItems, screenSel, lineSel, meetingEndMs, hint],
    );
    const screenIds = useMemo(
        () => (linked ? link.screenIds : origin === "screens" ? orderedIds(frames, screenSel) : []),
        [linked, link, origin, frames, screenSel],
    );
    const lineIds = useMemo(() => {
        if (!linked) return origin === "lines" ? orderedIds(lineItems, lineSel) : [];
        const on = new Set([...link.lineIds, ...link.unplacedLineIds]);
        return lineItems.filter((l) => on.has(l.id)).map((l) => l.id);
    }, [linked, link, origin, lineItems, lineSel]);
    const screenSet = useMemo(() => new Set(screenIds), [screenIds]);
    const lineSet = useMemo(() => new Set(lineIds), [lineIds]);
    const splitSet = link.splitLineIds;
    const scope: "linked" | Pane | null = linked
        ? link.ranges.length > 0 || link.unplacedLineIds.length > 0
            ? "linked"
            : null
        : origin === "screens" && screenIds.length > 0
          ? "screens"
          : origin === "lines" && lineIds.length > 0
            ? "lines"
            : null;
    // The selected time, for the scrubber and the bar
    const selSpans: Span[] = useMemo(() => {
        if (scope === "linked") return link.ranges;
        if (scope === "screens") return runSpans(frames, screenSel.ids, (s) => sSpans.get(s.id));
        if (scope === "lines") return runSpans(lineItems, lineSel.ids, lineSpan);
        return [];
    }, [scope, link, frames, screenSel, sSpans, lineItems, lineSel]);

    // ── Word editing (token picker) ──────────────────────────────────────
    const lines = useMemo(() => transcripts.map((t) => ({ id: Number(t.id), text: t.text })), [transcripts]);
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

    // ── Selection ────────────────────────────────────────────────────────
    const clearSelection = useCallback(() => {
        setScreenSel(EMPTY_SELECTION);
        setLineSel(EMPTY_SELECTION);
        setOrigin(null);
        setHint(null);
        setSelectMode(false);
    }, []);

    const exitWordMode = useCallback(() => {
        setWordMode(false);
        clearWords();
    }, [clearWords]);

    /** Word editing over the transcript (the token picker). Starts with
     *  word `wi` of line `li` selected, or the whole line when `wi` is null,
     *  or nothing when `li` is null. */
    const enterWordMode = (li: number | null, wi: number | null) => {
        clearSelection();
        setWordMode(true);
        if (li === null) return;
        const n = lineWords(transcripts[li]?.text ?? "").length;
        if (wi === null) words.selectLine(li, n);
        else if (wi < n) words.selectWord({ li, wi });
    };

    // Esc anywhere leaves word editing (a dialog's own Esc closes the dialog)
    useEffect(() => {
        if (!wordMode) return;
        const key = (e: KeyboardEvent) => {
            if (e.key === "Escape" && !redaction.busy) setWordMode(false);
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [wordMode, redaction.busy]);

    const toggleLinked = () => {
        const next = !linked;
        setLinked(next);
        writeLinked(next);
        if (!next) {
            // Actions now apply only to the pane the user selected in
            const plain = (s: Selection): Selection => ({ ids: s.ids, anchor: s.anchor, base: s.base });
            if (origin === "lines") {
                setScreenSel(EMPTY_SELECTION);
                setLineSel(plain);
            } else {
                setLineSel(EMPTY_SELECTION);
                setScreenSel(plain);
            }
            if (!hint?.minutes) setHint(null);
        }
    };

    /** A new selection in one pane replaces the other pane's. */
    const pick = (pane: Pane, sel: Selection, h: Hint | null) => {
        if (wordMode) exitWordMode();
        if (pane === "screens") {
            setScreenSel(sel);
            setLineSel(EMPTY_SELECTION);
        } else {
            setLineSel(sel);
            setScreenSel(EMPTY_SELECTION);
        }
        setOrigin(pane);
        setHint(h);
    };

    const selectAllIn = (pane: Pane) =>
        pick(pane, selectAll(pane === "screens" ? frames : lineItems), linked ? { span: [0, meetingEndMs] } : null);

    // "Here": the screen being viewed; in the transcript, the line last
    // clicked, else the line at the current time
    const lineHere = (() => {
        const anchor = lineSel.anchor && lineItems.find((l) => l.id === lineSel.anchor);
        if (anchor) return anchor;
        const at = currentTranscript?.timestamp_ms;
        return at === undefined ? null : lineItems.find((l) => l.timestamp_ms >= at) ?? null;
    })();
    const selectToEndIn = (pane: Pane) => {
        const items = pane === "screens" ? frames : lineItems;
        const from = pane === "screens" ? (selectedFrame?.id ?? screenSel.anchor) : (lineHere?.id ?? null);
        if (!from) return;
        const sel = selectToEnd(items, from);
        const first = items.find((x) => x.id === sel.anchor);
        pick(pane, sel, linked && first ? { span: [first.timestamp_ms, meetingEndMs] } : null);
    };

    const selectLastIn = (pane: Pane) => {
        const { sel, range } = selectLastMinutes(pane === "screens" ? frames : lineItems, meetingEndMs, lastMinutes);
        pick(pane, sel, { span: range, minutes: lastMinutes });
    };

    const viewFrame = (frame: tauri.TimelineFrame) => {
        setSelectedFrame(frame);
        setCurrentTime(frame.timestamp_ms);
    };

    const seekToLine = (t: tauri.TimelineTranscript) => {
        setCurrentTime(t.timestamp_ms);
        const f = nearestFrame(t.timestamp_ms);
        if (f) setSelectedFrame(f);
    };

    const onThumbClick = (e: React.MouseEvent, frame: tauri.TimelineFrame) => {
        // Keyboard shortcuts (⌘A, Delete) act on the grid once it's used
        galleryRef.current?.focus({ preventScroll: true });
        if (!editable) {
            viewFrame(frame);
            return;
        }
        const mods = { shift: e.shiftKey, toggle: e.metaKey || e.ctrlKey, selectMode };
        const current = (linked ? screenIds[0] : undefined) ?? selectedFrame?.id ?? null;
        const r = clickItem(screenSel, frames, frame.id, mods, current, { covered: linked ? screenSet : undefined });
        setScreenSel(r.sel);
        if (r.view) {
            viewFrame(frame);
            return;
        }
        if (wordMode) exitWordMode();
        setOrigin("screens");
        // ⌘-click adjusts a linked selection; anything else redraws it
        if (!(linked && (mods.toggle || mods.selectMode))) setHint(null);
        if (!linked) setLineSel(EMPTY_SELECTION);
    };

    const onLineClick = (e: React.MouseEvent, t: tauri.TimelineTranscript) => {
        if (wordMode) {
            seekToLine(t);
            return;
        }
        linesRef.current?.focus({ preventScroll: true });
        if (e.detail > 1) return; // the second click of a double-click
        const mods = { shift: e.shiftKey, toggle: e.metaKey || e.ctrlKey, selectMode: false };
        if (!mods.shift && !mods.toggle) seekToLine(t);
        if (!editable || !(wordCounts.get(t.id) ?? 0)) return;
        const current = (linked ? lineIds[0] : undefined) ?? lineHere?.id ?? null;
        const r = clickItem(lineSel, lineItems, t.id, mods, current, {
            plain: "replace",
            covered: linked ? lineSet : undefined,
        });
        setLineSel(r.sel);
        setOrigin("lines");
        if (!(linked && mods.toggle)) setHint(null);
        // A plain click selects only this line; unlinked, one pane at a time
        if (!linked || r.view) setScreenSel(EMPTY_SELECTION);
    };

    const onLineDoubleClick = (e: React.MouseEvent, li: number) => {
        const el = (e.target as HTMLElement).closest("[data-wi]") as HTMLElement | null;
        enterWordMode(li, el ? Number(el.dataset.wi) : null);
    };

    // ── Actions ──────────────────────────────────────────────────────────
    /** Hide what a Delete removes during its undo window. */
    const hideDeleted = (p: TimeRangePreview | null, goneLineIds: (string | number)[]) => {
        const screens = new Set(p?.screen_ids ?? []);
        const gone = new Set(goneLineIds.map(String));
        setTimeline((tl) =>
            tl
                ? {
                      ...tl,
                      frames: tl.frames.filter((f) => !screens.has(f.id)),
                      transcripts: applyPreviewLines(
                          tl.transcripts.filter((t) => !gone.has(t.id)),
                          p?.lines ?? [],
                      ),
                  }
                : tl,
        );
        setSelectedFrame((f) => (f && screens.has(f.id) ? null : f));
        words.clear();
        clearSelection();
    };

    const msRanges = (): MsRange[] => link.ranges.map(([a, b]) => ({ start_ms: a, end_ms: b }));

    /** Linked: time-range semantics over every selected span (screens,
     *  transcript split at the edges, screen video), one undo. Deletes
     *  straight away when the backend's preview removes exactly what is
     *  highlighted; otherwise shows that preview first. */
    const deleteLinked = async () => {
        if (!meetingId) return;
        const ranges = msRanges();
        const unplaced = link.unplacedLineIds.map(Number);
        const ask = (note?: string) =>
            redaction.openRanges({ mode: "delete", ranges, lineIds: unplaced, startedAt, note, onDeleted: hideDeleted });
        let p: TimeRangePreview | null = null;
        if (ranges.length > 0) {
            try {
                p = await redactionApi.previewTimeRanges(meetingId, ranges);
            } catch {
                ask();
                return;
            }
            if (!matchesPreview(link, p)) {
                ask(
                    "What a Delete removes at the edges of the selection differs a little from what's highlighted (word timings and the half-a-line rule decide). This is exactly what will be removed:",
                );
                return;
            }
        }
        if (await redaction.deleteRanges(p, unplaced, startedAt)) hideDeleted(p, unplaced);
    };

    const deleteSelectedScreens = async () => {
        const ids = screenIds;
        if (ids.length === 0) return;
        if (await redaction.deleteScreens(ids)) {
            const gone = new Set(ids);
            setTimeline((tl) => (tl ? { ...tl, frames: tl.frames.filter((f) => !gone.has(f.id)) } : tl));
            setSelectedFrame((f) => (f && gone.has(f.id) ? null : f));
            clearSelection();
        }
    };

    const deleteSelectedLines = async () => {
        const ids = lineIds;
        if (ids.length === 0) return;
        if (await redaction.deleteLines(ids.map(Number))) hideDeleted(null, ids);
    };

    const deleteSelection = async () => {
        if (acting.current || redaction.busy) return;
        acting.current = true;
        try {
            if (scope === "linked") await deleteLinked();
            else if (scope === "screens") await deleteSelectedScreens();
            else if (scope === "lines") await deleteSelectedLines();
        } finally {
            acting.current = false;
        }
    };

    const strikeSelection = () => {
        if (scope === "linked") {
            redaction.openRanges({
                mode: "strike",
                ranges: msRanges(),
                lineIds: link.unplacedLineIds.map(Number),
                startedAt,
                onStruck: clearSelection,
            });
        } else if (scope === "screens") {
            redaction.strikeScreens(screenIds);
        } else if (scope === "lines") {
            redaction.strikeLines(
                lineIds.map(Number),
                lineIds.reduce((a, id) => a + (wordCounts.get(id) ?? 0), 0),
            );
        }
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

    const openTimeRange = () => {
        if (!startedAt) return;
        const r: Span =
            hint?.span ??
            (selSpans.length ? [selSpans[0][0], selSpans[selSpans.length - 1][1]] : null) ??
            [currentTime, meetingEndMs];
        redaction.openTimeRange({
            startMs: r[0],
            endMs: r[1] > r[0] ? r[1] : r[0] + 60_000,
            startedAt,
            maxMs: meetingEndMs,
            onDeleted: (p) => hideDeleted(p, []),
        });
    };

    const onGridKey = (e: React.KeyboardEvent) => {
        if (!editable || redaction.busy) return;
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
            e.preventDefault();
            selectAllIn("screens");
        } else if ((e.key === "Delete" || e.key === "Backspace") && (scope === "screens" || scope === "linked")) {
            e.preventDefault();
            deleteSelection();
        } else if (e.key === "Escape" && scope) {
            clearSelection();
        }
    };

    const onLinesKey = (e: React.KeyboardEvent) => {
        if (!editable || redaction.busy) return;
        if (wordMode) {
            if (e.key === "Escape") exitWordMode();
            return;
        }
        if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "a") {
            e.preventDefault();
            selectAllIn("lines");
        } else if ((e.key === "Delete" || e.key === "Backspace") && (scope === "lines" || scope === "linked")) {
            e.preventDefault();
            deleteSelection();
        } else if (e.key === "Escape" && scope) {
            clearSelection();
        }
    };

    // "17 screens · 42 lines · 10:41–10:53 · 2 groups": exactly what will be
    // deleted (unlinked: exactly the ids that will be sent)
    const selectionLabel = (() => {
        if (!scope) return "";
        const parts: string[] = [];
        if (scope !== "lines") parts.push(plural(screenIds.length, "screen", "screens"));
        if (scope !== "screens") {
            parts.push(`${plural(lineIds.length, "line", "lines")}${splitSet.size ? ` (${splitSet.size} split)` : ""}`);
        }
        const lastN = !linked && hint?.minutes !== undefined;
        if (startedAt && lastN && hint) {
            parts.push(`last ${hint.minutes} min (${spanLabel(startedAt, hint.span[0], hint.span[1])})`);
        } else if (startedAt && selSpans.length) {
            parts.push(spanLabel(startedAt, selSpans[0][0], selSpans[selSpans.length - 1][1]));
        }
        if (selSpans.length > 1 && !lastN) parts.push(`${selSpans.length} groups`);
        if (scope === "screens") parts.push("screens only");
        if (scope === "lines") parts.push("transcript only");
        return parts.join(" · ");
    })();

    const pct = (ms: number) => Math.max(0, Math.min(100, (ms / maxTime) * 100));
    const oneLineIndex = lineIds.length === 1 ? transcripts.findIndex((t) => t.id === lineIds[0]) : -1;

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
                    {editable && transcripts.length > 0 && (
                        <div className="rd-selectbar rd-lines-toolbar">
                            {wordMode ? (
                                <>
                                    <span className="rd-hint">
                                        Editing words: click a word, Shift-click or drag to select more.
                                    </span>
                                    <button className="rd-btn rd-btn-ghost" style={{ padding: "0 6px" }} onClick={exitWordMode} title="Esc">
                                        Done
                                    </button>
                                </>
                            ) : (
                                <>
                                    <button
                                        className="rd-btn rd-btn-ghost"
                                        style={{ padding: "0 6px" }}
                                        onClick={() => selectAllIn("lines")}
                                        title="⌘A in the transcript"
                                    >
                                        Select all
                                    </button>
                                    <button
                                        className="rd-btn rd-btn-ghost"
                                        style={{ padding: "0 6px" }}
                                        onClick={() => selectToEndIn("lines")}
                                        disabled={!lineHere}
                                        title={
                                            lineHere
                                                ? `From the line at ${startedAt ? clockAt(startedAt, lineHere.timestamp_ms) : formatTime(lineHere.timestamp_ms)} to the end of the meeting`
                                                : "Click a line first"
                                        }
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
                                    <button className="rd-btn rd-btn-ghost" style={{ padding: "0 6px" }} onClick={() => selectLastIn("lines")}>
                                        Select
                                    </button>
                                    <button
                                        className="rd-btn rd-btn-ghost"
                                        style={{ padding: "0 6px" }}
                                        onClick={() => enterWordMode(oneLineIndex >= 0 ? oneLineIndex : null, null)}
                                        title="Pick single words to delete or strike (or double-click a line)"
                                    >
                                        Edit words
                                    </button>
                                </>
                            )}
                        </div>
                    )}
                    {editable && transcripts.length > 0 && !wordMode && !scope && (
                        <p className="rd-tip">
                            Click a line to select it. Shift-click selects a range, ⌘-click adds or removes a line.
                            Double-click a line to edit its words.
                        </p>
                    )}
                    {transcripts.length === 0 ? (
                        <p className="no-transcripts">No transcripts yet</p>
                    ) : (
                        <div
                            className="transcript-entries"
                            ref={linesRef}
                            tabIndex={editable ? 0 : undefined}
                            onKeyDown={onLinesKey}
                            role="listbox"
                            aria-multiselectable={editable && !wordMode}
                            aria-label="Transcript. Click a line to select it, Shift-click to select a range, ⌘-click to add or remove one, ⌘A to select all, Delete to delete, double-click to edit words"
                        >
                            {transcripts.map((t: tauri.TimelineTranscript, li: number) => {
                                const sel = lineSet.has(t.id);
                                const split = splitSet.has(t.id);
                                const selectable = editable && !wordMode && (wordCounts.get(t.id) ?? 0) > 0;
                                return (
                                    <div
                                        key={t.id}
                                        data-transcript-id={t.id}
                                        role="option"
                                        aria-selected={sel}
                                        className={[
                                            "transcript-entry",
                                            t.id === currentTranscript?.id ? "active" : "",
                                            selectable ? "rd-line-selectable" : "",
                                            sel ? "rd-line-selected" : "",
                                            split ? "rd-line-split" : "",
                                        ]
                                            .filter(Boolean)
                                            .join(" ")}
                                        title={
                                            split
                                                ? "Partly inside the selected time: only the words spoken inside it are removed"
                                                : undefined
                                        }
                                        onMouseDown={(e) => {
                                            // Shift-click and double-click select lines/words, not page text
                                            if (selectable && (e.shiftKey || e.detail > 1)) e.preventDefault();
                                        }}
                                        onClick={(e) => onLineClick(e, t)}
                                        onDoubleClick={(e) => selectable && onLineDoubleClick(e, li)}
                                    >
                                        <span className="entry-time">{formatTime(t.timestamp_ms)}</span>
                                        {sel && (
                                            <span className="rd-line-check" aria-hidden="true">
                                                {split ? "◐" : "✓"}
                                            </span>
                                        )}
                                        <span
                                            className="entry-speaker"
                                            title={editable && wordMode ? "Double-click to select the whole line" : undefined}
                                            onDoubleClick={() => editable && wordMode && words.selectLine(li, lineWords(t.text).length)}
                                        >
                                            {t.speaker || "Speaker"}
                                        </span>
                                        <p className="entry-text">
                                            <RedactableLine
                                                text={t.text}
                                                li={li}
                                                selected={wordMode ? words.wordRangeForLine(li, lineWords(t.text).length) : null}
                                                records={records}
                                                editable={editable && wordMode}
                                                onWordDown={words.begin}
                                                onWordEnter={words.extend}
                                            />
                                        </p>
                                    </div>
                                );
                            })}
                        </div>
                    )}
                    {editable && wordMode && segments.length > 0 && (
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
                    {/* Selected time spans, and selected screens/lines in
                        yellow, so a pick scrolled out of view is still visible */}
                    <div className="timeline-markers">
                        {selSpans.map(([a, b], i) => (
                            <div
                                key={`span-${i}`}
                                className="rd-scrub-span"
                                style={{ left: `${pct(a)}%`, width: `${Math.max(0.4, pct(b) - pct(a))}%` }}
                                title={startedAt ? `Selected: ${spanLabel(startedAt, a, b)}` : "Selected"}
                            />
                        ))}
                        {frames.map((f: tauri.TimelineFrame) => (
                            <div
                                key={f.id}
                                className={`timeline-marker frame-marker${screenSet.has(f.id) ? " rd-marker-selected" : ""}`}
                                style={{ left: `${pct(f.timestamp_ms)}%` }}
                                title={`Frame at ${formatTime(f.timestamp_ms)}${screenSet.has(f.id) ? " (selected)" : ""}`}
                            />
                        ))}
                        {/* Transcript markers */}
                        {transcripts.filter((t: tauri.TimelineTranscript) => t.is_final).map((t: tauri.TimelineTranscript) => (
                            <div
                                key={t.id}
                                className={`timeline-marker transcript-marker${lineSet.has(t.id) ? " rd-marker-selected" : ""}`}
                                style={{ left: `${pct(t.timestamp_ms)}%` }}
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
                            className={`thumbnail ${item.frame.id === selectedFrame?.id ? "selected" : ""} ${screenSet.has(item.frame.id) ? "rd-selected" : ""}`}
                            aria-selected={screenSet.has(item.frame.id)}
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
                            {screenSet.has(item.frame.id) && <span className="rd-check">✓</span>}
                            <span className="thumbnail-time">{formatTime(item.frame.timestamp_ms)}</span>
                        </div>
                    )
                )}
            </div>

            {editable && scope && (
                <div style={{ padding: "0 12px 8px" }}>
                    <RedactionActionBar
                        label={selectionLabel}
                        busy={redaction.busy}
                        extra={
                            <>
                                <button
                                    className={`rd-btn rd-btn-ghost rd-link-toggle${linked ? " rd-on" : ""}`}
                                    aria-pressed={linked}
                                    onClick={toggleLinked}
                                    title={
                                        linked
                                            ? "Linked: screens and transcript are selected together by time, and Delete/Strike removes everything in the selected time. Click to act on one pane only."
                                            : "Not linked: Delete/Strike acts only on the pane you selected in. Click to select screens and transcript together by time."
                                    }
                                >
                                    {linked ? "Linked ✓" : "Linked"}
                                </button>
                                {oneLineIndex >= 0 && (
                                    <button
                                        className="rd-btn rd-btn-ghost"
                                        onClick={() => enterWordMode(oneLineIndex, null)}
                                        title="Pick single words in this line (or double-click it)"
                                    >
                                        Edit words
                                    </button>
                                )}
                                <button
                                    className="rd-btn rd-btn-ghost"
                                    onClick={openTimeRange}
                                    disabled={!startedAt}
                                    title="Type a start and end time: everything in it (screens, screen text, transcript and screen video)"
                                >
                                    Time range…
                                </button>
                            </>
                        }
                        onDelete={deleteSelection}
                        onStrike={strikeSelection}
                        onClear={clearSelection}
                    />
                </div>
            )}

            {/* Stats bar */}
            <div className="rewind-stats-bar">
                <span>📷 {frames.length} frames</span>
                <span>💬 {transcripts.length} transcripts</span>
                <span>⏱️ {formatTime(maxTime)} duration</span>
                {editable && frames.length > 0 && (
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
                            onClick={() => selectAllIn("screens")}
                            title="⌘A in the screen strip"
                        >
                            Select all
                        </button>
                        <button
                            className="rd-btn rd-btn-ghost"
                            style={{ padding: "0 6px" }}
                            onClick={() => selectToEndIn("screens")}
                            disabled={!selectedFrame && !screenSel.anchor}
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
                        <button className="rd-btn rd-btn-ghost" style={{ padding: "0 6px" }} onClick={() => selectLastIn("screens")}>
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
