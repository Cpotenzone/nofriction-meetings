// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    DURATION_CHOICES,
    buildStartPlan,
    canonicalClassName,
    choiceForKey,
    classSuggestions,
    formatClock,
    normalizeClassName,
    parseChoice,
    pickerKeyAction,
    secondsUntil,
    segmentCarryOver,
    timerLabel,
} from "./recordPlan.ts";

test("five choices in order, keys 1–5, the last is No limit", () => {
    assert.deepEqual(DURATION_CHOICES.map((c) => c.value), ["15", "30", "60", "90", "none"]);
    assert.deepEqual(DURATION_CHOICES.map((c) => c.key), ["1", "2", "3", "4", "5"]);
    assert.equal(DURATION_CHOICES[4].spoken, "No limit");
    assert.equal(choiceForKey("1"), "15");
    assert.equal(choiceForKey("5"), "none");
    assert.equal(choiceForKey("6"), null);
    assert.equal(choiceForKey("0"), null);
});

test("remembered choice parses; unknown values mean no limit", () => {
    assert.equal(parseChoice("60"), "60");
    assert.equal(parseChoice(" NONE "), "none");
    assert.equal(parseChoice("45"), "none");
    assert.equal(parseChoice(null), "none");
    assert.equal(parseChoice(undefined), "none");
});

test("keyboard: digits pick, Enter starts, Esc cancels", () => {
    assert.deepEqual(pickerKeyAction({ key: "3" }), { type: "select", choice: "60" });
    assert.deepEqual(pickerKeyAction({ key: "5" }), { type: "select", choice: "none" });
    assert.deepEqual(pickerKeyAction({ key: "Enter" }), { type: "start" });
    assert.deepEqual(pickerKeyAction({ key: "Escape" }), { type: "cancel" });
    assert.deepEqual(pickerKeyAction({ key: "x" }), { type: "none" });
    assert.deepEqual(pickerKeyAction({ key: "9" }), { type: "none" });
});

test("keyboard: digits type into the Class field; Enter and Esc still work there", () => {
    assert.deepEqual(pickerKeyAction({ key: "1", inTextField: true }), { type: "none" }, "BIO 101 can be typed");
    assert.deepEqual(pickerKeyAction({ key: "Enter", inTextField: true }), { type: "start" });
    assert.deepEqual(pickerKeyAction({ key: "Escape", inTextField: true }), { type: "cancel" });
});

test("keyboard: modified keys and IME composition are left alone", () => {
    assert.deepEqual(pickerKeyAction({ key: "1", metaKey: true }), { type: "none" }, "⌘1 is the Live view");
    assert.deepEqual(pickerKeyAction({ key: "Enter", ctrlKey: true }), { type: "none" });
    assert.deepEqual(pickerKeyAction({ key: "2", altKey: true }), { type: "none" });
    assert.deepEqual(pickerKeyAction({ key: "Enter", isComposing: true }), { type: "none" });
    assert.deepEqual(pickerKeyAction({ key: "Escape", metaKey: true }), { type: "cancel" });
});

test("class names are cleaned like the backend", () => {
    assert.equal(normalizeClassName("  BIO 101 —  Cell\n Biology "), "BIO 101 — Cell Biology");
    assert.equal(normalizeClassName("   "), null);
    assert.equal(normalizeClassName(""), null);
    assert.equal(normalizeClassName(null), null);
    assert.equal(normalizeClassName("x".repeat(200))?.length, 80);
    assert.equal(normalizeClassName(`${"x".repeat(79)} y`), "x".repeat(79), "no trailing space after the cap");
});

test("a typed class joins an existing one ignoring case", () => {
    const recents = ["BIO 101 — Cell Biology", "HIST 200"];
    assert.equal(canonicalClassName("bio 101 — cell biology", recents), "BIO 101 — Cell Biology");
    assert.equal(canonicalClassName("MATH 3", recents), "MATH 3");
    assert.equal(canonicalClassName(" ", recents), null);
});

test("suggestions: recents when empty, prefix matches before contains", () => {
    const recents = ["CHEM 110", "BIO 101", "Biochem 300", "HIST 200"];
    assert.deepEqual(classSuggestions("", recents, 2), ["CHEM 110", "BIO 101"]);
    assert.deepEqual(classSuggestions("bio", recents), ["BIO 101", "Biochem 300"]);
    assert.deepEqual(classSuggestions("chem", recents), ["CHEM 110", "Biochem 300"]);
    assert.deepEqual(classSuggestions("zzz", recents), []);
});

test("the sheet's plan is remembered and carries the class only when set", () => {
    assert.deepEqual(buildStartPlan("60", "", []), { duration: "60", className: null, remember: true });
    assert.deepEqual(buildStartPlan("none", " bio 101 ", ["BIO 101"]), { duration: "none", className: "BIO 101", remember: true });
});

test("clock and timer labels", () => {
    assert.equal(formatClock(754), "12:34");
    assert.equal(formatClock(3725), "1:02:05");
    assert.equal(formatClock(-3), "0:00");
    const now = Date.parse("2026-10-06T10:00:00Z");
    assert.equal(secondsUntil("2026-10-06T10:12:34Z", now), 754);
    assert.equal(secondsUntil("2026-10-06T09:00:00Z", now), 0);
    assert.equal(secondsUntil("not a date", now), 0);
    assert.equal(timerLabel({ deadline: "2026-10-06T10:12:34Z", startedAt: "2026-10-06T09:30:00Z", nowMs: now }), "12:34 left");
    assert.equal(timerLabel({ deadline: null, startedAt: "2026-10-06T09:30:00Z", nowMs: now }), "30:00", "elapsed without a limit");
    assert.equal(timerLabel({ deadline: null, startedAt: null, nowMs: now }), "");
});

test("a new segment keeps the class and the time left", () => {
    const now = Date.parse("2026-10-06T10:00:00Z");
    assert.deepEqual(segmentCarryOver({ deadline: "2026-10-06T10:14:01Z" }, "BIO 101", now), {
        duration: "15",
        className: "BIO 101",
        remember: false,
    });
    assert.deepEqual(segmentCarryOver({ deadline: null }, null, now), { duration: "none", className: null, remember: false });
    assert.deepEqual(segmentCarryOver(null, "  ", now), { duration: "none", className: null, remember: false });
});
