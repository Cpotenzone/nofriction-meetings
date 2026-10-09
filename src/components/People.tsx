// noFriction Meetings - People
//
// Who was in the meeting, from the calendar invite — and their LinkedIn.
// LinkedIn has no lookup API, so linking is one honest step: "Find" opens a
// prefilled LinkedIn search; paste the profile URL back and it sticks to
// that person for every meeting they're in.

import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
    getCalendarAccessStatus,
    getMeetingPeople,
    linkedinSearchUrl,
    listPeople,
    setPersonLinkedin,
    syncCalendar,
    type CalendarAccess,
    type MeetingPeople as MeetingPeopleData,
    type Person,
} from "../lib/tauri";
import { CalendarIcon, SearchIcon } from "./icons";
import "./People.css";

const open = (url: string) => openUrl(url).catch((e) => console.error("open failed", e));

function initials(p: Person) {
    const src = p.name || p.email;
    const parts = src.split(/[\s@._-]+/).filter(Boolean);
    return ((parts[0]?.[0] ?? "") + (parts[1]?.[0] ?? "")).toUpperCase() || "?";
}

function useRefreshOnPeopleUpdated(refresh: () => void) {
    useEffect(() => {
        let off: (() => void) | null = null;
        let disposed = false;
        listen("people_updated", () => refresh()).then((fn) => (disposed ? fn() : (off = fn)));
        return () => {
            disposed = true;
            off?.();
        };
    }, [refresh]);
}

// ── LinkedIn control ────────────────────────────────────────────────────

function LinkedInControl({ person, onSaved }: { person: Person; onSaved: (url: string | null) => void }) {
    const [editing, setEditing] = useState(false);
    const [value, setValue] = useState("");
    const [error, setError] = useState<string | null>(null);

    const save = async (url: string | null) => {
        try {
            const saved = await setPersonLinkedin(person.id, url);
            onSaved(saved);
            setEditing(false);
            setError(null);
        } catch (e) {
            setError(String(e));
        }
    };

    if (editing) {
        return (
            <form
                className="li-edit"
                onSubmit={(e) => {
                    e.preventDefault();
                    save(value);
                }}
            >
                <input
                    autoFocus
                    value={value}
                    placeholder="linkedin.com/in/…"
                    onChange={(e) => setValue(e.target.value)}
                    onKeyDown={(e) => e.key === "Escape" && setEditing(false)}
                    aria-label={`LinkedIn profile URL for ${person.name ?? person.email}`}
                />
                <button type="submit">Save</button>
                {error && <span className="li-edit__error">{error}</span>}
            </form>
        );
    }

    if (person.linkedin_url) {
        return (
            <div className="li">
                <button className="li__open" onClick={() => open(person.linkedin_url!)} type="button" title={person.linkedin_url}>
                    <span className="li__badge" aria-hidden>in</span>
                    Profile
                </button>
                <button
                    className="li__ghost"
                    onClick={() => {
                        setValue(person.linkedin_url ?? "");
                        setEditing(true);
                    }}
                    type="button"
                    title="Change LinkedIn link"
                >
                    Edit
                </button>
            </div>
        );
    }

    return (
        <div className="li">
            <button className="li__ghost" onClick={() => open(linkedinSearchUrl(person))} type="button" title="Search LinkedIn for this person">
                <SearchIcon size={12} /> Find
            </button>
            <button
                className="li__ghost"
                onClick={() => {
                    setValue("");
                    setEditing(true);
                }}
                type="button"
            >
                Paste link
            </button>
        </div>
    );
}

function PersonRow({ person, onChange, showStats }: { person: Person; onChange: (p: Person) => void; showStats?: boolean }) {
    return (
        <li className="person">
            <span className="person__avatar" aria-hidden>{initials(person)}</span>
            <div className="person__who">
                <span className="person__name">
                    {person.name ?? person.email}
                    {person.role === "organizer" && <span className="person__tag">Organizer</span>}
                </span>
                <span className="person__sub">
                    {[person.company, person.email].filter(Boolean).join(" · ")}
                    {showStats && person.meeting_count > 0 && (
                        <>
                            {" · "}
                            {person.meeting_count} meeting{person.meeting_count === 1 ? "" : "s"}
                            {person.last_met && `, last ${new Date(person.last_met).toLocaleDateString([], { month: "short", day: "numeric" })}`}
                        </>
                    )}
                </span>
            </div>
            <LinkedInControl person={person} onSaved={(url) => onChange({ ...person, linkedin_url: url })} />
        </li>
    );
}

// ── Connect-calendar prompt ─────────────────────────────────────────────

