// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    EMPTY_SELECTION,
    clickScreen,
    clockAt,
    durationLabel,
    orderedIds,
    parseClock,
    pruneSelection,
    rangeOfScreens,
    selectAll,
    selectLastMinutes,
    selectToEnd,
    spanLabel,
    summarize,
    type ClickMods,
    type ScreenItem,
    type Selection,
} from "./screenSelection.ts";

// Ten screens, one a minute, the meeting starting at 10:30:00 UTC
const STARTED = "2026-10-05T10:30:00Z";
const items: ScreenItem[] = Array.from({ length: 10 }, (_, i) => ({ id: `s${i}`, timestamp_ms: i * 60_000 }));
const plain: ClickMods = { shift: false, toggle: false, selectMode: false };
const shift: ClickMods = { shift: true, toggle: false, selectMode: false };
const cmd: ClickMods = { shift: false, toggle: true, selectMode: false };

function click(sel: Selection, id: string, mods: ClickMods, current: string | null = null) {
    return clickScreen(sel, items, id, mods, current);
}

test("Shift-click selects a contiguous, inclusive range from the anchor", () => {
    let r = click(EMPTY_SELECTION, "s2", plain);
    assert.equal(r.view, true);
    assert.equal(r.sel.ids.size, 0, "a plain click only views");
    r = click(r.sel, "s5", shift);
    assert.deepEqual(orderedIds(items, r.sel), ["s2", "s3", "s4", "s5"]);
    // Another Shift-click replaces the range (Finder), it doesn't add to it
    r = click(r.sel, "s3", shift);
    assert.deepEqual(orderedIds(items, r.sel), ["s2", "s3"]);
    // Backwards works too
    r = click(r.sel, "s0", shift);
    assert.deepEqual(orderedIds(items, r.sel), ["s0", "s1", "s2"]);
});

test("Shift-click with no anchor starts at the viewed screen, which is visibly selected", () => {
    const r = click(EMPTY_SELECTION, "s6", shift, "s4");
    assert.deepEqual(orderedIds(items, r.sel), ["s4", "s5", "s6"]);
    assert.equal(summarize(items, r.sel)?.count, 3);
});

test("Cmd-click toggles and keeps earlier picks; Shift then extends from it", () => {
    let r = click(EMPTY_SELECTION, "s1", cmd);
    r = click(r.sel, "s7", cmd);
    assert.deepEqual(orderedIds(items, r.sel), ["s1", "s7"]);
    r = click(r.sel, "s9", shift);
    assert.deepEqual(orderedIds(items, r.sel), ["s1", "s7", "s8", "s9"]);
    r = click(r.sel, "s8", cmd);
    assert.deepEqual(orderedIds(items, r.sel), ["s1", "s7", "s9"]);
    assert.equal(summarize(items, r.sel)?.groups, 3);
});

test("select mode toggles on a plain click", () => {
    const sm: ClickMods = { ...plain, selectMode: true };
    let r = click(EMPTY_SELECTION, "s3", sm);
    r = click(r.sel, "s4", sm);
    r = click(r.sel, "s3", sm);
    assert.equal(r.view, false);
    assert.deepEqual(orderedIds(items, r.sel), ["s4"]);
});

test("the count shown always equals the ids sent, even with stale ids", () => {
    // Four picked, plus one that is no longer on the timeline (deleted, or
    // filtered out on reload): it must be neither counted nor sent
    const sel: Selection = { ids: new Set(["s1", "s2", "s3", "s4", "gone"]), anchor: "s1", base: new Set() };
    const ids = orderedIds(items, sel);
    const sum = summarize(items, sel)!;
    assert.deepEqual(ids, ["s1", "s2", "s3", "s4"]);
    assert.equal(sum.count, ids.length);
    assert.deepEqual(sum.ids, ids);
    assert.deepEqual(orderedIds(items, pruneSelection(items, sel)), ids);
    assert.equal(pruneSelection(items, sel).ids.size, 4);
});

test("Select all, From here to the end, Last N minutes", () => {
    assert.equal(orderedIds(items, selectAll(items)).length, 10);
    assert.deepEqual(orderedIds(items, selectToEnd(items, "s7")), ["s7", "s8", "s9"]);
    // Meeting ends at 9:30 in; the last 3 minutes are 6:30–9:30
    const { sel, range } = selectLastMinutes(items, 570_000, 3);
    assert.deepEqual(range, [390_000, 570_000]);
    assert.deepEqual(orderedIds(items, sel), ["s7", "s8", "s9"]);
});

test("a screen selection becomes a time range", () => {
    const withEnds: ScreenItem[] = items.map((x) => ({ ...x, end_ms: x.timestamp_ms + 45_000 }));
    assert.deepEqual(rangeOfScreens(withEnds, ["s3", "s5"], 600_000), [180_000, 345_000]);
    // No end recorded: until the next screen, else the end of the meeting
    assert.deepEqual(rangeOfScreens(items, ["s3", "s5"], 600_000), [180_000, 360_000]);
    assert.deepEqual(rangeOfScreens(items, ["s9"], 600_000), [540_000, 600_000]);
    assert.equal(rangeOfScreens(items, [], 600_000), null);
});

test("clock labels and parsing (local time; npm test runs in UTC)", () => {
    assert.equal(clockAt(STARTED, 11 * 60_000 + 5_000), "10:41:05");
    assert.equal(spanLabel(STARTED, 11 * 60_000, 23 * 60_000), "10:41–10:53");
    assert.equal(spanLabel(STARTED, 11 * 60_000, 11 * 60_000 + 30_000), "10:41:00–10:41:30");
    assert.equal(durationLabel(12 * 60_000), "12 min");
    assert.equal(durationLabel(45_000), "45 s");
    assert.equal(parseClock(STARTED, "10:41", 0), 11 * 60_000);
    assert.equal(parseClock(STARTED, "10:41:05", 0), 11 * 60_000 + 5_000);
    assert.equal(parseClock(STARTED, "25:00", 0), null);
    // A meeting that runs past midnight
    const late = "2026-10-05T23:50:00Z";
    assert.equal(parseClock(late, "00:05", 10 * 60_000), 15 * 60_000);
});
