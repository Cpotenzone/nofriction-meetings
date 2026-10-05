// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    EMPTY_SELECTION,
    clickItem,
    lineCoverage,
    lineSpan,
    linkSelection,
    matchesPreview,
    mergeSpans,
    orderedIds,
    pruneSelection,
    runSpans,
    screenSpans,
    selectAll,
    selectLastMinutes,
    selectToEnd,
    subtractSpans,
    summarize,
    type ClickMods,
    type LineItem,
    type Selection,
    type Span,
    type TimedItem,
} from "./timelineSelection.ts";

const plain: ClickMods = { shift: false, toggle: false, selectMode: false };
const shift: ClickMods = { shift: true, toggle: false, selectMode: false };
const cmd: ClickMods = { shift: false, toggle: true, selectMode: false };

// A 10-minute meeting. Screens every 2 minutes (s0 at 0:00 … s4 at 8:00),
// the last one with a recorded end. Lines every 30 s from 0:10, each 20 s
// long (no word timings), except l3 which has word timings.
const END = 600_000;
const screens: TimedItem[] = [
    { id: "s0", timestamp_ms: 0 },
    { id: "s1", timestamp_ms: 120_000 },
    { id: "s2", timestamp_ms: 240_000 },
    { id: "s3", timestamp_ms: 360_000 },
    { id: "s4", timestamp_ms: 480_000, end_ms: 540_000 },
];
const lines: LineItem[] = Array.from({ length: 19 }, (_, i) => ({
    id: `l${i}`,
    timestamp_ms: 10_000 + i * 30_000,
    end_ms: 30_000 + i * 30_000,
    words: 8,
}));
// l3: 100 s–120 s, four words; the last two after 115 s
lines[3] = { ...lines[3], word_mids_ms: [101_000, 106_000, 116_000, 119_000], words: 4 };

function link(screenSel: Selection, lineSel: Selection, hint: Span | null = null) {
    return linkSelection({ screens, lines, screenSel, lineSel, meetingEndMs: END, hint });
}

test("lines: a plain click selects only that line; Shift extends; ⌘ toggles", () => {
    const opts = { plain: "replace" as const };
    let r = clickItem(EMPTY_SELECTION, lines, "l2", plain, null, opts);
    assert.equal(r.view, true, "it also seeks to the line");
    assert.deepEqual(orderedIds(lines, r.sel), ["l2"]);
    r = clickItem(r.sel, lines, "l6", shift, null, opts);
    assert.deepEqual(orderedIds(lines, r.sel), ["l2", "l3", "l4", "l5", "l6"]);
    // Another Shift-click replaces the range part (Finder)
    r = clickItem(r.sel, lines, "l4", shift, null, opts);
    assert.deepEqual(orderedIds(lines, r.sel), ["l2", "l3", "l4"]);
    r = clickItem(r.sel, lines, "l9", cmd, null, opts);
    r = clickItem(r.sel, lines, "l3", cmd, null, opts);
    assert.deepEqual(orderedIds(lines, r.sel), ["l2", "l4", "l9"]);
    assert.equal(summarize(lines, r.sel)?.groups, 3);
    // A plain click starts over
    r = clickItem(r.sel, lines, "l12", plain, null, opts);
    assert.deepEqual(orderedIds(lines, r.sel), ["l12"]);
});

test("Select all, From here to the end and Last N minutes work on lines", () => {
    assert.equal(orderedIds(lines, selectAll(lines)).length, 19);
    // The owner's fix: click the line where the deleted screens started, then
    // From here to the end
    assert.deepEqual(orderedIds(lines, selectToEnd(lines, "l16")), ["l16", "l17", "l18"]);
    const { sel, range } = selectLastMinutes(lines, END, 2);
    assert.deepEqual(range, [480_000, 600_000]);
    assert.deepEqual(orderedIds(lines, sel), ["l16", "l17", "l18"]);
});

test("spans: merge overlapping/adjacent, subtract, runs cover their gaps", () => {
    assert.deepEqual(mergeSpans([[50, 60], [0, 10], [11, 20], [5, 8], [100, 90]]), [[0, 20], [50, 60]]);
    assert.deepEqual(mergeSpans([]), []);
    assert.deepEqual(subtractSpans([[0, 100]], [[20, 30], [90, 120]]), [[0, 19], [31, 89]]);
    assert.deepEqual(subtractSpans([[0, 10]], [[0, 10]]), []);
    // l1..l2 are one run (30 s gap included); l5 is its own
    const ids = new Set(["l1", "l2", "l5"]);
    assert.deepEqual(runSpans(lines, ids, lineSpan), [[40_000, 90_000], [160_000, 180_000]]);
});

