// noFriction Meetings - Links for one recording (Recordings → Links)
// Sites said in the transcript, seen on screen, and references the user
// added (syllabus, a reading, slides). See docs/LINKS.md.
//
// Nothing here goes to the network: no page titles, no favicons. Open hands
// an http(s) link to the default browser, and only when the user asks.

import { useCallback, useEffect, useState } from "react";
import * as tauri from "../lib/tauri";
import { useCapabilities } from "../lib/build";
import {
    SOURCE_LABEL,
    countsLabel,
    displayLink,
    formatOffset,
    isOpenableUrl,
    linksMarkdown,
    referenceUrl,
    type MeetingLink,
    type MeetingLinks,
} from "../lib/meetingLinks";
import {
    addMeetingReference,
    deleteMeetingReference,
    getBrowserUrlCapture,
    hideMeetingLink,
    listMeetingLinks,
    openMeetingLink,
    setBrowserUrlCapture,
    updateMeetingReference,
} from "../lib/meetingLinksApi";
import ErrorState from "./ErrorState";
import "./MeetingLinksPanel.css";

interface MeetingLinksPanelProps {
    meetingId: string;
    /** Show this moment (ms from the meeting start) in Rewind */
    onJump?: (ms: number) => void;
}

interface Form {
    /** null: a new reference */
    id: string | null;
    url: string;
    title: string;
    note: string;
}

const EMPTY_FORM: Form = { id: null, url: "", title: "", note: "" };

