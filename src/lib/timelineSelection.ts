// Selection on the Recordings timeline: screens and transcript lines.
// Pure functions, tested in timelineSelection.test.ts (`npm test`).
//
// Each pane selects Finder-style: click, Shift-click (contiguous range from
// the anchor), ⌘-click (toggle), Select all, From here to the end, Last N
// minutes. The two panes can be LINKED by time (docs/REDACTION.md):
//
// A linked selection is a set of time spans: the span of each run of
// adjacent picks in either pane (gaps inside a run included), plus an exact
// span from Select all / From here to the end / Last N minutes, minus the
// spans of items toggled off. Both panes then highlight what those spans
// remove, by the rules of the backend's time-range Delete
// (src-tauri/src/redaction/time_range.rs):
// - a screen when it was captured inside a span;
// - a line with word timings when the middle of a word's time is inside
//   (every word: whole; some: split at the edge);
// - a line without word timings when at least half of its (estimated) span
//   is inside.
// So what is highlighted is what a Delete removes, and the preview of the
// spans confirms it before anything is sent (`matchesPreview`).
//
// One rule keeps the count honest: the ids sent to the backend and the
// number shown in the selection bar come from the same ordered list
// (`orderedIds`, or the linked selection's lists).

/** ms from the meeting start, both ends included */
export type Span = [number, number];

export interface TimedItem {
    id: string;
    /** ms from the meeting start */
    timestamp_ms: number;
    /** When the screen stopped being shown / the line ends (ms), if known */
    end_ms?: number | null;
}

export interface LineItem extends TimedItem {
    /** Middle of each word's time (ms), when word timings are stored */
    word_mids_ms?: number[] | null;
    /** Words left in the line (one holding only strike markers has none) */
    words?: number;
}

export interface Selection {
    /** Picked ids */
    ids: ReadonlySet<string>;
    /** Where a Shift-click range starts: the last item clicked without Shift */
    anchor: string | null;
    /** The selection before the current Shift-range. Another Shift-click
     *  replaces only the range part (like Finder), it doesn't accumulate. */
    base: ReadonlySet<string>;
    /** Linked selection: items toggled off although a picked span covers
     *  them (their span is left out of the time ranges) */
    excluded?: ReadonlySet<string>;
}

export const EMPTY_SELECTION: Selection = { ids: new Set(), anchor: null, base: new Set() };

export interface ClickMods {
    shift: boolean;
    /** ⌘ (or Ctrl) */
    toggle: boolean;
    /** "Select screens" mode: a plain click toggles */
    selectMode: boolean;
}

export interface ClickOptions {
    /** What a plain click does: just view the item (screens), or select
     *  only it (lines). Default "view". */
    plain?: "view" | "replace";
    /** Linked selection: what this pane highlights now. ⌘-click on a
     *  highlighted item turns it off even when it isn't a pick. */
    covered?: ReadonlySet<string>;
}

/** Ids between `a` and `b` (inclusive) in list order. */
function between(items: readonly { id: string }[], a: string, b: string): string[] {
    const i = items.findIndex((x) => x.id === a);
    const j = items.findIndex((x) => x.id === b);
    if (i < 0 || j < 0) return j >= 0 ? [b] : [];
    const [lo, hi] = i <= j ? [i, j] : [j, i];
    return items.slice(lo, hi + 1).map((x) => x.id);
}

function without(s: ReadonlySet<string> | undefined, drop: Iterable<string>): Set<string> {
    const out = new Set(s ?? []);
    for (const x of drop) out.delete(x);
    return out;
}

/**
 * A click on an item (screen thumbnail or transcript line).
 * - Shift: select every item from the anchor to this one (both included).
 *   With no anchor yet, `current` (the item being viewed, or the first one
 *   highlighted) is the anchor, so it is selected visibly rather than implied.
 * - ⌘/Ctrl, or any click in select mode: toggle this item; it becomes the
 *   anchor. In a linked selection, a highlighted item is turned off.
 * - Plain click: view it (`view: true`) without changing the selection, or
 *   with `plain: "replace"` select only it. It becomes the anchor.
 */
