// Tests for the PREVIEW-ONLY lesson logic (docs/LESSONS.md §15). These cover
// the live UI hints only; authoritative grading is Rust and is tested there.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
    previewNumeric,
    previewMastery,
    resumeExplainerIndex,
    examUnlockedPreview,
} from "./lessonLogic.ts";

test("previewNumeric: compatible units convert before comparison", () => {
    const abs = { kind: "abs" as const, value: 0.001 };
    assert.equal(previewNumeric("4000 mg", 4.0, "g", abs), "correct");
    assert.equal(previewNumeric("250 ml", 0.25, "L", abs), "correct");
    const tabs = { kind: "abs" as const, value: 0.01 };
    assert.equal(previewNumeric("25 °c", 298.15, "K", tabs), "correct");
});

test("previewNumeric: the full Rust unit set is mirrored (no false unreadable)", () => {
    // Units that the Rust grader accepts must not read as "unreadable" in the
    // live preview (NIT-8): µg/ug, dL, cm3/cc, kPa, mmHg/torr, Pa, °K.
    const rel = { kind: "rel" as const, value: 0.01 };
    assert.equal(previewNumeric("1000000 ug", 1.0, "g", rel), "correct"); // 1e6 µg = 1 g
    assert.equal(previewNumeric("2 dl", 0.2, "L", rel), "correct"); // 2 dL = 0.2 L
    assert.equal(previewNumeric("250 cm3", 0.25, "L", rel), "correct"); // 250 cm³ = 0.25 L
    assert.equal(previewNumeric("250 cc", 0.25, "L", rel), "correct");
    assert.equal(previewNumeric("101.325 kpa", 1.0, "atm", rel), "correct"); // 1 atm
    assert.equal(previewNumeric("760 mmhg", 1.0, "atm", rel), "correct");
    assert.equal(previewNumeric("760 torr", 1.0, "atm", rel), "correct");
    assert.equal(previewNumeric("101325 pa", 1.0, "atm", rel), "correct");
    assert.equal(previewNumeric("273.15 °k", 273.15, "K", rel), "correct");
});

test("previewNumeric: incompatible units are wrong, unknown units unreadable", () => {
    const abs = { kind: "abs" as const, value: 0.001 };
    assert.equal(previewNumeric("4 L", 4.0, "g", abs), "wrong");
    assert.equal(previewNumeric("4 foo", 4.0, "g", abs), "unreadable");
});

test("previewNumeric: a bare number compares in the declared unit", () => {
    const exact = { kind: "exact" as const, value: 0 };
    assert.equal(previewNumeric("4", 4.0, "g", exact), "correct");
});

test("previewNumeric: scientific notation parses", () => {
    const rel = { kind: "rel" as const, value: 0.01 };
    assert.equal(previewNumeric("6.022e23", 6.022e23, null, rel), "correct");
    assert.equal(previewNumeric("6.022 x 10^23", 6.022e23, null, rel), "correct");
});

test("previewNumeric: hostile input never throws", () => {
    const abs = { kind: "abs" as const, value: 0.001 };
    for (const g of ["", "   ", "abc", "NaN", "----", "9".repeat(400)]) {
        assert.doesNotThrow(() => previewNumeric(g, 1.0, "g", abs));
    }
    assert.equal(previewNumeric("", 1.0, "g", abs), "unreadable");
});

test("previewMastery: first attempt seeds the score; retry credit is lower", () => {
    assert.equal(previewMastery(0, 0, true, true), 100);
    assert.equal(previewMastery(0, 0, true, false), 70);
    assert.equal(previewMastery(0, 0, false, false), 0);
    // EMA blends on subsequent attempts.
    const next = previewMastery(100, 1, false, false);
    assert.ok(next < 100 && next > 0);
});

test("resumeExplainerIndex: points at the first unseen explainer", () => {
    assert.equal(resumeExplainerIndex(["a", "b", "c"], ["a"]), 1);
    assert.equal(resumeExplainerIndex(["a", "b"], []), 0);
    // All seen → stay on the last.
    assert.equal(resumeExplainerIndex(["a", "b"], ["a", "b"]), 1);
});

test("examUnlockedPreview: all checkpoints passed unlocks", () => {
    assert.equal(examUnlockedPreview(6, 6), true);
    assert.equal(examUnlockedPreview(5, 6), false);
    assert.equal(examUnlockedPreview(0, 0), false);
});
