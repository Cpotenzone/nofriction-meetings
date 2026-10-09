// Rewind: the screens next to what was said. Scrub to any moment; marks
// sit on the timeline. Editing is one gesture: click a line (or a screen)
// to select it, drag or Shift-click for a range, ⌘A for all, and one bar
// offers Delete and Strike from the record… (docs/REDACTION.md). Screens
// and transcript are always selected together by time. Double-click a
// line to pick single words.

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
    EMPTY_SELECTION,
    clickItem,
    lineSpan,
    linkSelection,
    matchesPreview,
    plural,
    pruneSelection,
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
import { MarkerInline, MarkerList, MarkerPins, useMarkers } from "./study/MarkerList";
import { MarkKindContext } from "./study/MarkerBits";
import { useRecordingKind } from "../hooks/useRecordingKind";
import { markersInSpans, placeMarkers } from "../lib/studyLogic";
import { ChevronDownIcon, DisplayIcon } from "./icons";
import "./Rewind.css";

/** A moment to show, asked for from outside (Links → "first said at 12:03").
 *  `n` makes each request new, so the same time can be asked for again. */
export interface RewindSeek {
    meetingId: string;
    ms: number;
    n: number;
}

interface RewindGalleryProps {
    meetingId: string | null;
    isRecording: boolean;
    seek?: RewindSeek | null;
}

type StripItem =
    | { kind: "frame"; ms: number; frame: tauri.TimelineFrame }
    | { kind: "stricken"; ms: number; record: RedactionRecord };

type Pane = "screens" | "lines";

/** An exact span picked as such (Select time…), kept so the selection is
 *  the span, not just the items inside it. */
interface Hint {
    span: Span;
    minutes?: number;
}

const LAST_MINUTES = [5, 15, 30] as const;