test("screen spans end before the next screen; without an end, at the next one or the meeting end", () => {
    const sp = screenSpans(screens, END);
    assert.deepEqual(sp.get("s0"), [0, 119_999]);
    assert.deepEqual(sp.get("s4"), [480_000, 540_000], "recorded end");
    const noEnd = screenSpans([{ id: "a", timestamp_ms: 0 }, { id: "b", timestamp_ms: 5_000 }], 9_000);
    assert.deepEqual(noEnd.get("b"), [5_000, 9_000]);
    // An end recorded past the next screen is cut short of it
    const late = screenSpans([{ id: "a", timestamp_ms: 0, end_ms: 7_000 }, { id: "b", timestamp_ms: 5_000 }], 9_000);
    assert.deepEqual(late.get("a"), [0, 4_999]);
    // Capture records each end about a second before the next screen: the
    // screen was still showing, so its span reaches the next one (and the
    // screen video of that second goes with it)
    const real = screenSpans([{ id: "a", timestamp_ms: 0, end_ms: 59_266 }, { id: "b", timestamp_ms: 60_316 }], 90_000);
    assert.deepEqual(real.get("a"), [0, 60_315]);
    // After a long gap (capture paused), the recorded end stands
    const paused = screenSpans([{ id: "a", timestamp_ms: 0, end_ms: 30_000 }, { id: "b", timestamp_ms: 600_000 }], 900_000);
    assert.deepEqual(paused.get("a"), [0, 30_000]);
});

test("linked: selecting screens selects what was said while they were shown", () => {
    const r = link({ ...EMPTY_SELECTION, ids: new Set(["s1"]) }, EMPTY_SELECTION);
    assert.deepEqual(r.ranges, [[120_000, 239_999]]);
    assert.deepEqual(r.screenIds, ["s1"], "the next screen is not pulled in");
    // l4 (130–150 s) … l7 (220–240 s, 19.999 of 20 s inside) are inside
    assert.deepEqual(r.lineIds, ["l4", "l5", "l6", "l7"]);
    assert.equal(r.splitLineIds.size, 0);
    // l3 has word timings: its last two words (116 s, 119 s) are before s1
    const r2 = link({ ...EMPTY_SELECTION, ids: new Set(["s0"]) }, EMPTY_SELECTION);
    assert.ok(r2.lineIds.includes("l3"));
    assert.equal(r2.splitLineIds.has("l3"), false, "all of l3's words are before 120 s");
});

test("linked: selecting lines selects the screens captured while they were said", () => {
    // l7 (220–240 s) … l8 (250–270 s): s2 was captured at 240 s
    const r = link(EMPTY_SELECTION, { ...EMPTY_SELECTION, ids: new Set(["l7", "l8"]) });
    assert.deepEqual(r.ranges, [[220_000, 270_000]]);
    assert.deepEqual(r.screenIds, ["s2"]);
    assert.deepEqual(r.lineIds, ["l7", "l8"]);
    // A line alone, with no screen captured during it: no screens
    assert.deepEqual(link(EMPTY_SELECTION, { ...EMPTY_SELECTION, ids: new Set(["l5"]) }).screenIds, []);
});

test("linked: word timings split a line at the edge of the span", () => {
    // A span from 110 s: l3's words at 116 s and 119 s go, 101 s and 106 s stay
    assert.equal(lineCoverage(lines[3], [[110_000, 200_000]]), "split");
    assert.equal(lineCoverage(lines[3], [[100_000, 200_000]]), "whole");
    assert.equal(lineCoverage(lines[3], [[120_000, 200_000]]), "none");
    // Without timings, at least half must be inside (l4: 130–150 s)
    assert.equal(lineCoverage(lines[4], [[140_000, 200_000]]), "whole");
    assert.equal(lineCoverage(lines[4], [[141_000, 200_000]]), "none");
    // …adding up every span
    assert.equal(lineCoverage(lines[4], [[130_000, 135_000], [144_000, 150_000]]), "whole");
    // A line holding only strike markers has nothing to remove
    assert.equal(lineCoverage({ ...lines[4], words: 0 }, [[0, END]]), "none");
    const r = link(EMPTY_SELECTION, EMPTY_SELECTION, [110_000, 125_000]);
    assert.deepEqual(r.lineIds, ["l3"]);
    assert.ok(r.splitLineIds.has("l3"));
    assert.deepEqual(r.screenIds, ["s1"]);
});