function ConnectCalendar({ access, onSynced }: { access: CalendarAccess | null; onSynced: () => void }) {
    const [busy, setBusy] = useState(false);
    const [msg, setMsg] = useState<string | null>(null);

    const connect = async () => {
        setBusy(true);
        try {
            const r = await syncCalendar();
            if (r.access !== "authorized") {
                setMsg("Calendar access is off. Turn on noFriction in System Settings → Privacy & Security → Calendars, then try again.");
                invoke("open_system_settings", { pane: "calendar" }).catch((e) => console.error("open failed", e));
            } else {
                setMsg(r.report ? `Linked ${r.report.meetings_linked} of ${r.report.meetings_checked} recordings to your calendar.` : null);
                onSynced();
            }
        } catch (e) {
            setMsg(String(e));
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="cal-connect">
            <CalendarIcon size={18} />
            <div className="cal-connect__text">
                <strong>{access === "denied" ? "Calendar access is off" : "Connect your calendar"}</strong>
                <span>{msg ?? "Recordings get their real titles, times and attendees — then link each person's LinkedIn."}</span>
            </div>
            <button onClick={connect} disabled={busy} type="button">
                {busy ? "Connecting…" : access === "denied" ? "Open Settings" : "Connect"}
            </button>
        </div>
    );
}

// ── Per-meeting strip ───────────────────────────────────────────────────

export function MeetingPeople({ meetingId }: { meetingId: string }) {
    const [data, setData] = useState<MeetingPeopleData | null>(null);
    const [access, setAccess] = useState<CalendarAccess | null>(null);

    const load = useCallback(() => {
        getMeetingPeople(meetingId).then(setData).catch(() => setData({ details: null, people: [] }));
        getCalendarAccessStatus().then(setAccess).catch(() => {});
    }, [meetingId]);

    useEffect(load, [load]);
    useRefreshOnPeopleUpdated(load);

    if (!data) return null;
    const people = data.people.filter((p) => !p.is_self);
    const d = data.details;

    if (!d && access !== "authorized") {
        return (
            <section className="mpeople">
                <ConnectCalendar access={access} onSynced={load} />
            </section>
        );
    }

    if (!d) {
        return (
            <section className="mpeople mpeople--quiet">
                <p>No calendar event overlapped this recording.</p>
            </section>
        );
    }

    const when = d.scheduled_start
        ? `${new Date(d.scheduled_start).toLocaleString([], { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" })}${
              d.scheduled_end ? ` – ${new Date(d.scheduled_end).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}` : ""
          }`
        : null;

    return (
        <section className="mpeople">
            <header className="mpeople__head">
                <div className="mpeople__event">
                    <span className="mpeople__title">{d.event_title}</span>
                    <span className="mpeople__meta">
                        {[when, d.location && !d.location.startsWith("http") ? d.location : null, d.calendar_name].filter(Boolean).join(" · ")}
                    </span>
                </div>
                {d.meeting_url && (
                    <button className="li__ghost" onClick={() => open(d.meeting_url!)} type="button">Join link</button>
                )}
            </header>
            {people.length > 0 ? (
                <ul className="people-list">
                    {people.map((p) => (
                        <PersonRow
                            key={p.id}
                            person={p}
                            onChange={(np) => setData((cur) => cur && { ...cur, people: cur.people.map((x) => (x.id === np.id ? np : x)) })}
                        />
                    ))}
                </ul>
            ) : (
                <p className="mpeople__none">No other attendees on the invite.</p>
            )}
            {d.notes && (
                <details className="mpeople__notes">
                    <summary>Invite notes</summary>
                    <p>{d.notes}</p>
                </details>
            )}
        </section>
    );
}

// ── Directory ───────────────────────────────────────────────────────────

export function PeopleDirectory() {
    const [people, setPeople] = useState<Person[] | null>(null);
    const [access, setAccess] = useState<CalendarAccess | null>(null);
    const [query, setQuery] = useState("");
    const [onlyUnlinked, setOnlyUnlinked] = useState(false);
    const [syncing, setSyncing] = useState(false);

    const load = useCallback(() => {
        listPeople().then(setPeople).catch(() => setPeople([]));
        getCalendarAccessStatus().then(setAccess).catch(() => {});
    }, []);
    useEffect(load, [load]);
    useRefreshOnPeopleUpdated(load);

    const shown = useMemo(() => {
        const q = query.trim().toLowerCase();
        return (people ?? []).filter(
            (p) =>
                (!onlyUnlinked || !p.linkedin_url) &&
                (!q || [p.name, p.email, p.company].some((f) => f?.toLowerCase().includes(q)))
        );
    }, [people, query, onlyUnlinked]);

    const resync = async () => {
        setSyncing(true);
        try {
            await syncCalendar(true);
            load();
        } finally {
            setSyncing(false);
        }
    };

    return (
        <div className="pdir">
            {access !== "authorized" && <ConnectCalendar access={access} onSynced={load} />}
            <div className="pdir__bar">
                <input
                    className="pdir__search"
                    placeholder="Search people or companies"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                />
                <label className="pdir__toggle">
                    <input type="checkbox" checked={onlyUnlinked} onChange={(e) => setOnlyUnlinked(e.target.checked)} />
                    No LinkedIn yet
                </label>
                {access === "authorized" && (
                    <button className="li__ghost" onClick={resync} disabled={syncing} type="button">
                        {syncing ? "Syncing…" : "Re-sync calendar"}
                    </button>
                )}
            </div>
            {people === null ? null : shown.length === 0 ? (
                <p className="pdir__empty">
                    {people.length === 0 ? "People from your meeting invites will appear here." : "No one matches."}
                </p>
            ) : (
                <ul className="people-list">
                    {shown.map((p) => (
                        <PersonRow
                            key={p.id}
                            person={p}
                            showStats
                            onChange={(np) => setPeople((cur) => cur && cur.map((x) => (x.id === np.id ? np : x)))}
                        />
                    ))}
                </ul>
            )}
        </div>
    );
}
