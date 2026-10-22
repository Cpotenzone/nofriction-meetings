// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    baseName,
    clock,
    contentsLine,
    defaultInclude,
    defaultSelection,
    hasAiParts,
    isTranscriptChanged,
    mergeCaptions,
    plainError,
    orderByTime,
    picturesNoun,
    sampleIndexes,
    uncaptioned,
    type DraftItem,
    type ExportDraft,
} from "./exportLogic.ts";

const item = (id: string, ms: number, source: "screen" | "photo" = "screen"): DraftItem => ({ id, ms, source, label: null });

const draft = (over: Partial<ExportDraft> = {}): ExportDraft => ({
    meeting_id: "m1",
    title: "Weekly sync",
    kind: "meeting",
    when: "Tuesday 21 October 2026, 14:00",
    duration: "42 min",
    folder: "2026-10-21 Weekly sync",
    notes: { generated_at: "2026-10-21T15:00:00Z", stale: false, summary: "We agreed." },
    guide_parts: [],
    marks: 2,
    links: 0,
    transcript_lines: 120,
    fingerprint: "abc",
    items: [],
    ...over,
});

test("clock matches the package's times", () => {
    assert.equal(clock(0), "0:00");
    assert.equal(clock(61_000), "1:01");
    assert.equal(clock(3_725_000), "1:02:05");
    assert.equal(clock(-1), "0:00");
});

test("sampleIndexes spreads evenly like the backend", () => {
    assert.deepEqual(sampleIndexes(3, 10), [0, 1, 2]);
    assert.deepEqual(sampleIndexes(100, 5), [0, 24, 49, 74, 99]);
    assert.deepEqual(sampleIndexes(10, 1), [0]);
    assert.deepEqual(sampleIndexes(0, 5), []);
});

test("the default picks keep every photo, then spread screens, in time order", () => {
    const items = [
        ...Array.from({ length: 40 }, (_, i) => item(`s${i}`, i * 60_000)),
        item("p1", 90_000, "photo"),
        item("p2", 30 * 60_000 + 1, "photo"),
    ];
    const picked = defaultSelection(items, 6);
    assert.equal(picked.length, 6);
    assert.ok(picked.includes("p1") && picked.includes("p2"), "photos are always picked");
    assert.deepEqual(picked.filter((id) => id.startsWith("s")), ["s0", "s13", "s26", "s39"]);
    const ms = picked.map((id) => items.find((i) => i.id === id)!.ms);
    assert.deepEqual(ms, [...ms].sort((a, b) => a - b), "time order");
    assert.deepEqual(defaultSelection([item("a", 5), item("b", 1)]), ["b", "a"]);
    assert.deepEqual(defaultSelection([]), []);
});

test("orderByTime keeps only the picked ones", () => {
    const items = [item("c", 30), item("a", 10), item("b", 10)];
    assert.deepEqual(orderByTime(items, new Set(["c", "b", "a"])), ["a", "b", "c"]);
    assert.deepEqual(orderByTime(items, new Set(["z"])), []);
});

test("everything that exists is ticked when the sheet opens", () => {
    const inc = defaultInclude(draft({ items: [item("a", 1)], guide_parts: ["flashcards"] }));
    assert.deepEqual(inc, { notes: true, highlights: true, screens: true, marks: true, links: false, email: true, guide: true, transcript: true });
    const bare = defaultInclude(draft({ notes: null, marks: 0, transcript_lines: 0 }));
    assert.equal(bare.notes, false);
    assert.equal(bare.screens, false);
    assert.equal(bare.transcript, false);
});

test("the pictures heading follows what's there", () => {
    assert.equal(picturesNoun([item("a", 1)]), "Screens");
    assert.equal(picturesNoun([item("a", 1, "photo")]), "Photos");
    assert.equal(picturesNoun([item("a", 1), item("b", 2, "photo")]), "Screens and photos");
});

test("the contents line names only ticked parts that exist", () => {
    const picked = [item("a", 1), item("b", 2), item("c", 3, "photo")];
    const d = draft({ guide_parts: ["quiz"], links: 1, items: picked });
    const inc = defaultInclude(d);
    assert.equal(
        contentsLine(d, inc, { picked, highlights: 3, hasEmail: true }),
        "Notes, 3 highlights, 2 screens, 1 photo, 2 marks, 1 link, follow-up email, review guide, transcript",
    );
    assert.equal(contentsLine(draft({ kind: "class", guide_parts: ["quiz"], marks: 0 }), { ...inc, notes: false, transcript: false, links: false, email: false }, { picked: [], highlights: 0, hasEmail: false }), "Study guide");
    const none = { notes: false, highlights: false, screens: false, marks: false, links: false, email: false, guide: false, transcript: false };
    assert.equal(contentsLine(d, none, { picked, highlights: 3, hasEmail: true }), "Just the title and date");
});

test("captions to write are the picked ones without one", () => {
    assert.deepEqual(uncaptioned(["a", "b", "c"], { a: "Roadmap", b: "  " }), ["b", "c"]);
});

test("AI parts are flagged only when they're in the package", () => {
    const d = draft({ notes: null, items: [item("a", 1)] });
    const inc = defaultInclude(d);
    assert.equal(hasAiParts(d, inc, { highlights: 0, captions: 0, hasEmail: false }), false);
    assert.equal(hasAiParts(d, inc, { highlights: 0, captions: 2, hasEmail: false }), true);
    assert.equal(hasAiParts(d, { ...inc, screens: false }, { highlights: 0, captions: 2, hasEmail: false }), false);
    assert.equal(hasAiParts(draft(), defaultInclude(draft()), { highlights: 0, captions: 0, hasEmail: false }), true, "saved notes are AI");
});

test("baseName takes the last path part", () => {
    assert.equal(baseName("/Users/a/Desktop/2026-10-21 Weekly sync.zip"), "2026-10-21 Weekly sync.zip");
    assert.equal(baseName("/Users/a/Desktop/2026-10-21 Weekly sync/"), "2026-10-21 Weekly sync");
});

test("AI captions never overwrite what the user typed", () => {
    const { captions, aiIds } = mergeCaptions({ a: "Mine", b: "  " }, new Set(["z"]), [
        { id: "a", caption: "AI a" },
        { id: "b", caption: "AI b" },
        { id: "c", caption: "AI c" },
    ]);
    assert.deepEqual(captions, { a: "Mine", b: "AI b", c: "AI c" });
    assert.deepEqual([...aiIds].sort(), ["b", "c", "z"]);
});

test("a changed transcript is recognised and shown without its prefix", () => {
    const e = "TRANSCRIPT_CHANGED: The transcript was edited while you were exporting.";
    assert.equal(isTranscriptChanged(e), true);
    assert.equal(isTranscriptChanged(new Error("Database busy")), false);
    assert.equal(plainError(e), "The transcript was edited while you were exporting.");
    assert.equal(plainError(new Error("Couldn't save the file")), "Couldn't save the file");
});
