// Screen selection in the Recordings timeline (Finder-style), and the time
// helpers the time-range Delete/Strike uses. Pure functions, tested in
// screenSelection.test.ts (`npm test`).
//
// One rule keeps the count honest: the ids sent to the backend and the
// number shown in the selection bar both come from `orderedIds()`, which
// keeps only screens that are on the timeline right now, in timeline order.

export interface ScreenItem {
    id: string;
    /** ms from the meeting start */
    timestamp_ms: number;
    /** when the screen stopped being shown, if known */
    end_ms?: number | null;
}

export interface Selection {
    /** Selected screen ids */
    ids: ReadonlySet<string>;
    /** Where a Shift-click range starts: the last screen clicked without Shift */
    anchor: string | null;
    /** The selection before the current Shift-range. Another Shift-click
     *  replaces only the range part (like Finder), it doesn't accumulate. */
    base: ReadonlySet<string>;
}

export const EMPTY_SELECTION: Selection = { ids: new Set(), anchor: null, base: new Set() };

export interface ClickMods {
    shift: boolean;
    /** ⌘ (or Ctrl) */
    toggle: boolean;
    /** "Select screens" mode: a plain click toggles */
    selectMode: boolean;
}

/** Ids between `a` and `b` (inclusive) in timeline order. */
function between(items: readonly ScreenItem[], a: string, b: string): string[] {
    const i = items.findIndex((x) => x.id === a);
    const j = items.findIndex((x) => x.id === b);
    if (i < 0 || j < 0) return j >= 0 ? [b] : [];
    const [lo, hi] = i <= j ? [i, j] : [j, i];
    return items.slice(lo, hi + 1).map((x) => x.id);
}

/**
 * A click on a screen thumbnail.
 * - Shift: select every screen from the anchor to this one (both included).
 *   With no anchor yet, the screen being viewed is the anchor, so it is
 *   selected visibly (✓) rather than implied.
 * - ⌘/Ctrl, or any click in select mode: toggle this screen; it becomes the anchor.
 * - Plain click: just view it (`view: true`); it becomes the anchor. The
 *   selection is unchanged.
 */
export function clickScreen(
    sel: Selection,
    items: readonly ScreenItem[],
    id: string,
    mods: ClickMods,
    current: string | null,
): { sel: Selection; view: boolean } {
    if (mods.shift) {
        const anchor = sel.anchor ?? current ?? id;
        const ids = new Set(sel.base);
        for (const x of between(items, anchor, id)) ids.add(x);
        return { sel: { ids, anchor, base: sel.base }, view: false };
    }
    if (mods.toggle || mods.selectMode) {
        const ids = new Set(sel.ids);
        if (ids.has(id)) ids.delete(id);
        else ids.add(id);
        return { sel: { ids, anchor: id, base: new Set(ids) }, view: false };
    }
    return { sel: { ...sel, anchor: id, base: new Set(sel.ids) }, view: true };
}

export function selectAll(items: readonly ScreenItem[]): Selection {
    const ids = new Set(items.map((x) => x.id));
    return { ids, anchor: items[0]?.id ?? null, base: new Set(ids) };
}

/** From `fromId` (included) to the last screen. */
export function selectToEnd(items: readonly ScreenItem[], fromId: string | null): Selection {
    const i = Math.max(0, fromId ? items.findIndex((x) => x.id === fromId) : 0);
    const ids = new Set(items.slice(i).map((x) => x.id));
    return { ids, anchor: items[i]?.id ?? null, base: new Set(ids) };
}

/** Screens in the last `minutes` before `endMs`, and that span itself
 *  (what "delete the last 12 minutes" removes). */
export function selectLastMinutes(
    items: readonly ScreenItem[],
    endMs: number,
    minutes: number,
): { sel: Selection; range: [number, number] } {
    const start = Math.max(0, endMs - Math.max(0, minutes) * 60_000);
    const picked = items.filter((x) => x.timestamp_ms >= start && x.timestamp_ms <= endMs);
    const ids = new Set(picked.map((x) => x.id));
    return { sel: { ids, anchor: picked[0]?.id ?? null, base: new Set(ids) }, range: [start, endMs] };
}

/** The selected ids that are on the timeline, in timeline order. Send
 *  exactly these, and show exactly this many. */
export function orderedIds(items: readonly ScreenItem[], sel: Selection): string[] {
    return items.filter((x) => sel.ids.has(x.id)).map((x) => x.id);
}

