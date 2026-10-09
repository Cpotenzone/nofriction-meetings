// Topics (docs/TOPICS_AND_CHAT.md): the topic index the Notes view and the
// search use, and the day grouping of the Recordings list. Pure (no Tauri,
// no React), tested with `npm test`.

export interface TopicRef {
    key: string;
    label: string;
}

export interface TopicSummary extends TopicRef {
    /** Recordings with this topic */
    count: number;
}

/** `list_topics`: every topic with its count, and each recording's topics. */
export interface TopicIndex {
    topics: TopicSummary[];
    by_meeting: Record<string, TopicRef[]>;
}

export interface MeetingTopic extends TopicRef {
    id: string;
    meeting_id: string;
    confidence: number | null;
    /** "ai" | "user" */
    source: string;
    created_at: string;
}

export const EMPTY_INDEX: TopicIndex = { topics: [], by_meeting: {} };

/** The fields of a recording the grouping needs. */
export interface Groupable {
    id: string;
    started_at: string;
}

export interface Group<M extends Groupable> {
    key: string;
    label: string;
    meetings: M[];
}

/** "Today", "Yesterday" or the date (local time). */
export function dayLabel(iso: string, now: Date = new Date()): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return "Unknown date";
    const sameDay = (a: Date, b: Date) =>
        a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
    if (sameDay(d, now)) return "Today";
    const y = new Date(now);
    y.setDate(now.getDate() - 1);
    if (sameDay(d, y)) return "Yesterday";
    return d.toLocaleDateString("en-US", { weekday: "short", month: "short", day: "numeric", year: "numeric" });
}

function dayKey(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return "unknown";
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** One group per day, in list order (the list is newest first). */
export function groupByDay<M extends Groupable>(meetings: M[], now: Date = new Date()): Group<M>[] {
    const groups: Group<M>[] = [];
    for (const m of meetings) {
        const key = dayKey(m.started_at);
        const g = groups[groups.length - 1];
        if (g && g.key === key) g.meetings.push(m);
        else groups.push({ key, label: dayLabel(m.started_at, now), meetings: [m] });
    }
    return groups;
}

/** The labels the editor sends back: trimmed, non-empty, no exact repeats (case-insensitive). */
export function cleanLabels(labels: string[]): string[] {
    const out: string[] = [];
    for (const raw of labels) {
        const l = raw.replace(/\s+/g, " ").trim();
        if (!l) continue;
        if (out.some((x) => x.toLowerCase() === l.toLowerCase())) continue;
        out.push(l);
    }
    return out;
}