export function clickItem(
    sel: Selection,
    items: readonly { id: string }[],
    id: string,
    mods: ClickMods,
    current: string | null,
    opts: ClickOptions = {},
): { sel: Selection; view: boolean } {
    if (mods.shift) {
        const anchor = sel.anchor ?? current ?? id;
        const range = between(items, anchor, id);
        const ids = new Set(sel.base);
        for (const x of range) ids.add(x);
        return { sel: { ids, anchor, base: sel.base, excluded: without(sel.excluded, range) }, view: false };
    }
    if (mods.toggle || mods.selectMode) {
        const ids = new Set(sel.ids);
        const excluded = new Set(sel.excluded ?? []);
        const on = opts.covered ? opts.covered.has(id) : ids.has(id);
        if (on) {
            ids.delete(id);
            if (opts.covered) excluded.add(id);
        } else {
            ids.add(id);
            excluded.delete(id);
        }
        return { sel: { ids, anchor: id, base: new Set(ids), excluded }, view: false };
    }
    if (opts.plain === "replace") {
        return { sel: { ids: new Set([id]), anchor: id, base: new Set([id]) }, view: true };
    }
    return { sel: { ...sel, anchor: id, base: new Set(sel.ids) }, view: true };
}

export function selectAll(items: readonly { id: string }[]): Selection {
    const ids = new Set(items.map((x) => x.id));
    return { ids, anchor: items[0]?.id ?? null, base: new Set(ids) };
}

/** From `fromId` (included) to the last item. */
export function selectToEnd(items: readonly { id: string }[], fromId: string | null): Selection {
    const i = Math.max(0, fromId ? items.findIndex((x) => x.id === fromId) : 0);
    const ids = new Set(items.slice(i).map((x) => x.id));
    return { ids, anchor: items[i]?.id ?? null, base: new Set(ids) };
}

/** Items that start in the last `minutes` before `endMs`, and that span
 *  itself (what "delete the last 12 minutes" removes). */
export function selectLastMinutes(
    items: readonly TimedItem[],
    endMs: number,
    minutes: number,
): { sel: Selection; range: Span } {
    const start = Math.max(0, endMs - Math.max(0, minutes) * 60_000);
    const picked = items.filter((x) => x.timestamp_ms >= start && x.timestamp_ms <= endMs);
    const ids = new Set(picked.map((x) => x.id));
    return { sel: { ids, anchor: picked[0]?.id ?? null, base: new Set(ids) }, range: [start, endMs] };
}

/** The selected ids that are in the list, in list order. Send exactly
 *  these, and show exactly this many. */
export function orderedIds(items: readonly { id: string }[], sel: Selection): string[] {
    return items.filter((x) => sel.ids.has(x.id)).map((x) => x.id);
}

/** Drop ids that are no longer in the list (after a reload). */
export function pruneSelection(items: readonly { id: string }[], sel: Selection): Selection {
    const on = new Set(items.map((x) => x.id));
    const keep = (s: ReadonlySet<string>) => new Set([...s].filter((id) => on.has(id)));
    return {
        ids: keep(sel.ids),
        anchor: sel.anchor && on.has(sel.anchor) ? sel.anchor : null,
        base: keep(sel.base),
        ...(sel.excluded ? { excluded: keep(sel.excluded) } : {}),
    };
}

export interface SelectionSummary {
    count: number;
    ids: string[];
    firstMs: number;
    lastMs: number;
    /** Separate runs of adjacent items (1 = one contiguous block) */
    groups: number;
}

export function summarize(items: readonly TimedItem[], sel: Selection): SelectionSummary | null {
    const ids = orderedIds(items, sel);
    if (ids.length === 0) return null;
    const picked = items.filter((x) => sel.ids.has(x.id));
    return {
        count: ids.length,
        ids,
        firstMs: picked[0].timestamp_ms,
        lastMs: picked[picked.length - 1].timestamp_ms,
        groups: selectedRuns(items, sel.ids).length,
    };
}

/** Runs of adjacent selected items, in list order. */
export function selectedRuns<T extends { id: string }>(items: readonly T[], ids: ReadonlySet<string>): T[][] {
    const runs: T[][] = [];
    let run: T[] | null = null;
    for (const x of items) {
        if (ids.has(x.id)) {
            if (!run) runs.push((run = []));
            run.push(x);
        } else {
            run = null;
        }
    }
    return runs;
}

/** The time span a set of screens covers: from the first one's start to
 *  when the last one stopped being shown (its end, else the next screen,
 *  else the end of the meeting). Used to prefill the typed time range. */
