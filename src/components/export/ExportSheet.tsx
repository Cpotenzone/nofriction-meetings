// Export a recording as a package (docs/EXPORT.md). One sheet:
// - What goes in: notes, highlights, screens and photos, marks, links, the
//   follow-up email, the review guide and the transcript, each a checkbox
// - Screens and photos: pick the ones to keep (click), caption them
// - AI tools: Make notes, Find highlights, Pick with AI, Write captions,
//   Draft email, and Fill with AI to do whichever are missing in one go
// - Save as Zip… or Save to Folder…
// AI goes through the usual consent and Pro checks (withAiConsent). Nothing
// the AI writes here is stored: it goes into the file the user saves.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import * as tauri from "../../lib/tauri";
import { aiErrorClass, friendlyAiError, isNoProviderError, withAiConsent } from "../../lib/ai";
import { useCapabilities, useIsPro } from "../../lib/build";
import { proSuffix } from "../../lib/pro";
import { guideTitle } from "../../lib/recordingKind";
import { AiSetupNotice, useAiStatus } from "../AiSetupNotice";
import {
    AI_PICKS,
    baseName,
    clock,
    contentsLine,
    defaultInclude,
    defaultSelection,
    hasAiParts,
    isTranscriptChanged,
    mergeCaptions,
    orderByTime,
    plainError,
    picturesNoun,
    uncaptioned,
    type DraftItem,
    type ExportDraft,
    type Highlight,
    type Include,
    type IncludeKey,
} from "../../lib/exportLogic";
import "./ExportSheet.css";

interface Caption {
    id: string;
    caption: string;
}

interface Email {
    subject: string;
    body: string;
}

const api = {
    draft: (meetingId: string) => invoke<ExportDraft>("get_export_draft", { meetingId }),
    image: (meetingId: string, itemId: string, small: boolean) => invoke<string>("get_export_image", { meetingId, itemId, small }),
    highlights: (meetingId: string, basis: string) =>
        withAiConsent(() => invoke<Highlight[]>("export_highlights", { meetingId, basis }), "ai"),
    captions: (meetingId: string, itemIds: string[], pick: number | null, basis: string) =>
        withAiConsent(() => invoke<Caption[]>("export_captions", { meetingId, itemIds, pick, basis }), "ai"),
    email: (meetingId: string) =>
        withAiConsent(() => invoke<Email & { to: string[] }>("draft_followup_email", { meetingId }), "follow_up"),
    save: (meetingId: string, request: unknown, destination: "zip" | "folder") =>
        invoke<string | null>("save_export_package", { meetingId, request, destination }),
    reveal: (path: string) => invoke<void>("reveal_export", { path }),
};

type Busy = "notes" | "highlights" | "captions" | "pick" | "email" | "fill" | "zip" | "folder";
type Filter = "all" | "picked" | "screen" | "photo";

/** Error text for a failed AI tool: never a raw machine prefix. */
function aiFailure(e: unknown): string {
    switch (aiErrorClass(e)) {
        case "pro_required":
            return "The AI tools are part of noFriction Pro. Exporting stays free.";
        case "consent_required":
            return "The AI tools need your permission to send this recording to your AI endpoint. Try again and choose Allow.";
        default:
            return friendlyAiError(e);
    }
}

/** One "What goes in" row: a checkbox, what it is, and its AI tool. */
function Row({
    on,
    onToggle,
    title,
    detail,
    action,
    disabled,
    children,
}: {
    on: boolean;
    onToggle: () => void;
    title: string;
    detail: string;
    action?: React.ReactNode;
    disabled?: boolean;
    children?: React.ReactNode;
}) {
    const checked = on && !disabled;
    return (
        <li className={`xp-row ${checked ? "on" : ""}`}>
            <label className="xp-row__main">
                <input type="checkbox" checked={checked} disabled={disabled} onChange={onToggle} />
                <span className="xp-row__text">
                    <span className="xp-row__title">{title}</span>
                    <span className="xp-row__detail">{detail}</span>
                </span>
            </label>
            {action && <div className="xp-row__action">{action}</div>}
            {children}
        </li>
    );
}

