// noFriction Meetings - Links & References (pure helpers; docs/LINKS.md)
//
// The backend (src-tauri/src/meeting_links.rs) detects and normalizes links
// and checks the scheme again before opening. Here: the same scheme check
// for the UI, display text, times and "Copy all as Markdown".

export type LinkSource = "added" | "said" | "screen";

export interface MeetingLink {
    /** Normalized URL without scheme: the dedupe key, and what Hide hashes */
    key: string;
    /** What Open opens (http/https only) */
    url: string;
    /** Host without www. (with :port when not the default) */
    host: string;
    /** Path, query and #/route after the host ("" for a home page) */
    path: string;
    title: string | null;
    note: string | null;
    sources: LinkSource[];
    said_count: number;
    screen_count: number;
    /** First time said or seen, ms from the meeting start (Recordings time) */
    first_ms: number | null;
    first_at: string | null;
    first_source: "said" | "screen" | null;
    /** Set for an added reference */
    reference_id: string | null;
    created_at: string | null;
}

export interface MeetingLinks {
    meeting_id: string;
    started_at: string;
    links: MeetingLink[];
    /** Detected links the user hid */
    hidden: MeetingLink[];
}

export interface MeetingReference {
    id: string;
    meeting_id: string;
    url: string;
    title: string | null;
    note: string | null;
    created_at: string;
}

const HOST_LABEL = /^[a-z0-9]([a-z0-9-]*[a-z0-9])?$/;

/** Only http and https links open, and only well-formed ones (the backend
 *  checks again). Never javascript:, file:, data:, mailto: or anything else. */
export function isOpenableUrl(url: string): boolean {
    if (typeof url !== "string" || url !== url.trim() || /[\s\u0000-\u001f\u007f]/.test(url)) return false;
    const m = /^(https?):\/\/([^/?#]*)/i.exec(url);
    if (!m) return false;
    const authority = m[2];
    if (!authority || authority.includes("@")) return false;
    let host = authority;
    const colon = authority.lastIndexOf(":");
    if (colon >= 0) {
        const port = authority.slice(colon + 1);
        if (!/^\d{1,5}$/.test(port) || Number(port) === 0 || Number(port) > 65535) return false;
        host = authority.slice(0, colon);
    }
    host = host.toLowerCase().replace(/\.$/, "");
    if (!host || host.length > 253) return false;
    if (host !== "localhost" && !host.includes(".")) return false;
    return host.split(".").every((l) => l.length <= 63 && HOST_LABEL.test(l));
}

/** A typed reference address: https:// is added when there's no scheme.
 *  Returns null for anything that isn't a web address. */
export function referenceUrl(input: string): string | null {
    const s = input.trim();
    if (!s) return null;
    const withScheme = /^https?:\/\//i.test(s) ? s : /^[a-z][a-z0-9+.-]*:(?!\d)/i.test(s) ? null : `https://${s}`;
    return withScheme && isOpenableUrl(withScheme) ? withScheme : null;
}

/** "/document/d/abc123/edit" → itself; long paths keep their first and last
 *  segment ("/courses/…/week-3"), then are cut to `max` characters. */
export function shortPath(path: string, max = 36): string {
    if (path.length <= max) return path;
    const q = path.search(/[?#]/);
    const bare = q >= 0 ? path.slice(0, q) : path;
    const segs = bare.split("/").filter(Boolean);
    if (segs.length >= 3) {
        const s = `/${segs[0]}/…/${segs[segs.length - 1]}${q >= 0 ? "?…" : ""}`;
        if (s.length <= max) return s;
    }
    return `${path.slice(0, Math.max(1, max - 1))}…`;
}

/** Domain plus a shortened path: "khanacademy.org/math/…/unit-2". */
export function displayLink(l: Pick<MeetingLink, "host" | "path">): string {
    return `${l.host}${shortPath(l.path)}`;
}

/** Meeting time as the Recordings view shows it: m:ss from the start. */
export function formatOffset(ms: number): string {
    const s = Math.max(0, Math.floor(ms / 1000));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** "Said 2× · On screen 5×" (Added is its own badge). */
export function countsLabel(l: Pick<MeetingLink, "said_count" | "screen_count">): string {
    const parts: string[] = [];
    if (l.said_count > 0) parts.push(`said ${l.said_count}×`);
    if (l.screen_count > 0) parts.push(`on screen ${l.screen_count}×`);
    return parts.join(" · ");
}

export const SOURCE_LABEL: Record<LinkSource, string> = {
    added: "Added",
    said: "Said",
    screen: "On screen",
};

function mdText(s: string): string {
    return s.replace(/\s+/g, " ").replace(/([\\[\]*_`<>])/g, "\\$1").trim();
}

/** A link target Markdown can't misread: brackets and spaces percent-encoded
 *  (encodeURIComponent leaves parentheses alone). */
function mdUrl(url: string): string {
    return url.replace(/[()\s<>]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase().padStart(2, "0")}`);
}

/** "Copy all as Markdown": one bullet per link, with its sources, first
 *  time and note. Only openable links are written as links. */
export function linksMarkdown(meetingTitle: string, links: MeetingLink[]): string {
    const lines = [`## Links — ${mdText(meetingTitle || "Meeting")}`, ""];
    for (const l of links) {
        const label = mdText(l.title || displayLink(l) || l.url);
        const head = isOpenableUrl(l.url) ? `[${label}](${mdUrl(l.url)})` : label;
        const meta: string[] = [];
        if (l.sources.includes("added")) meta.push("added");
        const counts = countsLabel(l);
        if (counts) meta.push(counts);
        if (l.first_ms !== null) meta.push(`first at ${formatOffset(l.first_ms)}`);
        let line = `- ${head}`;
        if (meta.length) line += ` — ${meta.join(" · ")}`;
        if (l.note) line += ` — ${mdText(l.note)}`;
        lines.push(line);
    }
    if (links.length === 0) lines.push("_No links._");
    return lines.join("\n") + "\n";
}