export function rangeOfScreens(
    items: readonly TimedItem[],
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

// ── Time spans ──────────────────────────────────────────────────────────

/** A screen's recorded end that is at most this long before the next
 *  screen is capture latency: the screen was still showing until the next. */
export const SCREEN_GAP_MS = 5_000;

/**
 * Each screen's span: from its capture until just before the next screen
 * starts (or, after a longer gap such as a paused capture, its recorded
 * end; the last screen: its end, else the end of the meeting). A span
 * always ends before the next screen starts, so it never captures its
 * neighbour.
 */
export function screenSpans(screens: readonly TimedItem[], meetingEndMs: number): Map<string, Span> {
    const starts = [...new Set(screens.map((s) => s.timestamp_ms).filter(Number.isFinite))].sort((a, b) => a - b);
    // The first start after `t` (binary search: meetings have thousands of screens)
    const nextAfter = (t: number): number | undefined => {
        let lo = 0;
        let hi = starts.length;
        while (lo < hi) {
            const mid = (lo + hi) >> 1;
            if (starts[mid] > t) hi = mid;
            else lo = mid + 1;
        }
        return starts[lo];
    };
    const out = new Map<string, Span>();
    for (const s of screens) {
        const start = s.timestamp_ms;
        if (!Number.isFinite(start)) continue;
        const next = nextAfter(start);
        let end = s.end_ms ?? (next !== undefined ? next - 1 : meetingEndMs);
        if (next !== undefined) end = next - end <= SCREEN_GAP_MS ? next - 1 : Math.min(end, next - 1);
        if (!(end > start)) end = start + (next === undefined ? 1000 : 1);
        out.set(s.id, [start, end]);
    }
    return out;
}

/** A line's span (its start to its end as the backend reads it), or null
 *  when it has no usable time: no time range can reach such a line. */
export function lineSpan(line: LineItem): Span | null {
    const { timestamp_ms: s, end_ms: e } = line;
    if (e === null || e === undefined || !Number.isFinite(s) || !Number.isFinite(e)) return null;
    return [s, Math.max(e, s + 1)];
}

/** Sort spans and merge the ones that overlap or touch (≤ 1 ms apart). */
export function mergeSpans(spans: readonly Span[]): Span[] {
    const v = spans.filter(([a, b]) => b >= a).map(([a, b]) => [a, b] as Span);
    v.sort((x, y) => x[0] - y[0] || x[1] - y[1]);
    const out: Span[] = [];
    for (const [a, b] of v) {
        const last = out[out.length - 1];
        if (last && a <= last[1] + 1) last[1] = Math.max(last[1], b);
        else out.push([a, b]);
    }
    return out;
}

/** `spans` minus `cut` (whole ms, both ends included; pieces left at
 *  least 1 ms long). */
export function subtractSpans(spans: readonly Span[], cut: readonly Span[]): Span[] {
    let out = mergeSpans(spans);
    for (const [c, d] of mergeSpans(cut)) {
        const next: Span[] = [];
        for (const [a, b] of out) {
            if (d < a || c > b) {
                next.push([a, b]);
                continue;
            }
            // (a piece shorter than a millisecond is dropped: no time to select)
            if (c - 1 > a) next.push([a, c - 1]);
            if (b > d + 1) next.push([d + 1, b]);
        }
        out = next;
    }
    return out;
}

const inSpans = (t: number, ranges: readonly Span[]) => ranges.some(([a, b]) => t >= a && t <= b);

/** How much of a line a set of disjoint spans removes (the backend's rules). */
export function lineCoverage(line: LineItem, ranges: readonly Span[]): "whole" | "split" | "none" {
    if (line.words === 0 || ranges.length === 0) return "none";
    const span = lineSpan(line);
    if (!span) return "none";
    const mids = line.word_mids_ms;
    if (mids && mids.length > 0) {
        const n = mids.filter((t) => inSpans(t, ranges)).length;
        return n === 0 ? "none" : n === mids.length ? "whole" : "split";
    }
    const [s, e] = span;
    let total = 0;
    let touched = false;
    for (const [a, b] of ranges) {
        const overlap = Math.min(b, e) - Math.max(a, s);
        if (overlap < 0 || (overlap === 0 && !(s >= a && s <= b))) continue;
        total += overlap;
        touched = true;
    }
    return touched && total * 2 >= e - s ? "whole" : "none";
}

/** The spans of the runs of adjacent picks in one pane (a run covers the
 *  gaps between its items). Items without a span are skipped. */
export function runSpans<T extends { id: string }>(
    items: readonly T[],
    ids: ReadonlySet<string>,
    spanOf: (x: T) => Span | null | undefined,
): Span[] {
    const out: Span[] = [];
    for (const run of selectedRuns(items, ids)) {
        const spans = run.map(spanOf).filter((s): s is Span => !!s);
        if (spans.length === 0) continue;
        out.push([Math.min(...spans.map((s) => s[0])), Math.max(...spans.map((s) => s[1]))]);
    }
    return mergeSpans(out);
}

export interface LinkedSelection {
    /** The time spans a Delete/Strike removes: merged, in time order */
    ranges: Span[];
    /** Screens captured inside them, in strip order */
    screenIds: string[];
    /** Lines they remove (whole or split), in transcript order */
    lineIds: string[];
    /** Of those, the lines only partly inside (split by word timings) */
    splitLineIds: ReadonlySet<string>;
    /** Picked lines with no usable time: removed whole, on their own */
    unplacedLineIds: string[];
}

export const EMPTY_LINKED: LinkedSelection = {
    ranges: [],
    screenIds: [],
    lineIds: [],
    splitLineIds: new Set(),
    unplacedLineIds: [],
};

/** Screens and lines selected together, linked by time (see the top). */
export function linkSelection(args: {
    screens: readonly TimedItem[];
    lines: readonly LineItem[];
    screenSel: Selection;
    lineSel: Selection;
    meetingEndMs: number;
    /** An exact span picked as such (Last N minutes, From here to the end,
     *  Select all) */
    hint?: Span | null;
}): LinkedSelection {
    const { screens, lines, screenSel, lineSel, meetingEndMs, hint } = args;
    const sSpans = screenSpans(screens, meetingEndMs);
    const include: Span[] = [
        ...runSpans(screens, screenSel.ids, (s) => sSpans.get(s.id)),
        ...runSpans(lines, lineSel.ids, lineSpan),
    ];
    if (hint && hint[1] > hint[0]) include.push(hint);
    const exclude: Span[] = [];
    for (const s of screens) {
        const sp = screenSel.excluded?.has(s.id) ? sSpans.get(s.id) : undefined;
        if (sp) exclude.push(sp);
    }
    for (const l of lines) {
        const sp = lineSel.excluded?.has(l.id) ? lineSpan(l) : null;
        if (sp) exclude.push(sp);
    }
    const ranges = subtractSpans(include, exclude);
    const screenIds = screens.filter((s) => inSpans(s.timestamp_ms, ranges)).map((s) => s.id);
    const lineIds: string[] = [];
    const split = new Set<string>();
    const unplaced: string[] = [];
    for (const l of lines) {
        if (!lineSpan(l)) {
            if (lineSel.ids.has(l.id) && !lineSel.excluded?.has(l.id) && l.words !== 0) unplaced.push(l.id);
            continue;
        }
        const c = lineCoverage(l, ranges);
        if (c === "none") continue;
        lineIds.push(l.id);
        if (c === "split") split.add(l.id);
    }
    return { ranges, screenIds, lineIds, splitLineIds: split, unplacedLineIds: unplaced };
}

/** The part of a backend preview that says what a Delete removes. */
export interface PreviewIds {
    screen_ids: readonly string[];
    lines: readonly { transcript_id: number; whole: boolean }[];
}

/** Whether the backend's preview of the spans removes exactly what the
 *  linked selection highlights (same screens, same lines, same split ones).
 *  If not, the UI shows the preview instead of deleting straight away. */
export function matchesPreview(linked: LinkedSelection, preview: PreviewIds): boolean {
    const same = (a: Iterable<string>, b: Iterable<string>) => {
        const x = new Set(a);
        const y = new Set(b);
        return x.size === y.size && [...x].every((v) => y.has(v));
    };
    const whole = new Set(preview.lines.filter((l) => l.whole).map((l) => String(l.transcript_id)));
    const split = preview.lines.filter((l) => !l.whole && !whole.has(String(l.transcript_id))).map((l) => String(l.transcript_id));
    return (
        same(linked.screenIds, preview.screen_ids) &&
        same(linked.lineIds, preview.lines.map((l) => String(l.transcript_id))) &&
        same(linked.splitLineIds, split)
    );
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

export function plural(n: number, one: string, many: string): string {
    return `${n} ${n === 1 ? one : many}`;
}