export function ExportSheet({ meetingId, onClose }: { meetingId: string; onClose: () => void }) {
    const [draft, setDraft] = useState<ExportDraft | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [include, setInclude] = useState<Include | null>(null);
    const [picked, setPicked] = useState<Set<string>>(new Set());
    const [captions, setCaptions] = useState<Record<string, string>>({});
    /** Captions the AI wrote (and the user hasn't edited) */
    const [aiCaptioned, setAiCaptioned] = useState<Set<string>>(new Set());
    /** The transcript the sheet's AI parts are made from (draft.fingerprint) */
    const [basis, setBasis] = useState("");
    // The latest values, for code that runs after an await
    const captionsRef = useRef(captions);
    captionsRef.current = captions;
    const aiCaptionedRef = useRef(aiCaptioned);
    aiCaptionedRef.current = aiCaptioned;
    const [highlights, setHighlights] = useState<Highlight[]>([]);
    const [email, setEmail] = useState<Email | null>(null);
    const [thumbs, setThumbs] = useState<Record<string, string>>({});
    const [filter, setFilter] = useState<Filter>("all");
    const [preview, setPreview] = useState<{ item: DraftItem; src: string | null } | null>(null);
    const [busy, setBusy] = useState<Busy | null>(null);
    const [progress, setProgress] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [needsAi, setNeedsAi] = useState(false);
    const [saved, setSaved] = useState<string | null>(null);
    const { configured } = useAiStatus();
    const caps = useCapabilities();
    const isPro = useIsPro();
    const closeRef = useRef<HTMLButtonElement>(null);
    const picsRef = useRef<HTMLElement>(null);
    const alive = useRef(true);

    const pro = proSuffix(caps?.pro_gating, isPro);
    const aiOff = configured === false || needsAi;

    const load = useCallback(async () => {
        setLoadError(null);
        try {
            const d = await api.draft(meetingId);
            if (!alive.current) return;
            setDraft(d);
            return d;
        } catch (e) {
            if (alive.current) setLoadError(String(e));
        }
    }, [meetingId]);

    // First load: draft, then the default ticks and picks
    useEffect(() => {
        alive.current = true;
        load().then((d) => {
            if (!d) return;
            setBasis(d.fingerprint);
            setInclude(defaultInclude(d));
            setPicked(new Set(defaultSelection(d.items)));
        });
        closeRef.current?.focus();
        return () => {
            alive.current = false;
        };
    }, [load]);

    // Thumbnails, four at a time
    useEffect(() => {
        if (!draft) return;
        let stop = false;
        const queue = draft.items.map((i) => i.id);
        const worker = async () => {
            while (!stop && queue.length) {
                const id = queue.shift()!;
                try {
                    const b64 = await api.image(meetingId, id, true);
                    if (!stop) setThumbs((t) => ({ ...t, [id]: `data:image/jpeg;base64,${b64}` }));
                } catch {
                    // A picture that can't be read shows as a blank tile
                }
            }
        };
        for (let i = 0; i < 4; i++) worker();
        return () => {
            stop = true;
        };
    }, [draft?.meeting_id, draft?.items.length]); // eslint-disable-line react-hooks/exhaustive-deps

    // Esc closes the preview, then the sheet (not while saving)
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (e.key !== "Escape") return;
            if (preview) setPreview(null);
            else if (busy !== "zip" && busy !== "folder") onClose();
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [preview, busy, onClose]);

    const items = draft?.items ?? [];
    const pickedIds = useMemo(() => orderByTime(items, picked), [items, picked]);
    const pickedItems = useMemo(() => pickedIds.map((id) => items.find((i) => i.id === id)!).filter(Boolean), [pickedIds, items]);
    const bothKinds = items.some((i) => i.source === "photo") && items.some((i) => i.source === "screen");
    const shown = items.filter((i) => (filter === "all" ? true : filter === "picked" ? picked.has(i.id) : i.source === filter));
    const captionCount = pickedIds.filter((id) => (captions[id] ?? "").trim()).length;

    const run = async (what: Busy, work: () => Promise<void>) => {
        setBusy(what);
        setError(null);
        setNeedsAi(false);
        setSaved(null);
        try {
            await work();
        } catch (e) {
            if (isTranscriptChanged(e)) await transcriptChanged(e);
            else if (isNoProviderError(e)) setNeedsAi(true);
            else setError(aiFailure(e));
        } finally {
            if (alive.current) {
                setBusy(null);
                setProgress(null);
            }
        }
    };

    /** Edited while the sheet was open (a Delete or Strike, here or over
     *  Sync): the AI parts may quote removed words, so they go. */
    const transcriptChanged = async (e: unknown) => {
        const d = await load();
        if (d) setBasis(d.fingerprint);
        setHighlights([]);
        setEmail(null);
        const ids = aiCaptionedRef.current;
        setCaptions((c) => {
            const next = { ...c };
            for (const id of ids) delete next[id];
            return next;
        });
        setAiCaptioned(new Set());
        setError(plainError(e));
    };

    const tick = (k: IncludeKey, v?: boolean) => setInclude((inc) => (inc ? { ...inc, [k]: v ?? !inc[k] } : inc));

    // ─── AI tools ────────────────────────────────────────────────────────
    const makeNotes = async () => {
        await tauri.generateMeetingReport(meetingId);
        await load();
        tick("notes", true);
    };
    const findHighlights = async () => {
        const hs = await api.highlights(meetingId, basis);
        if (!alive.current) return;
        setHighlights(hs);
        tick("highlights", true);
    };
    const writeCaptions = async (ids: string[]) => {
        if (!ids.length) return;
        const cs = await api.captions(meetingId, ids, null, basis);
        if (!alive.current) return;
        applyCaptions(cs);
    };
    /** Typed captions win: the AI fills only empty ones (typing goes on while it works). */
    const applyCaptions = (cs: Caption[]) => {
        // Decided against the captions as they are now (typing went on while
        // the AI worked), then applied with functional updates
        const filled = [...mergeCaptions(captionsRef.current, new Set(), cs).aiIds];
        const byId = new Map(cs.map((c) => [c.id, c.caption]));
        setCaptions((prev) => {
            const next = { ...prev };
            for (const id of filled) if (!(prev[id] ?? "").trim()) next[id] = byId.get(id)!;
            return next;
        });
        setAiCaptioned((ids) => new Set([...ids, ...filled]));
    };
    const pickWithAi = async () => {
        const cs = await api.captions(meetingId, [], AI_PICKS, basis);
        if (!alive.current || !cs.length) return;
        setPicked(new Set(cs.map((c) => c.id)));
        applyCaptions(cs);
        setFilter("picked");
        tick("screens", true);
    };
    const draftEmail = async () => {
        const e = await api.email(meetingId);
        if (!alive.current) return;
        setEmail({ subject: e.subject, body: e.body });
        tick("email", true);
    };

    /** Everything that's missing, one after another. */
    const fillWithAi = () =>
        run("fill", async () => {
            if (!draft) return;
            const steps: { label: string; go: () => Promise<void> }[] = [];
            if (!draft.notes && draft.transcript_lines > 0) steps.push({ label: "Making notes", go: makeNotes });
            if (!highlights.length && draft.transcript_lines > 0) steps.push({ label: "Finding highlights", go: findHighlights });
            const missing = uncaptioned(pickedIds, captions);
            if (missing.length) steps.push({ label: "Writing captions", go: () => writeCaptions(missing) });
            if (draft.kind === "meeting" && !email && draft.transcript_lines > 0) steps.push({ label: "Drafting the follow-up email", go: draftEmail });
            if (!steps.length) {
                setProgress(null);
                return;
            }
            for (const [i, s] of steps.entries()) {
                if (!alive.current) return;
                setProgress(`${s.label}… (${i + 1} of ${steps.length})`);
                await s.go();
            }
        });

    // ─── Saving ──────────────────────────────────────────────────────────
    const save = (destination: "zip" | "folder") =>
        run(destination, async () => {
            if (!draft || !include) return;
            const hasEmail = !!email?.body.trim();
            const request = {
                include,
                items: pickedIds.map((id) => ({ id, caption: (captions[id] ?? "").trim() || null, ai: aiCaptioned.has(id) })),
                highlights: highlights.filter((h) => h.label.trim()),
                email: hasEmail ? email : null,
                hasAi: hasAiParts(draft, include, { highlights: highlights.length, captions: captionCount, hasEmail }),
                basis,
            };
            try {
                const path = await api.save(meetingId, request, destination);
                if (path && alive.current) setSaved(path);
            } catch (e) {
                if (!alive.current) return;
                if (isTranscriptChanged(e)) await transcriptChanged(e);
                // Not an AI error: say what happened in plain words
                else setError(`Couldn't save the export. ${plainError(e)}`);
            }
        });

    const openPreview = async (item: DraftItem) => {
        setPreview({ item, src: null });
        try {
            const b64 = await api.image(meetingId, item.id, false);
            setPreview((p) => (p && p.item.id === item.id ? { item, src: `data:image/jpeg;base64,${b64}` } : p));
        } catch {
            setPreview((p) => (p && p.item.id === item.id ? { item, src: thumbs[item.id] ?? null } : p));
        }
    };

    const togglePick = (id: string) =>
        setPicked((s) => {
            const n = new Set(s);
            if (n.has(id)) n.delete(id);
            else n.add(id);
            return n;
        });

    // ─── Render ──────────────────────────────────────────────────────────
    if (loadError || !draft || !include) {
        return (
            <div className="modal-overlay xp-overlay" role="dialog" aria-modal="true" aria-labelledby="xp-title">
                <div className="xp-sheet xp-sheet--small">
                    <div className="xp-head">
                        <h2 id="xp-title">Export recording</h2>
                        <button ref={closeRef} className="xp-close" onClick={onClose} aria-label="Close">×</button>
                    </div>
                    {loadError ? (
                        <p className="xp-error" role="alert">Couldn't read this recording: {loadError}</p>
                    ) : (
                        <div className="loading-spinner" style={{ margin: "32px auto" }} />
                    )}
                </div>
            </div>
        );
    }

    const hasEmail = !!email?.body.trim();
    const noTranscript = draft.transcript_lines === 0;
    const aiBusy = busy !== null;
    const aiDisabled = aiBusy || aiOff;
    const kindGuide = guideTitle(draft.kind === "class" ? "class" : draft.kind === "personal" ? "personal" : "meeting");
    const notesWhen = draft.notes ? new Date(draft.notes.generated_at).toLocaleString([], { dateStyle: "medium", timeStyle: "short" }) : null;
    const summary = contentsLine(draft, include, { picked: pickedItems, highlights: highlights.length, hasEmail });

    return (
        <div className="modal-overlay xp-overlay" role="dialog" aria-modal="true" aria-labelledby="xp-title">
            <div className="xp-sheet">
                <div className="xp-head">
                    <div>
                        <h2 id="xp-title">Export recording</h2>
                        <p className="xp-sub">
                            {draft.title} · {draft.when}
                            {draft.duration ? ` · ${draft.duration}` : ""}
                        </p>
                    </div>
                    <button ref={closeRef} className="xp-close" onClick={onClose} aria-label="Close" disabled={busy === "zip" || busy === "folder"}>
                        ×
                    </button>
                </div>

                <div className="xp-body">
                    {aiOff && <AiSetupNotice feature="The AI tools" compact />}
                    {error && (
                        <p className="xp-error" role="alert">
                            {error}
                        </p>
                    )}

                    <section aria-labelledby="xp-what">
                        <h3 id="xp-what" className="xp-h">What goes in</h3>
                        <ul className="xp-rows">
                            <Row
                                on={include.notes}
                                onToggle={() => tick("notes")}
                                title="Summary and notes"
                                disabled={!draft.notes}
                                detail={
                                    draft.notes
                                        ? `Made ${notesWhen}${draft.notes.stale ? ", before an edit" : ""}`
                                        : noTranscript
                                          ? "No transcript to make notes from"
                                          : "Not made yet. The AI writes a summary, decisions and action items."
                                }
                                action={
                                    !noTranscript && (
                                        <button className="xp-btn" disabled={aiDisabled} onClick={() => run("notes", makeNotes)}>
                                            {busy === "notes" ? "Making notes…" : draft.notes ? "Make again" : `Make notes${pro}`}
                                        </button>
                                    )
                                }
                            />
                            <Row
                                on={include.highlights}
                                onToggle={() => tick("highlights")}
                                title="Highlights"
                                disabled={highlights.length === 0}
                                detail={
                                    highlights.length
                                        ? `${highlights.length} key moments, quoted from the transcript`
                                        : "The AI picks the key moments and quotes what was said."
                                }
                                action={
                                    !noTranscript && (
                                        <button className="xp-btn" disabled={aiDisabled} onClick={() => run("highlights", findHighlights)}>
                                            {busy === "highlights" ? "Finding…" : highlights.length ? "Find again" : `Find highlights${pro}`}
                                        </button>
                                    )
                                }
                            >
                                {highlights.length > 0 && include.highlights && (
                                    <ol className="xp-highlights">
                                        {highlights.map((h, i) => (
                                            <li key={`${h.ms}-${i}`}>
                                                <span className="xp-time">{clock(h.ms)}</span>
                                                <div className="xp-highlight">
                                                    <input
                                                        className="xp-input"
                                                        value={h.label}
                                                        aria-label={`Highlight at ${clock(h.ms)}`}
                                                        onChange={(e) =>
                                                            setHighlights((hs) => hs.map((x, j) => (j === i ? { ...x, label: e.target.value } : x)))
                                                        }
                                                    />
                                                    {h.quote && <q className="xp-quote">{h.quote}</q>}
                                                </div>
                                                <button
                                                    className="xp-x"
                                                    aria-label={`Remove the highlight at ${clock(h.ms)}`}
                                                    onClick={() => setHighlights((hs) => hs.filter((_, j) => j !== i))}
                                                >
                                                    ×
                                                </button>
                                            </li>
                                        ))}
                                    </ol>
                                )}
                            </Row>
                            {items.length > 0 && (
                                <Row
                                    on={include.screens}
                                    onToggle={() => tick("screens")}
                                    title={picturesNoun(items)}
                                    disabled={pickedIds.length === 0}
                                    detail={`${pickedIds.length} of ${items.length} picked${captionCount ? `, ${captionCount} with captions` : ""}`}
                                    action={
                                        <button className="xp-btn ghost" onClick={() => picsRef.current?.scrollIntoView({ behavior: "smooth", block: "start" })}>
                                            Pick…
                                        </button>
                                    }
                                />
                            )}
                            {draft.marks > 0 && <Row on={include.marks} onToggle={() => tick("marks")} title="Marked moments" detail={`${draft.marks} marked`} />}
                            {draft.links > 0 && <Row on={include.links} onToggle={() => tick("links")} title="Links" detail={`${draft.links} said, seen or added`} />}
                            {draft.kind === "meeting" && (
                                <Row
                                    on={include.email}
                                    onToggle={() => tick("email")}
                                    title="Follow-up email"
                                    disabled={!hasEmail}
                                    detail={hasEmail ? "Drafted. Edit it below." : "The AI drafts a recap email from the transcript."}
                                    action={
                                        !noTranscript && (
                                            <button className="xp-btn" disabled={aiDisabled} onClick={() => run("email", draftEmail)}>
                                                {busy === "email" ? "Drafting…" : hasEmail ? "Draft again" : `Draft email${pro}`}
                                            </button>
                                        )
                                    }
                                >
                                    {email && include.email && (
                                        <div className="xp-email">
                                            <input
                                                className="xp-input"
                                                aria-label="Subject"
                                                value={email.subject}
                                                onChange={(e) => setEmail({ ...email, subject: e.target.value })}
                                            />
                                            <textarea
                                                className="xp-input xp-textarea"
                                                aria-label="Message"
                                                rows={6}
                                                value={email.body}
                                                onChange={(e) => setEmail({ ...email, body: e.target.value })}
                                            />
                                        </div>
                                    )}
                                </Row>
                            )}
                            {draft.guide_parts.length > 0 && (
                                <Row
                                    on={include.guide}
                                    onToggle={() => tick("guide")}
                                    title={kindGuide}
                                    detail={
                                        draft.guide_parts.includes("flashcards")
                                            ? `${kindGuide}.md and flashcards.csv, as made in ${kindGuide}`
                                            : `${kindGuide}.md, as made in ${kindGuide}`
                                    }
                                />
                            )}
                            {draft.transcript_lines > 0 && (
                                <Row on={include.transcript} onToggle={() => tick("transcript")} title="Transcript" detail={`${draft.transcript_lines} lines, as transcript.txt`} />
                            )}
                        </ul>
                    </section>

                    {items.length > 0 && (
                        <section aria-labelledby="xp-pics" className="xp-pics" ref={picsRef}>
                            <div className="xp-pics__bar">
                                <h3 id="xp-pics" className="xp-h">
                                    {picturesNoun(items)}
                                </h3>
                                <div className="xp-seg" role="group" aria-label="Show">
                                    {(
                                        [
                                            ["all", `All ${items.length}`],
                                            ["picked", `Picked ${pickedIds.length}`],
                                            ...(bothKinds
                                                ? ([
                                                      ["screen", "Screens"],
                                                      ["photo", "Photos"],
                                                  ] as const)
                                                : []),
                                        ] as const
                                    ).map(([f, label]) => (
                                        <button key={f} className={filter === f ? "on" : ""} aria-pressed={filter === f} onClick={() => setFilter(f as Filter)}>
                                            {label}
                                        </button>
                                    ))}
                                </div>
                                <span className="xp-spacer" />
                                <button className="xp-btn ghost" onClick={() => setPicked(new Set(items.map((i) => i.id)))} disabled={aiBusy}>
                                    Pick all
                                </button>
                                <button className="xp-btn ghost" onClick={() => setPicked(new Set())} disabled={aiBusy}>
                                    Clear
                                </button>
                                <button className="xp-btn" onClick={() => run("pick", pickWithAi)} disabled={aiDisabled || noTranscript}>
                                    {busy === "pick" ? "Picking…" : `Pick with AI${pro}`}
                                </button>
                                <button
                                    className="xp-btn"
                                    onClick={() => run("captions", () => writeCaptions(uncaptioned(pickedIds, captions)))}
                                    disabled={aiDisabled || uncaptioned(pickedIds, captions).length === 0}
                                    title="Captions for the picked ones that don't have one, from what was said at that moment"
                                >
                                    {busy === "captions" ? "Writing…" : `Write captions${pro}`}
                                </button>
                            </div>
                            {shown.length === 0 ? (
                                <p className="xp-muted">Nothing picked yet. Click the ones to keep.</p>
                            ) : (
                                <ul className="xp-grid">
                                    {shown.map((it) => {
                                        const on = picked.has(it.id);
                                        return (
                                            <li key={it.id} className={`xp-tile ${on ? "on" : ""}`}>
                                                <button
                                                    className="xp-tile__img"
                                                    aria-pressed={on}
                                                    aria-label={`${it.source === "photo" ? "Photo" : "Screen"} at ${clock(it.ms)}${it.label ? `, ${it.label}` : ""}`}
                                                    onClick={() => togglePick(it.id)}
                                                    onDoubleClick={() => openPreview(it)}
                                                >
                                                    {thumbs[it.id] ? <img src={thumbs[it.id]} alt="" /> : <span className="xp-tile__blank" />}
                                                    <span className="xp-check" aria-hidden="true">
                                                        {on ? "✓" : ""}
                                                    </span>
                                                    <span className="xp-tile__time">
                                                        {it.source === "photo" ? "Photo · " : ""}
                                                        {clock(it.ms)}
                                                    </span>
                                                </button>
                                                <button className="xp-zoom" aria-label={`Show larger, ${clock(it.ms)}`} onClick={() => openPreview(it)}>
                                                    ⤢
                                                </button>
                                                {it.label && <span className="xp-tile__label" title={it.label}>{it.label}</span>}
                                                {on && (
                                                    <input
                                                        className="xp-input xp-caption"
                                                        placeholder="Caption (optional)"
                                                        aria-label={`Caption for ${clock(it.ms)}`}
                                                        value={captions[it.id] ?? ""}
                                                        onChange={(e) => {
                                                            const v = e.target.value;
                                                            setCaptions((c) => ({ ...c, [it.id]: v }));
                                                            // Edited: the caption is the user's now
                                                            setAiCaptioned((s) => {
                                                                if (!s.has(it.id)) return s;
                                                                const n = new Set(s);
                                                                n.delete(it.id);
                                                                return n;
                                                            });
                                                        }}
                                                    />
                                                )}
                                            </li>
                                        );
                                    })}
                                </ul>
                            )}
                        </section>
                    )}
                </div>

                <div className="xp-foot">
                    <div className="xp-foot__info" aria-live="polite">
                        {saved ? (
                            <>
                                <span className="xp-saved">Saved {baseName(saved)}</span>
                                <button className="xp-link" onClick={() => api.reveal(saved).catch((e) => setError(String(e)))}>
                                    Show in Finder
                                </button>
                            </>
                        ) : progress ? (
                            <span>{progress}</span>
                        ) : (
                            <>
                                <span className="xp-contents">{summary}</span>
                                <span className="xp-muted">
                                    As Markdown and a web page you can print to PDF, in “{draft.folder}”
                                </span>
                            </>
                        )}
                    </div>
                    <button className="xp-btn" onClick={fillWithAi} disabled={aiDisabled} title="Makes whatever is missing: notes, highlights, captions and (for a meeting) the follow-up email">
                        {busy === "fill" ? "Filling…" : `✦ Fill with AI${pro}`}
                    </button>
                    <button className="xp-btn" onClick={() => save("folder")} disabled={aiBusy}>
                        {busy === "folder" ? "Saving…" : "Save to Folder…"}
                    </button>
                    <button className="xp-btn primary" onClick={() => save("zip")} disabled={aiBusy}>
                        {busy === "zip" ? "Saving…" : "Save as Zip…"}
                    </button>
                </div>
            </div>

            {preview && (
                <div className="xp-preview" role="dialog" aria-modal="true" aria-label={`Preview at ${clock(preview.item.ms)}`} onClick={() => setPreview(null)}>
                    <figure onClick={(e) => e.stopPropagation()}>
                        {preview.src ? <img src={preview.src} alt="" /> : <div className="loading-spinner" />}
                        <figcaption>
                            <span className="xp-time">{clock(preview.item.ms)}</span>
                            {captions[preview.item.id] || preview.item.label || ""}
                        </figcaption>
                        <div className="xp-preview__actions">
                            <button className="xp-btn" onClick={() => togglePick(preview.item.id)}>
                                {picked.has(preview.item.id) ? "Unpick" : "Pick"}
                            </button>
                            <button className="xp-btn primary" onClick={() => setPreview(null)}>
                                Done
                            </button>
                        </div>
                    </figure>
                </div>
            )}
        </div>
    );
}
