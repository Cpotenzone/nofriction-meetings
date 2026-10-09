// The one search (Recordings): hits from the backend grouped per recording.
// Pure (no Tauri, no React), tested with `npm test`.

export type HitKind = "title" | "notebook" | "person" | "topic" | "said";

export interface SearchHit {
    meeting_id: string;
    kind: HitKind | string;
    label: string;
    /** A line: ms from the recording's start */
    ms: number | null;
}

export interface SearchGroup {
    meetingId: string;
    /** People and topics that matched ("Dana Whitfield · Q4 launch") */
    why: string[];
    /** Up to `maxLines` lines that matched, earliest first */
    lines: { ms: number | null; text: string }[];
    /** Lines that matched beyond `maxLines` */
    moreLines: number;
}

/** Group hits per recording, keeping `order` (newest first) for the groups. */
export function groupHits(hits: SearchHit[], order: string[], maxLines = 3): SearchGroup[] {
    const byId = new Map<string, SearchGroup>();
    for (const h of hits) {
        let g = byId.get(h.meeting_id);
        if (!g) {
            g = { meetingId: h.meeting_id, why: [], lines: [], moreLines: 0 };
            byId.set(h.meeting_id, g);
        }
        if (h.kind === "said") {
            g.lines.push({ ms: h.ms, text: h.label });
        } else if (h.kind !== "title" && !g.why.some((w) => w.toLowerCase() === h.label.toLowerCase())) {
            g.why.push(h.label);
        }
    }
    const groups: SearchGroup[] = [];
    const seen = new Set<string>();
    const finish = (g: SearchGroup): SearchGroup => {
        g.lines.sort((a, b) => (a.ms ?? Number.MAX_SAFE_INTEGER) - (b.ms ?? Number.MAX_SAFE_INTEGER));
        g.moreLines = Math.max(0, g.lines.length - maxLines);
        g.lines = g.lines.slice(0, maxLines);
        return g;
    };
    for (const id of order) {
        const g = byId.get(id);
        if (g && !seen.has(id)) {
            seen.add(id);
            groups.push(finish(g));
        }
    }
    // Recordings not in `order` (an older list) still show, after it
    for (const [id, g] of byId) {
        if (!seen.has(id)) groups.push(finish(g));
    }
    return groups;
}

/** "Titles, people, topics, or anything said" */
export const SEARCH_PROMPT = "Titles, people, topics, or anything said";