export function MeetingLinksPanel({ meetingId, onJump }: MeetingLinksPanelProps) {
    const [data, setData] = useState<MeetingLinks | null>(null);
    const [meetingTitle, setMeetingTitle] = useState("");
    const [loading, setLoading] = useState(true);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [form, setForm] = useState<Form | null>(null);
    const [saving, setSaving] = useState(false);
    const [showHidden, setShowHidden] = useState(false);
    const [copied, setCopied] = useState(false);
    const [capture, setCapture] = useState<boolean | null>(null);
    const caps = useCapabilities();

    const load = useCallback(async () => {
        setLoading(true);
        setLoadError(null);
        try {
            const [links, meeting] = await Promise.all([
                listMeetingLinks(meetingId),
                tauri.getMeeting(meetingId).catch(() => null),
            ]);
            setData(links);
            setMeetingTitle(meeting?.title ?? "");
        } catch (e) {
            setLoadError(String(e));
        } finally {
            setLoading(false);
        }
    }, [meetingId]);

    useEffect(() => {
        setData(null);
        setForm(null);
        setError(null);
        setShowHidden(false);
        load();
    }, [load]);

    // DMG build only: the browser-address setting
    useEffect(() => {
        if (!caps?.accessibility_capture) return;
        getBrowserUrlCapture()
            .then(setCapture)
            .catch(() => setCapture(null));
    }, [caps?.accessibility_capture]);

    const act = async (what: string, fn: () => Promise<unknown>) => {
        setError(null);
        try {
            await fn();
        } catch (e) {
            setError(`${what}: ${e instanceof Error ? e.message : String(e)}`);
        }
    };

    const open = (l: MeetingLink) => act("Couldn't open the link", () => openMeetingLink(l.url));

    const hide = (l: MeetingLink, hidden: boolean) =>
        act(hidden ? "Couldn't hide the link" : "Couldn't show the link", async () => {
            await hideMeetingLink(meetingId, l.key, hidden);
            await load();
        });

    const remove = (l: MeetingLink) => {
        if (!l.reference_id) return;
        if (!confirm(`Remove "${l.title || displayLink(l)}" from this recording's references?`)) return;
        act("Couldn't remove the reference", async () => {
            await deleteMeetingReference(l.reference_id!);
            await load();
        });
    };

    const save = async () => {
        if (!form) return;
        const url = referenceUrl(form.url);
        if (!url) {
            setError("Enter a web address that starts with http:// or https:// (like https://example.com/syllabus).");
            return;
        }
        setSaving(true);
        await act("Couldn't save the reference", async () => {
            const title = form.title.trim() || null;
            const note = form.note.trim() || null;
            if (form.id) await updateMeetingReference(form.id, url, title, note);
            else await addMeetingReference(meetingId, url, title, note);
            setForm(null);
            await load();
        });
        setSaving(false);
    };

    const copyAll = () =>
        act("Couldn't copy", async () => {
            await navigator.clipboard.writeText(linksMarkdown(meetingTitle, data?.links ?? []));
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
        });

    const toggleCapture = (on: boolean) =>
        act("Couldn't save the setting", async () => {
            await setBrowserUrlCapture(on);
            setCapture(on);
        });

    if (loading && !data) {
        return (
            <div className="ml-panel ml-center">
                <div className="loading-spinner" />
            </div>
        );
    }

    if (loadError || !data) {
        return (
            <div className="ml-panel">
                <ErrorState title="Couldn't load the links" message="Try again in a moment." onRetry={load} />
            </div>
        );
    }

    const links = data.links;

    return (
        <div className="ml-panel">
            <div className="ml-toolbar">
                <h3 className="ml-heading">
                    Links <span className="ml-muted">{links.length}</span>
                </h3>
                <div className="ml-actions">
                    <button className="ml-btn primary" onClick={() => setForm({ ...EMPTY_FORM })} disabled={!!form}>
                        Add reference
                    </button>
                    <button className="ml-btn" onClick={copyAll} disabled={links.length === 0}>
                        {copied ? "Copied" : "Copy all as Markdown"}
                    </button>
                </div>
            </div>

            {error && (
                <p className="ml-error" role="alert">
                    {error}
                </p>
            )}

            {form && (
                <ReferenceForm
                    form={form}
                    saving={saving}
                    onChange={setForm}
                    onSave={save}
                    onCancel={() => {
                        setForm(null);
                        setError(null);
                    }}
                />
            )}

            {links.length === 0 && !form ? (
                <div className="ml-empty">
                    <h4>No links yet</h4>
                    <p>
                        Sites mentioned in the transcript ("example dot com") and addresses seen on screen show up here.
                        Add the agenda, the slides or a reading with <strong>Add reference</strong>.
                    </p>
                </div>
            ) : (
                <ul className="ml-list">
                    {links.map((l) => (
                        <LinkRow
                            key={l.reference_id ?? l.key}
                            link={l}
                            onJump={onJump}
                            onOpen={() => open(l)}
                            onHide={() => hide(l, true)}
                            onEdit={() =>
                                setForm({ id: l.reference_id, url: l.url, title: l.title ?? "", note: l.note ?? "" })
                            }
                            onDelete={() => remove(l)}
                        />
                    ))}
                </ul>
            )}

            {data.hidden.length > 0 && (
                <div className="ml-hidden">
                    <button className="ml-link-btn" onClick={() => setShowHidden((v) => !v)} aria-expanded={showHidden}>
                        {showHidden ? "Hide" : "Show"} {data.hidden.length} hidden {data.hidden.length === 1 ? "link" : "links"}
                    </button>
                    {showHidden && (
                        <ul className="ml-list">
                            {data.hidden.map((l) => (
                                <LinkRow key={l.key} link={l} onJump={onJump} onOpen={() => open(l)} onUnhide={() => hide(l, false)} />
                            ))}
                        </ul>
                    )}
                </div>
            )}

            <p className="ml-muted ml-footnote">
                Said: from the transcript. On screen: from screen text
                {caps?.accessibility_capture ? " and the browser's address" : ""}. Links are found on this Mac; pages are
                never fetched. Deleting or striking the words or screens removes their links.
            </p>
            {caps?.accessibility_capture && capture !== null && (
                <label className="ml-setting">
                    <input type="checkbox" checked={capture} onChange={(e) => toggleCapture(e.target.checked)} />
                    <span>
                        Record the browser's address while recording
                        <span className="ml-muted">
                            {" "}
                            (Safari, Chrome, Arc, Edge and Brave, while screen capture is on and Accessibility is already
                            allowed for noFriction)
                        </span>
                    </span>
                </label>
            )}
        </div>
    );
}