test("linked: picks in both panes add up; ⌘-click on a highlighted item turns its time off", () => {
    let screenSel: Selection = { ...EMPTY_SELECTION, ids: new Set(["s0"]) };
    let lineSel: Selection = { ...EMPTY_SELECTION, ids: new Set(["l15"]) }; // 460–480 s: s4 is captured at 480 s
    let r = link(screenSel, lineSel);
    assert.equal(r.ranges.length, 2, "two separate groups");
    assert.deepEqual(r.screenIds, ["s0", "s4"]);
    // l1 is highlighted only because s0 is picked; ⌘-click turns it off
    const covered = new Set(r.lineIds);
    assert.ok(covered.has("l1") && !lineSel.ids.has("l1"));
    lineSel = clickItem(lineSel, lines, "l1", cmd, null, { covered }).sel;
    r = link(screenSel, lineSel);
    assert.equal(r.lineIds.includes("l1"), false);
    assert.ok(r.lineIds.includes("l0") && r.lineIds.includes("l2"), "its neighbours stay");
    assert.equal(r.ranges.length, 3, "s0's span now has a hole");
    // ⌘-click it again: back in
    lineSel = clickItem(lineSel, lines, "l1", cmd, null, { covered: new Set(r.lineIds) }).sel;
    assert.ok(link(screenSel, lineSel).lineIds.includes("l1"));
    // And a picked screen toggles off like before
    screenSel = clickItem(screenSel, screens, "s0", cmd, null, { covered: new Set(r.screenIds) }).sel;
    assert.equal(link(screenSel, lineSel).screenIds.includes("s0"), false);
});

test("linked: From here to the end in the transcript reaches the end of the meeting", () => {
    // The owner's case: screens after 8:00 were deleted, the lines stayed.
    const noLate = screens.filter((s) => s.timestamp_ms < 480_000);
    const lineSel = selectToEnd(lines, "l16");
    const r = linkSelection({ screens: noLate, lines, screenSel: EMPTY_SELECTION, lineSel, meetingEndMs: END, hint: [490_000, END] });
    assert.deepEqual(r.ranges, [[490_000, END]]);
    assert.deepEqual(r.lineIds, ["l16", "l17", "l18"]);
    assert.deepEqual(r.screenIds, []);
});

test("linked: lines without a usable time are kept as picks, never linked", () => {
    const odd: LineItem[] = [...lines, { id: "x", timestamp_ms: 0, end_ms: null, words: 3 }];
    const r = linkSelection({
        screens,
        lines: odd,
        screenSel: EMPTY_SELECTION,
        lineSel: { ...EMPTY_SELECTION, ids: new Set(["x"]) },
        meetingEndMs: END,
    });
    assert.deepEqual(r.ranges, []);
    assert.deepEqual(r.unplacedLineIds, ["x"]);
    assert.deepEqual(r.screenIds, []);
    // …and a span covering 0:00 doesn't pull it in
    const all = linkSelection({ screens, lines: odd, screenSel: selectAll(screens), lineSel: EMPTY_SELECTION, meetingEndMs: END, hint: [0, END] });
    assert.equal(all.lineIds.includes("x"), false);
    assert.equal(all.lineIds.length, 19);
});

test("empty selections select nothing", () => {
    const r = link(EMPTY_SELECTION, EMPTY_SELECTION);
    assert.deepEqual(r, { ranges: [], screenIds: [], lineIds: [], splitLineIds: new Set(), unplacedLineIds: [] });
    assert.equal(summarize(lines, EMPTY_SELECTION), null);
});

test("the count shown always equals the ids sent", () => {
    // One pane: the bar's count is the length of the list that is sent
    const sel: Selection = { ids: new Set(["l1", "l2", "gone"]), anchor: "l1", base: new Set() };
    const ids = orderedIds(lines, sel);
    assert.deepEqual(ids, ["l1", "l2"]);
    assert.equal(summarize(lines, sel)?.count, ids.length);
    assert.deepEqual(orderedIds(lines, pruneSelection(lines, sel)), ids);
    // Linked: each highlighted item is listed once, in pane order
    const r = link(selectAll(screens), selectAll(lines), [0, END]);
    assert.equal(r.screenIds.length, 5);
    assert.equal(r.lineIds.length, 19);
    assert.equal(new Set(r.lineIds).size, r.lineIds.length);
});

test("the backend preview must remove exactly what is highlighted", () => {
    const r = link(EMPTY_SELECTION, EMPTY_SELECTION, [110_000, 125_000]);
    const ok = { screen_ids: ["s1"], lines: [{ transcript_id: 3, whole: false }] };
    const asIds = { ...r, lineIds: r.lineIds.map((x) => x.slice(1)), splitLineIds: new Set([...r.splitLineIds].map((x) => x.slice(1))) };
    assert.equal(matchesPreview(asIds, ok), true);
    assert.equal(matchesPreview(asIds, { ...ok, lines: [{ transcript_id: 3, whole: true }] }), false, "whole vs split");
    assert.equal(matchesPreview(asIds, { ...ok, screen_ids: [] }), false);
    // A line cut by two spans appears twice in the preview: still one line
    assert.equal(
        matchesPreview(asIds, { ...ok, lines: [{ transcript_id: 3, whole: false }, { transcript_id: 3, whole: false }] }),
        true,
    );
});
