// Topics in the recordings list (docs/TOPICS_AND_CHAT.md): filtering by
// topic and grouping the list by Date, Notebook or Topic. Pure (no Tauri,
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

export type GroupBy = "date" | "notebook" | "topic";
export const GROUP_BY_OPTIONS: { value: GroupBy; label: string }[] = [
    { value: "date", label: "Date" },
    { value: "notebook", label: "Notebook" },
    { value: "topic", label: "Topic" },
];
export const GROUP_BY_KEY = "nf.recordings.groupBy";

export function parseGroupBy(value: string | null | undefined): GroupBy {
    return value === "notebook" || value === "topic" ? value : "date";
}

/** The fields of a recording the grouping needs. */
export interface Groupable {
    id: string;
    started_at: string;
    class_name?: string | null;
}

export interface Group<M extends Groupable> {
    key: string;
    label: string;
    meetings: M[];
}

/** Recordings with topic `key` (null = all). */
export function filterByTopic<M extends Groupable>(meetings: M[], index: TopicIndex, key: string | null): M[] {
    if (!key) return meetings;
    return meetings.filter((m) => (index.by_meeting[m.id] ?? []).some((t) => t.key === key));
}

/** Up to `max` topic chips for a row (the backend already orders user topics first). */
export function topicsForRow(index: TopicIndex, meetingId: string, max = 2): { shown: TopicRef[]; more: number } {
    const all = index.by_meeting[meetingId] ?? [];
    return { shown: all.slice(0, max), more: Math.max(0, all.length - max) };
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

/**
 * Group a list (already newest first) for display. Date: one group per
 * day, in list order. Notebook: by name (case-insensitive, first spelling
 * shown), then "No notebook". Topic: a recording appears under each of its
 * topics, biggest topic first (then by label), then "No topics".
 */
export function groupRecordings<M extends Groupable>(
    meetings: M[],
    index: TopicIndex,
    by: GroupBy,
    now: Date = new Date(),
): Group<M>[] {
    if (by === "date") {
        const groups: Group<M>[] = [];
        for (const m of meetings) {
            const key = dayKey(m.started_at);
            const g = groups[groups.length - 1];
            if (g && g.key === key) g.meetings.push(m);
            else groups.push({ key, label: dayLabel(m.started_at, now), meetings: [m] });
        }
        return groups;
    }
    if (by === "notebook") {
        const map = new Map<string, Group<M>>();
        const none: M[] = [];
        for (const m of meetings) {
            const name = (m.class_name ?? "").trim();
            if (!name) {
                none.push(m);
                continue;
            }
            const key = name.toLowerCase();
            const g = map.get(key);
            if (g) g.meetings.push(m);
            else map.set(key, { key: `nb:${key}`, label: name, meetings: [m] });
        }
        const groups = [...map.values()].sort((a, b) => a.label.localeCompare(b.label, undefined, { sensitivity: "base" }));
        if (none.length) groups.push({ key: "nb:", label: "No notebook", meetings: none });
        return groups;
    }
    const map = new Map<string, Group<M>>();
    const none: M[] = [];
    for (const m of meetings) {
        const refs = index.by_meeting[m.id] ?? [];
        if (refs.length === 0) {
            none.push(m);
            continue;
        }
        for (const t of refs) {
            const g = map.get(t.key);
            if (g) g.meetings.push(m);
            else map.set(t.key, { key: `topic:${t.key}`, label: t.label, meetings: [m] });
        }
    }
    const groups = [...map.values()].sort(
        (a, b) => b.meetings.length - a.meetings.length || a.label.localeCompare(b.label, undefined, { sensitivity: "base" }),
    );
    if (none.length) groups.push({ key: "topic:", label: "No topics", meetings: none });
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