function LinkRow({
    link: l,
    onJump,
    onOpen,
    onHide,
    onUnhide,
    onEdit,
    onDelete,
}: {
    link: MeetingLink;
    onJump?: (ms: number) => void;
    onOpen: () => void;
    onHide?: () => void;
    onUnhide?: () => void;
    onEdit?: () => void;
    onDelete?: () => void;
}) {
    const counts = countsLabel(l);
    const openable = isOpenableUrl(l.url);
    return (
        <li className="ml-row">
            <div className="ml-main">
                {l.title && <div className="ml-title">{l.title}</div>}
                <div className="ml-addr" title={l.url}>
                    {displayLink(l) || l.url}
                </div>
                {l.note && <div className="ml-note">{l.note}</div>}
                <div className="ml-meta">
                    {l.sources.map((s) => (
                        <span key={s} className={`ml-badge ml-badge-${s}`}>
                            {SOURCE_LABEL[s]}
                        </span>
                    ))}
                    {counts && <span className="ml-muted">{counts}</span>}
                    {l.first_ms !== null &&
                        (onJump ? (
                            <button
                                className="ml-link-btn"
                                onClick={() => onJump(l.first_ms!)}
                                title="Show this moment in Rewind"
                            >
                                {l.first_source === "said" ? "first said" : "first seen"} at {formatOffset(l.first_ms)}
                            </button>
                        ) : (
                            <span className="ml-muted">
                                {l.first_source === "said" ? "first said" : "first seen"} at {formatOffset(l.first_ms)}
                            </span>
                        ))}
                </div>
            </div>
            <div className="ml-row-actions">
                <button className="ml-btn" onClick={onOpen} disabled={!openable} title={openable ? l.url : "Not a web link"}>
                    Open
                </button>
                {onEdit && l.reference_id && (
                    <button className="ml-btn ghost" onClick={onEdit}>
                        Edit
                    </button>
                )}
                {onDelete && l.reference_id && (
                    <button className="ml-btn ghost" onClick={onDelete}>
                        Remove
                    </button>
                )}
                {onHide && !l.reference_id && (
                    <button className="ml-btn ghost" onClick={onHide} title="Hide this link from the list (it stays in the transcript)">
                        Hide
                    </button>
                )}
                {onUnhide && (
                    <button className="ml-btn ghost" onClick={onUnhide}>
                        Unhide
                    </button>
                )}
            </div>
        </li>
    );
}

function ReferenceForm({
    form,
    saving,
    onChange,
    onSave,
    onCancel,
}: {
    form: Form;
    saving: boolean;
    onChange: (f: Form) => void;
    onSave: () => void;
    onCancel: () => void;
}) {
    return (
        <form
            className="ml-form"
            onSubmit={(e) => {
                e.preventDefault();
                onSave();
            }}
            onKeyDown={(e) => e.key === "Escape" && onCancel()}
        >
            <h4>{form.id ? "Edit reference" : "Add a reference"}</h4>
            <label className="ml-label" htmlFor="ml-url">
                Web address
            </label>
            <input
                id="ml-url"
                className="ml-input"
                value={form.url}
                placeholder="https://example.com/slides"
                autoFocus
                spellCheck={false}
                autoCapitalize="off"
                autoCorrect="off"
                onChange={(e) => onChange({ ...form, url: e.target.value })}
            />
            <label className="ml-label" htmlFor="ml-title">
                Title <span className="ml-muted">(optional)</span>
            </label>
            <input
                id="ml-title"
                className="ml-input"
                value={form.title}
                maxLength={200}
                placeholder="Agenda, slides, Chapter 3 reading…"
                onChange={(e) => onChange({ ...form, title: e.target.value })}
            />
            <label className="ml-label" htmlFor="ml-note">
                Note <span className="ml-muted">(optional)</span>
            </label>
            <textarea
                id="ml-note"
                className="ml-input ml-textarea"
                value={form.note}
                maxLength={2000}
                rows={2}
                onChange={(e) => onChange({ ...form, note: e.target.value })}
            />
            <div className="ml-actions end">
                <button type="button" className="ml-btn" onClick={onCancel}>
                    Cancel
                </button>
                <button type="submit" className="ml-btn primary" disabled={saving || !form.url.trim()}>
                    {saving ? "Saving…" : "Save"}
                </button>
            </div>
        </form>
    );
}