/** Drop ids that are no longer on the timeline (after a reload). */
export function pruneSelection(items: readonly ScreenItem[], sel: Selection): Selection {
    const on = new Set(items.map((x) => x.id));
    const keep = (s: ReadonlySet<string>) => new Set([...s].filter((id) => on.has(id)));
    return { ids: keep(sel.ids), anchor: sel.anchor && on.has(sel.anchor) ? sel.anchor : null, base: keep(sel.base) };
}

export interface SelectionSummary {
    count: number;
    ids: string[];
    firstMs: number;
    lastMs: number;
    /** Separate runs of adjacent screens (1 = one contiguous block) */
    groups: number;
}

export function summarize(items: readonly ScreenItem[], sel: Selection): SelectionSummary | null {
    const ids = orderedIds(items, sel);
    if (ids.length === 0) return null;
    let groups = 0;
    let prevSelected = false;
    for (const x of items) {
        const s = sel.ids.has(x.id);
        if (s && !prevSelected) groups++;
        prevSelected = s;
    }
    const picked = items.filter((x) => sel.ids.has(x.id));
    return {
        count: ids.length,
        ids,
        firstMs: picked[0].timestamp_ms,
        lastMs: picked[picked.length - 1].timestamp_ms,
        groups,
    };
}

/** The time span a set of screens covers: from the first one's start to
 *  when the last one stopped being shown (its end, else the next screen,
 *  else the end of the meeting). */
export function rangeOfScreens(
    items: readonly ScreenItem[],
    ids: readonly string[],
    meetingEndMs: number,
): [number, number] | null {
    const set = new Set(ids);
    const idx = items.map((x, i) => (set.has(x.id) ? i : -1)).filter((i) => i >= 0);
    if (idx.length === 0) return null;
    const first = items[idx[0]];
    const lastI = idx[idx.length - 1];
    const last = items[lastI];
    const next = items[lastI + 1];
    let end = last.end_ms ?? next?.timestamp_ms ?? meetingEndMs;
    if (!(end > last.timestamp_ms)) end = last.timestamp_ms + 1000;
    return [first.timestamp_ms, end];
}

// ── Clock times ─────────────────────────────────────────────────────────

const pad = (n: number) => String(n).padStart(2, "0");

/** Local wall-clock time of a timeline offset: "10:41:05". */
export function clockAt(startedAt: string, ms: number): string {
    const d = new Date(Date.parse(startedAt) + ms);
    if (isNaN(d.getTime())) return "";
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/** "10:41–10:53" (seconds shown when the two would otherwise read the same) */
export function spanLabel(startedAt: string, a: number, b: number): string {
    const x = clockAt(startedAt, a);
    const y = clockAt(startedAt, b);
    if (!x || !y) return "";
    const short = (s: string) => s.slice(0, 5);
    return short(x) === short(y) ? `${x}–${y}` : `${short(x)}–${short(y)}`;
}

/** "12 min", "45 s", "1 h 5 min" */
export function durationLabel(ms: number): string {
    const s = Math.max(0, Math.round(ms / 1000));
    if (s < 60) return `${s} s`;
    const m = Math.round(s / 60);
    if (m < 60) return `${m} min`;
    return `${Math.floor(m / 60)} h ${m % 60} min`;
}

/**
 * "HH:MM" or "HH:MM:SS" (local time) → ms from the meeting start. The day is
 * chosen so the result lands nearest `nearMs` (meetings can cross midnight).
 */
export function parseClock(startedAt: string, value: string, nearMs: number): number | null {
    const m = /^(\d{1,2}):(\d{2})(?::(\d{2}))?$/.exec(value.trim());
    const start = Date.parse(startedAt);
    if (!m || isNaN(start)) return null;
    const [h, min, sec] = [Number(m[1]), Number(m[2]), Number(m[3] ?? 0)];
    if (h > 23 || min > 59 || sec > 59) return null;
    const near = new Date(start + nearMs);
    const day = new Date(near.getFullYear(), near.getMonth(), near.getDate(), h, min, sec).getTime();
    let best: number | null = null;
    for (const shift of [-1, 0, 1]) {
        const t = day + shift * 86_400_000 - start;
        if (best === null || Math.abs(t - nearMs) < Math.abs(best - nearMs)) best = t;
    }
    return best;
}