export function RewindGallery({ meetingId, isRecording, seek = null }: RewindGalleryProps) {
    const [timeline, setTimeline] = useState<tauri.SyncedTimeline | null>(null);
    const [currentTime, setCurrentTime] = useState(0);
    const [selectedFrame, setSelectedFrame] = useState<tauri.TimelineFrame | null>(null);
    const [frameImage, setFrameImage] = useState<string | null>(null);
    const [thumbnails, setThumbnails] = useState<Map<string, string>>(new Map());
    const [isLoading, setIsLoading] = useState(false);
    const [reloadKey, setReloadKey] = useState(0);
    // Selection (lib/timelineSelection.ts): each pane's picks and the pane
    // the user last selected in. The panes are linked by time.
    const [screenSel, setScreenSel] = useState<Selection>(EMPTY_SELECTION);
    const [lineSel, setLineSel] = useState<Selection>(EMPTY_SELECTION);
    const [origin, setOrigin] = useState<Pane | null>(null);
    const [hint, setHint] = useState<Hint | null>(null);
    // Word-level editing (the token picker): double-click a line, or "Edit words"
    const [wordMode, setWordMode] = useState(false);
    const [timeMenu, setTimeMenu] = useState(false);
    const [notesStale, setNotesStale] = useState(false);
    const [regenerating, setRegenerating] = useState(false);
    const [regenError, setRegenError] = useState<string | null>(null);
    const [needsAi, setNeedsAi] = useState(false);

    const galleryRef = useRef<HTMLDivElement>(null);
    const transcriptRef = useRef<HTMLDivElement>(null);
    const linesRef = useRef<HTMLDivElement>(null);
    const timeMenuRef = useRef<HTMLDivElement>(null);
    const acting = useRef(false);
    // Drag across lines selects a range
    const dragAnchor = useRef<string | null>(null);

    const words = useWordSelection();
    const clearWords = words.clear;
    const reload = useCallback(() => setReloadKey((k) => k + 1), []);
    const redaction = useRedaction(meetingId, reload);
    // Marks (docs/STUDY_TOOLS.md)
    const { markers, setMarkers, reload: reloadMarkers } = useMarkers(meetingId, reloadKey);
    // The recording's type labels the third mark (On the test / Follow up / Remember)
    const recKind = useRecordingKind(meetingId);

    // Reset when the recording changes
    useEffect(() => {
        setTimeline(null);
        setSelectedFrame(null);
        setFrameImage(null);
        setThumbnails(new Map());
        setScreenSel(EMPTY_SELECTION);
        setLineSel(EMPTY_SELECTION);
        setOrigin(null);
        setHint(null);
        setWordMode(false);
        clearWords();
    }, [meetingId, clearWords]);

    useEffect(() => {
        if (!meetingId) return;

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

        // Refresh while this recording is still running
        let interval: ReturnType<typeof setInterval> | null = null;
        if (isRecording) interval = setInterval(loadTimeline, 3000);
        return () => {
            if (interval) clearInterval(interval);
        };
    }, [meetingId, isRecording, reloadKey]);

    // Edits change line text: drop any word selection on reload
    useEffect(() => {
        clearWords();
    }, [reloadKey, clearWords]);

    // The selected frame's picture
    useEffect(() => {
        if (!selectedFrame) {
            setFrameImage(null);
            return;
        }
        tauri
            .getFrameThumbnail(selectedFrame.id, false)
            .then((base64) => setFrameImage(base64 ? `data:image/jpeg;base64,${base64}` : null))
            .catch((err) => console.error("Failed to load frame:", err));
    }, [selectedFrame]);

    // Thumbnails, progressively
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
    }, [timeline]); // eslint-disable-line react-hooks/exhaustive-deps

    const nearestFrame = useCallback(
        (ms: number) =>
            timeline && timeline.frames.length > 0
                ? timeline.frames.reduce((prev: tauri.TimelineFrame, curr: tauri.TimelineFrame) =>
                      Math.abs(curr.timestamp_ms - ms) < Math.abs(prev.timestamp_ms - ms) ? curr : prev,
                  )
                : null,
        [timeline],
    );

    // Jump to a moment asked for from outside, once the timeline is loaded
    const appliedSeek = useRef<number | null>(null);
    useEffect(() => {
        if (!seek || seek.meetingId !== meetingId || !timeline || appliedSeek.current === seek.n) return;
        appliedSeek.current = seek.n;
        setCurrentTime(seek.ms);
        const f = nearestFrame(seek.ms);
        if (f) setSelectedFrame(f);
    }, [seek, meetingId, timeline, nearestFrame]);

    const handleScrub = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) => {
            const time = parseInt(e.target.value, 10);
            setCurrentTime(time);
            const f = nearestFrame(time);
            if (f) setSelectedFrame(f);
        },
        [nearestFrame],
    );

    // The line at the current time
    const currentTranscript = timeline?.transcripts.reduce((best: tauri.TimelineTranscript | null, t: tauri.TimelineTranscript) => {
        if (t.timestamp_ms > currentTime) return best;
        if (!best) return t;
        return Math.abs(t.timestamp_ms - currentTime) < Math.abs(best.timestamp_ms - currentTime) ? t : best;
    }, null as tauri.TimelineTranscript | null);

    useEffect(() => {
        if (!currentTranscript || !transcriptRef.current) return;
        const el = transcriptRef.current.querySelector(`[data-transcript-id="${currentTranscript.id}"]`);
        if (el) el.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }, [currentTranscript?.id]); // eslint-disable-line react-hooks/exhaustive-deps

    useEffect(() => {
        if (!selectedFrame || !galleryRef.current) return;
        const el = galleryRef.current.querySelector(`[data-frame-id="${selectedFrame.id}"]`);
        if (el) el.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "center" });
    }, [selectedFrame?.id]);

    // Close the Select time… menu on an outside click
    useEffect(() => {
        if (!timeMenu) return;
        const onDown = (e: MouseEvent) => {
            if (timeMenuRef.current && !timeMenuRef.current.contains(e.target as Node)) setTimeMenu(false);
        };
        document.addEventListener("mousedown", onDown);
        return () => document.removeEventListener("mousedown", onDown);
    }, [timeMenu]);

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

    // What is selected: everything the time spans remove, in both panes.
    // The counts in the bar and the ids sent are these same lists.
    const link = useMemo(
        () => linkSelection({ screens: frames, lines: lineItems, screenSel, lineSel, meetingEndMs, hint: hint?.span ?? null }),
        [frames, lineItems, screenSel, lineSel, meetingEndMs, hint],
    );
    const screenIds = link.screenIds;
    const lineIds = useMemo(() => {
        const on = new Set([...link.lineIds, ...link.unplacedLineIds]);
        return lineItems.filter((l) => on.has(l.id)).map((l) => l.id);
    }, [link, lineItems]);
    const screenSet = useMemo(() => new Set(screenIds), [screenIds]);
    const lineSet = useMemo(() => new Set(lineIds), [lineIds]);
    const splitSet = link.splitLineIds;
    const hasSelection = link.ranges.length > 0 || link.unplacedLineIds.length > 0;
    const selSpans: Span[] = link.ranges;

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
    }, []);

    const exitWordMode = useCallback(() => {
        setWordMode(false);
        clearWords();
    }, [clearWords]);

    /** Word editing over the transcript. Starts with word `wi` of line `li`
     *  selected, or the whole line when `wi` is null. */
    const enterWordMode = (li: number | null, wi: number | null) => {
        clearSelection();
        setWordMode(true);
        if (li === null) return;
        const n = lineWords(transcripts[li]?.text ?? "").length;
        if (wi === null) words.selectLine(li, n);
        else if (wi < n) words.selectWord({ li, wi });
    };

    // Esc leaves word editing (a dialog's own Esc closes the dialog)
    useEffect(() => {
        if (!wordMode) return;
        const key = (e: KeyboardEvent) => {
            if (e.key === "Escape" && !redaction.busy) setWordMode(false);
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [wordMode, redaction.busy]);

    // A drag ends anywhere
    useEffect(() => {
        const up = () => {
            dragAnchor.current = null;
        };
        window.addEventListener("mouseup", up);
        return () => window.removeEventListener("mouseup", up);
    }, []);

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
        pick(pane, selectAll(pane === "screens" ? frames : lineItems), { span: [0, meetingEndMs] });

    // "Here": the line last clicked, else the line at the current time
    const lineHere = (() => {
        const anchor = lineSel.anchor && lineItems.find((l) => l.id === lineSel.anchor);
        if (anchor) return anchor;
        const at = currentTranscript?.timestamp_ms;
        return at === undefined ? null : lineItems.find((l) => l.timestamp_ms >= at) ?? null;
    })();
    const selectToEndFromHere = () => {
        const from = lineHere?.id ?? null;
        if (!from) return;
        const sel = selectToEnd(lineItems, from);
        const first = lineItems.find((x) => x.id === sel.anchor);
        pick("lines", sel, first ? { span: [first.timestamp_ms, meetingEndMs] } : null);
    };

    const selectLast = (minutes: number) => {
        const { sel, range } = selectLastMinutes(lineItems, meetingEndMs, minutes);
        pick("lines", sel, { span: range, minutes });
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

    /** Jump to a moment (a mark). Review's quiz links come in through `seek`. */
    const seekTo = useCallback(
        (ms: number) => {
            setCurrentTime(ms);
            const f = nearestFrame(ms);
            if (f) setSelectedFrame(f);
        },
        [nearestFrame],
    );
    const placed = useMemo(() => placeMarkers(transcripts, markers), [transcripts, markers]);

    const onThumbClick = (e: React.MouseEvent, frame: tauri.TimelineFrame) => {
        galleryRef.current?.focus({ preventScroll: true });
        if (!editable) {
            viewFrame(frame);
            return;
        }
        const mods = { shift: e.shiftKey, toggle: e.metaKey || e.ctrlKey, selectMode: false };
        const current = screenIds[0] ?? selectedFrame?.id ?? null;
        const r = clickItem(screenSel, frames, frame.id, mods, current, { covered: screenSet });
        setScreenSel(r.sel);
        if (r.view) {
            viewFrame(frame);
            return;
        }
        if (wordMode) exitWordMode();
        setOrigin("screens");
        // ⌘-click adjusts the selection; anything else redraws it
        if (!mods.toggle) setHint(null);
    };

    const lineSelectable = (t: tauri.TimelineTranscript) => editable && !wordMode && (wordCounts.get(t.id) ?? 0) > 0;

    const selectLineRange = (t: tauri.TimelineTranscript, mods: { shift: boolean; toggle: boolean }) => {
        const current = lineIds[0] ?? lineHere?.id ?? null;
        const r = clickItem(lineSel, lineItems, t.id, { ...mods, selectMode: false }, current, {
            plain: "replace",
            covered: lineSet,
        });
        setLineSel(r.sel);
        setOrigin("lines");
        if (!mods.toggle) setHint(null);
        // A plain click selects only this line
        if (r.view) setScreenSel(EMPTY_SELECTION);
    };

    const onLineClick = (e: React.MouseEvent, t: tauri.TimelineTranscript) => {
        if (wordMode) {
            seekToLine(t);
            return;
        }
        linesRef.current?.focus({ preventScroll: true });
        if (e.detail > 1) return; // the second click of a double-click
        const mods = { shift: e.shiftKey, toggle: e.metaKey || e.ctrlKey };
        if (!mods.shift && !mods.toggle) seekToLine(t);
        if (!lineSelectable(t)) return;
        selectLineRange(t, mods);
    };

    const onLineMouseDown = (e: React.MouseEvent, t: tauri.TimelineTranscript) => {
        // Shift-click, drag and double-click select lines, not page text
        if (lineSelectable(t) && (e.shiftKey || e.detail > 1)) e.preventDefault();
        if (lineSelectable(t) && e.button === 0 && !e.shiftKey && !e.metaKey && !e.ctrlKey && e.detail === 1) {
            dragAnchor.current = t.id;
        }
    };

    const onLineMouseEnter = (e: React.MouseEvent, t: tauri.TimelineTranscript) => {
        // Dragging from another line: extend the range to this one
        if (!dragAnchor.current || dragAnchor.current === t.id || !lineSelectable(t)) return;
        e.preventDefault();
        window.getSelection()?.removeAllRanges();
        const anchor = dragAnchor.current;
        const base: Selection = lineSel.anchor === anchor ? lineSel : { ids: new Set([anchor]), anchor, base: new Set() };
        const r = clickItem(base, lineItems, t.id, { shift: true, toggle: false, selectMode: false }, anchor, {
            plain: "replace",
            covered: lineSet,
        });
        setLineSel(r.sel);
        setScreenSel(EMPTY_SELECTION);
        setOrigin("lines");
        setHint(null);
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
        // Marks in the deleted time go with it at commit (Undo brings them back)
        if (p?.ranges?.length) {
            const spans = p.ranges.map((r) => [r.start_ms, r.end_ms] as [number, number]);
            setMarkers((ms) => {
                const gone = markersInSpans(ms, spans);
                return ms.filter((m) => !gone.has(m.id));
            });
        }
        words.clear();
        clearSelection();
    };

    const msRanges = (): MsRange[] => link.ranges.map(([a, b]) => ({ start_ms: a, end_ms: b }));

    /** Time-range semantics over every selected span (screens, transcript
     *  split at the edges, screen video), one undo. Deletes straight away
     *  when the backend's preview removes exactly what is highlighted;
     *  otherwise shows that preview first. */
    const deleteSelection = async () => {
        if (!meetingId || acting.current || redaction.busy) return;
        acting.current = true;
        try {
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
                    ask("Deleting this time removes something other than what's highlighted. This is exactly what will be removed:");
                    return;
                }
                // Marks are notes the user wrote: never remove them unseen
                if ((p.moment_markers ?? 0) > 0) {
                    ask("This time also has marks you placed; they go with it. This is exactly what will be removed:");
                    return;
                }
            }
            if (await redaction.deleteRanges(p, unplaced, startedAt)) hideDeleted(p, unplaced);
        } finally {
            acting.current = false;
        }
    };

    const strikeSelection = () => {
        redaction.openRanges({
            mode: "strike",
            ranges: msRanges(),
            lineIds: link.unplacedLineIds.map(Number),
            startedAt,
            onStruck: clearSelection,
        });
    };

    const regenerateNotes = async () => {
        if (!meetingId) return;
        setRegenerating(true);
        setRegenError(null);
        setNeedsAi(false);
        try {
            await tauri.generateMeetingReport(meetingId);
            setNotesStale(false);
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setRegenError(friendlyAiError(e));
        } finally {
            setRegenerating(false);
        }
    };

    const formatTime = (ms: number) => {
        const seconds = Math.floor(ms / 1000);
        const h = Math.floor(seconds / 3600);
        const mins = Math.floor((seconds % 3600) / 60);
        const secs = seconds % 60;
        return h > 0
            ? `${h}:${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`
            : `${mins}:${String(secs).padStart(2, "0")}`;
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
        } else if ((e.key === "Delete" || e.key === "Backspace") && hasSelection) {
            e.preventDefault();
            void deleteSelection();
        } else if (e.key === "Escape" && hasSelection) {
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
        } else if ((e.key === "Delete" || e.key === "Backspace") && hasSelection) {
            e.preventDefault();
            void deleteSelection();
        } else if (e.key === "Escape" && hasSelection) {
            clearSelection();
        }
    };

    // "17 screens · 42 lines · 10:41–10:53": exactly what will be deleted
    const selectionLabel = (() => {
        if (!hasSelection) return "";
        const parts: string[] = [];
        if (screenIds.length) parts.push(plural(screenIds.length, "screen", "screens"));
        parts.push(`${plural(lineIds.length, "line", "lines")}${splitSet.size ? ` (${splitSet.size} split)` : ""}`);
        if (startedAt && hint?.minutes !== undefined) {
            parts.push(`last ${hint.minutes} min (${spanLabel(startedAt, hint.span[0], hint.span[1])})`);
        } else if (startedAt && selSpans.length) {
            parts.push(spanLabel(startedAt, selSpans[0][0], selSpans[selSpans.length - 1][1]));
        }
        if (selSpans.length > 1 && hint?.minutes === undefined) parts.push(`${selSpans.length} spans`);
        return parts.join(" · ");
    })();

    const pct = (ms: number) => Math.max(0, Math.min(100, (ms / maxTime) * 100));
    const oneLineIndex = lineIds.length === 1 ? transcripts.findIndex((t) => t.id === lineIds[0]) : -1;

    if (!meetingId) {
        return (
            <div className="rewind-empty">
                <p>Select a recording to rewind it.</p>
            </div>
        );
    }

    if (isLoading && !timeline) {
        return (
            <div className="rewind-loading">
                <div className="loading-spinner" />
                <p>Loading the timeline…</p>
            </div>
        );
    }

    const timeMenuItems = (
        <div className="rw-menu" role="menu" ref={timeMenuRef}>
            {LAST_MINUTES.map((m) => (
                <button key={m} type="button" role="menuitem" className="rw-menu__item" onClick={() => { setTimeMenu(false); selectLast(m); }}>
                    Last {m} min
                </button>
            ))}
            <button type="button" role="menuitem" className="rw-menu__item" disabled={!lineHere} onClick={() => { setTimeMenu(false); selectToEndFromHere(); }}>
                From here to the end
            </button>
            <button type="button" role="menuitem" className="rw-menu__item" onClick={() => { setTimeMenu(false); selectAllIn("lines"); }}>
                Everything
            </button>
            <button type="button" role="menuitem" className="rw-menu__item" disabled={!startedAt} onClick={() => { setTimeMenu(false); openTimeRange(); }}>
                Time range…
            </button>
        </div>
    );

    return (
        <MarkKindContext.Provider value={recKind}>
        <div className="rewind-gallery">
            <div className="rewind-main">
                {/* The screen at the current moment */}
                <div className="rewind-frame-preview">
                    {frameImage ? (
                        <img src={frameImage} alt="The screen at this moment" />
                    ) : (
                        <div className="frame-placeholder">
                            <DisplayIcon size={36} strokeWidth={1.5} />
                            <p>{frames.length === 0 ? "No screens were captured" : "No screen at this moment"}</p>
                        </div>
                    )}
                    <div className="frame-timestamp">{selectedFrame && formatTime(selectedFrame.timestamp_ms)}</div>
                </div>

                {/* Transcript */}
                <div className="rewind-transcripts scrollable" ref={transcriptRef}>
                    <MarkerList
                        meetingId={meetingId}
                        markers={markers}
                        currentMs={currentTime}
                        onJump={seekTo}
                        onChanged={reloadMarkers}
                    />
                    {notesStale && (
                        <div className="rd-stale" role="status">
                            <span>The notes were made before an edit.</span>
                            <button className="rd-btn" onClick={regenerateNotes} disabled={regenerating}>
                                {regenerating ? "Making notes…" : "Make again"}
                            </button>
                        </div>
                    )}
                    {needsAi && <AiSetupNotice feature="Notes" compact />}
                    {regenError && <p className="rd-tip" role="alert">Couldn't make the notes: {regenError}</p>}
                    {editable && transcripts.length > 0 && !wordMode && !hasSelection && (
                        <p className="rd-tip">Click a line to select it, drag or Shift-click for a range, ⌘A for all. Double-click a line to edit its words.</p>
                    )}
                    {editable && wordMode && (
                        <div className="rd-selectbar rd-lines-toolbar">
                            <span className="rd-hint">Editing words: click a word, Shift-click or drag to select more.</span>
                            <button className="rd-btn rd-btn-ghost" style={{ padding: "0 6px" }} onClick={exitWordMode} title="Esc">
                                Done
                            </button>
                        </div>
                    )}
                    {placed.before.map((m) => (
                        <MarkerInline key={m.id} m={m} onJump={seekTo} />
                    ))}
                    {transcripts.length === 0 ? (
                        <p className="no-transcripts">Nothing was said yet.</p>
                    ) : (
                        <div
                            className="transcript-entries"
                            ref={linesRef}
                            tabIndex={editable ? 0 : undefined}
                            onKeyDown={onLinesKey}
                            role="listbox"
                            aria-multiselectable={editable && !wordMode}
                            aria-label="Transcript. Click a line to select it, drag or Shift-click for a range, ⌘A for all, Delete to delete, double-click to edit words"
                        >
                            {transcripts.map((t: tauri.TimelineTranscript, li: number) => {
                                const sel = lineSet.has(t.id);
                                const split = splitSet.has(t.id);
                                const selectable = lineSelectable(t);
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
                                        title={split ? "Partly inside the selected time: only the words spoken inside it are removed" : undefined}
                                        onMouseDown={(e) => onLineMouseDown(e, t)}
                                        onMouseEnter={(e) => onLineMouseEnter(e, t)}
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
                                        {placed.after.get(t.id)?.map((m) => (
                                            <MarkerInline key={m.id} m={m} onJump={seekTo} />
                                        ))}
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

            {/* Timeline scrubber, with the marks as pins */}
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
                        aria-label="Moment in the recording"
                    />
                    <MarkerPins markers={markers} pct={pct} onJump={seekTo} />
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
                                title={`Screen at ${formatTime(f.timestamp_ms)}${screenSet.has(f.id) ? " (selected)" : ""}`}
                            />
                        ))}
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

            {/* Screens (focusable: ⌘A selects all, Delete deletes) */}
            {strip.length > 0 && (
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
                                    if (e.shiftKey) e.preventDefault();
                                }}
                                onClick={(e) => onThumbClick(e, item.frame)}
                            >
                                {thumbnails.has(item.frame.id) ? (
                                    <img src={thumbnails.get(item.frame.id)} alt={`Screen ${item.frame.frame_number}`} />
                                ) : (
                                    <div className="thumbnail-loading" />
                                )}
                                {screenSet.has(item.frame.id) && <span className="rd-check">✓</span>}
                                <span className="thumbnail-time">{formatTime(item.frame.timestamp_ms)}</span>
                            </div>
                        )
                    )}
                </div>
            )}

            {/* One bar: Delete · Strike from the record…, with Edit words and Select time… */}
            {editable && hasSelection && (
                <div className="rewind-actionbar">
                    <RedactionActionBar
                        label={selectionLabel}
                        busy={redaction.busy}
                        extra={
                            <>
                                {oneLineIndex >= 0 && (
                                    <button className="rd-btn rd-btn-ghost" onClick={() => enterWordMode(oneLineIndex, null)} title="Pick single words in this line (or double-click it)">
                                        Edit words
                                    </button>
                                )}
                                <span className="rw-menu-anchor">
                                    <button className="rd-btn rd-btn-ghost" onClick={() => setTimeMenu((v) => !v)} aria-haspopup="menu" aria-expanded={timeMenu}>
                                        Select time… <ChevronDownIcon size={12} />
                                    </button>
                                    {timeMenu && timeMenuItems}
                                </span>
                            </>
                        }
                        onDelete={() => void deleteSelection()}
                        onStrike={strikeSelection}
                        onClear={clearSelection}
                    />
                </div>
            )}

            <div className="rewind-status">
                <span>
                    {plural(frames.length, "screen", "screens")} · {plural(transcripts.length, "line", "lines")} · {formatTime(maxTime)}
                </span>
                {editable && !hasSelection && !wordMode && transcripts.length > 0 && (
                    <span className="rw-menu-anchor">
                        <button className="rd-btn rd-btn-ghost" onClick={() => setTimeMenu((v) => !v)} aria-haspopup="menu" aria-expanded={timeMenu}>
                            Select time… <ChevronDownIcon size={12} />
                        </button>
                        {timeMenu && timeMenuItems}
                    </span>
                )}
            </div>

            {redaction.ui}
        </div>
        </MarkKindContext.Provider>
    );
}
