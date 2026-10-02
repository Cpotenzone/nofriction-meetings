// noFriction Meetings - Capture Bar
//
// The one place that answers "what is this app capturing right now?"
// Status on the left, the chosen sources in the middle, and two actions:
// choose sources, and snap them now. Choosing a window is a click on its
// picture — no settings page, no IDs.

import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
    listCaptureSources,
    getCaptureTargets,
    setCaptureTargets,
    snapCaptureTarget,
    targetKey,
    type CaptureSource,
    type CaptureTarget,
    type FrameCapturedEvent,
} from "../lib/tauri";
import { CameraIcon, CheckIcon, DisplayIcon, WindowIcon } from "./icons";
import "./CaptureBar.css";

interface TranscriptionStatus {
    connected: boolean;
    provider: string;
    error: string | null;
}

interface CaptureBarProps {
    isRecording: boolean;
    sttStatus: TranscriptionStatus | null;
    audioWarning?: string | null;
}

export function CaptureBar({ isRecording, sttStatus, audioWarning }: CaptureBarProps) {
    const [targets, setTargets] = useState<CaptureTarget[]>([]);
    const [labels, setLabels] = useState<Record<string, string>>({});
    const [pickerOpen, setPickerOpen] = useState(false);
    const [flash, setFlash] = useState<string | null>(null);
    // The calendar event this recording was matched to, if any
    const [event, setEvent] = useState<{ title: string; people: number } | null>(null);

    useEffect(() => {
        let off: (() => void) | null = null;
        let disposed = false;
        listen<{ event_title: string; attendee_names: string[] }>("calendar_match", (e) =>
            setEvent({ title: e.payload.event_title, people: e.payload.attendee_names.length })
        ).then((fn) => (disposed ? fn() : (off = fn)));
        return () => {
            disposed = true;
            off?.();
        };
    }, []);

    useEffect(() => {
        if (!isRecording) setEvent(null);
    }, [isRecording]);

    useEffect(() => {
        getCaptureTargets().then(setTargets).catch(() => {});
        // Resolve labels for persisted targets without paying for thumbnails
        listCaptureSources(false)
            .then((s) => setLabels(Object.fromEntries(s.map((x) => [targetKey(x.target), x.title]))))
            .catch(() => {});
    }, []);

    const snap = useCallback(async () => {
        const list: CaptureTarget[] = targets.length ? targets : [];
        try {
            if (list.length === 0) {
                const sources = await listCaptureSources(false);
                const main = sources.find((s) => s.target.kind === "display" && s.is_primary)
                    ?? sources.find((s) => s.target.kind === "display");
                if (main) list.push(main.target);
            }
            const results = await Promise.all(list.map((t) => snapCaptureTarget(t)));
            setFlash(results.length === 1 ? `Saved ${results[0].label}` : `Saved ${results.length} snapshots`);
        } catch (e) {
            setFlash(`Snapshot failed — ${String(e)}`);
        }
        setTimeout(() => setFlash(null), 2200);
    }, [targets]);

    // ⌘⇧S — snap without reaching for the mouse
    useEffect(() => {
        const onKey = (e: KeyboardEvent) => {
            if (e.metaKey && e.shiftKey && e.key.toLowerCase() === "s") {
                e.preventDefault();
                snap();
            }
        };
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [snap]);

    const status = !isRecording
        ? { tone: "idle", text: "Not recording" }
        : sttStatus && !sttStatus.connected
            ? { tone: "warn", text: sttStatus.error ? `Transcription stopped — ${sttStatus.error}` : "Transcription stopped" }
            : audioWarning
                ? { tone: "warn", text: audioWarning }
                : {
                      tone: "live",
                      text: event
                          ? `Recording · ${event.title}${event.people ? ` · ${event.people} ${event.people === 1 ? "person" : "people"}` : ""}`
                          : "Recording · transcribing on this Mac",
                  };

    const chips = targets.length
        ? targets.map((t) => ({ key: targetKey(t), kind: t.kind, label: labels[targetKey(t)] ?? (t.kind === "display" ? "Display" : "Window") }))
        : [{ key: "default", kind: "display" as const, label: "Main display" }];

    return (
        <>
            <div className="cbar" role="toolbar" aria-label="Capture">
                <div className={`cbar__status is-${status.tone}`}>
                    <span className="cbar__dot" aria-hidden />
                    <span className="cbar__status-text" title={status.text}>{status.text}</span>
                </div>

                <button className="cbar__sources" onClick={() => setPickerOpen(true)} type="button" title="Choose what to capture">
                    {chips.slice(0, 3).map((c) => (
                        <span key={c.key} className="cbar__chip">
                            {c.kind === "display" ? <DisplayIcon size={12} /> : <WindowIcon size={12} />}
                            <span className="cbar__chip-label">{c.label}</span>
                        </span>
                    ))}
                    {chips.length > 3 && <span className="cbar__chip">+{chips.length - 3}</span>}
                    <span className="cbar__edit">Change</span>
                </button>

                <button className="cbar__snap" onClick={snap} type="button" title="Snap now (⌘⇧S)">
                    <CameraIcon size={15} />
                    <span>Snap</span>
                </button>

                {flash && <div className="cbar__flash" role="status">{flash}</div>}
            </div>

            {pickerOpen && (
                <SourcePicker
                    selected={targets}
                    onChange={async (next, nextLabels) => {
                        setTargets(next);
                        setLabels((l) => ({ ...l, ...nextLabels }));
                        await setCaptureTargets(next).catch(() => {});
                    }}
                    onClose={() => setPickerOpen(false)}
                />
            )}
        </>
    );
}

// ── Source picker sheet ─────────────────────────────────────────────────

interface SourcePickerProps {
    selected: CaptureTarget[];
    onChange: (targets: CaptureTarget[], labels: Record<string, string>) => void;
    onClose: () => void;
}

function SourcePicker({ selected, onChange, onClose }: SourcePickerProps) {
    const [sources, setSources] = useState<CaptureSource[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    const selectedKeys = useMemo(() => new Set(selected.map(targetKey)), [selected]);

    const load = useCallback(() => {
        setError(null);
        listCaptureSources(true)
            .then(setSources)
            .catch((e) => setError(String(e)));
    }, []);

    useEffect(load, [load]);

    useEffect(() => {
        const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
        window.addEventListener("keydown", onKey);
        return () => window.removeEventListener("keydown", onKey);
    }, [onClose]);

    const toggle = (s: CaptureSource) => {
        const key = targetKey(s.target);
        const next = selectedKeys.has(key)
            ? selected.filter((t) => targetKey(t) !== key)
            : [...selected, s.target];
        onChange(next, { [key]: s.title });
    };

    const displays = sources?.filter((s) => s.target.kind === "display") ?? [];
    const windows = sources?.filter((s) => s.target.kind === "window") ?? [];

    return (
        <div className="spick__scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
            <div className="spick" role="dialog" aria-modal="true" aria-labelledby="spick-title">
                <header className="spick__head">
                    <div>
                        <h2 id="spick-title">What should be captured?</h2>
                        <p>Pick any screens or windows. Captured about once a second, and only when they change.</p>
                    </div>
                    <div className="spick__head-actions">
                        <button className="spick__ghost" onClick={load} type="button">Refresh</button>
                        <button className="spick__done" onClick={onClose} type="button">Done</button>
                    </div>
                </header>

                {error && <p className="spick__error">Couldn't list sources — {error}. Check Screen Recording permission in System Settings.</p>}
                {!sources && !error && <p className="spick__loading">Looking at your screens…</p>}

                {sources && (
                    <div className="spick__body">
                        <SourceGroup title="Screens" items={displays} selectedKeys={selectedKeys} onToggle={toggle} />
                        <SourceGroup title="Windows" items={windows} selectedKeys={selectedKeys} onToggle={toggle} />
                        {selected.length === 0 && (
                            <p className="spick__note">Nothing selected — the main display is captured.</p>
                        )}
                    </div>
                )}
            </div>
        </div>
    );
}

function SourceGroup({
    title,
    items,
    selectedKeys,
    onToggle,
}: {
    title: string;
    items: CaptureSource[];
    selectedKeys: Set<string>;
    onToggle: (s: CaptureSource) => void;
}) {
    if (items.length === 0) return null;
    return (
        <section className="spick__group">
            <h3>{title}</h3>
            <div className="spick__grid">
                {items.map((s) => {
                    const key = targetKey(s.target);
                    const on = selectedKeys.has(key);
                    return (
                        <button
                            key={key}
                            className={`spick__card ${on ? "is-on" : ""}`}
                            onClick={() => onToggle(s)}
                            type="button"
                            aria-pressed={on}
                        >
                            <div className="spick__thumb">
                                {s.thumbnail ? <img src={s.thumbnail} alt="" /> : (s.target.kind === "display" ? <DisplayIcon size={28} strokeWidth={1.5} /> : <WindowIcon size={28} strokeWidth={1.5} />)}
                                {on && <span className="spick__check"><CheckIcon size={13} strokeWidth={3} /></span>}
                            </div>
                            <div className="spick__meta">
                                <span className="spick__title">{s.title}</span>
                                <span className="spick__sub">
                                    {s.target.kind === "display"
                                        ? `${s.width}×${s.height}${s.is_primary ? " · Main" : ""}`
                                        : s.app_name}
                                </span>
                            </div>
                        </button>
                    );
                })}
            </div>
        </section>
    );
}

// ── Filmstrip of recent captures ────────────────────────────────────────

export function CaptureFilmstrip({ isRecording }: { isRecording: boolean }) {
    const [frames, setFrames] = useState<FrameCapturedEvent[]>([]);

    useEffect(() => {
        let off: (() => void) | null = null;
        let disposed = false;
        listen<FrameCapturedEvent>("frame_captured", (e) => {
            setFrames((prev) => [e.payload, ...prev].slice(0, 24));
        }).then((fn) => (disposed ? fn() : (off = fn)));
        return () => {
            disposed = true;
            off?.();
        };
    }, []);

    useEffect(() => {
        if (isRecording) setFrames([]);
    }, [isRecording]);

    return (
        <section className="film">
            <header className="film__head">
                <h3>Captures</h3>
                <span>{frames.length ? `${frames.length} recent` : ""}</span>
            </header>
            {frames.length === 0 ? (
                <p className="film__empty">
                    {isRecording ? "A new picture is saved whenever a captured screen changes." : "Screens you capture appear here."}
                </p>
            ) : (
                <div className="film__strip">
                    {frames.map((f) => (
                        <figure key={`${f.path}`} className={`film__frame ${f.manual ? "is-manual" : ""}`} title={f.label}>
                            <img src={convertFileSrc(f.path)} alt={f.label} loading="lazy" />
                            <figcaption>
                                <span className="film__label">{f.label}</span>
                                <time>{new Date(f.timestamp).toLocaleTimeString([], { hour: "numeric", minute: "2-digit", second: "2-digit" })}</time>
                            </figcaption>
                        </figure>
                    ))}
                </div>
            )}
        </section>
    );
}
