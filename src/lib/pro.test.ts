// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    FREE_SUMMARY,
    PRO_FEATURE_KEYS,
    PRO_GROUPS,
    PRO_VALUE,
    groupFor,
    paywallHeadline,
    proFeatureFromError,
} from "./pro.ts";

test("the Rust feature gate's errors name the feature (entitlement::ProRequired)", () => {
    assert.equal(proFeatureFromError("PRO_REQUIRED:obsidian: Export to Obsidian is part of noFriction Pro."), "obsidian");
    assert.equal(proFeatureFromError(new Error("PRO_REQUIRED:sync: Sync is part of noFriction Pro.")), "sync");
    // Wrapped by a command
    assert.equal(proFeatureFromError("Couldn't save: PRO_REQUIRED:obsidian: Export to Obsidian is part of noFriction Pro."), "obsidian");
});

test("the AI gate's error means AI; unknown keys fall back to AI", () => {
    assert.equal(proFeatureFromError("PRO_REQUIRED: AI notes, summaries, chat and briefings are part of noFriction Pro."), "ai");
    assert.equal(proFeatureFromError("PRO_REQUIRED:teleport: nope"), "ai");
});

test("other errors aren't Pro errors", () => {
    assert.equal(proFeatureFromError("AI_UNREACHABLE: Can't reach the AI provider"), null);
    assert.equal(proFeatureFromError("CONSENT_REQUIRED:custom"), null);
});

test("a paywall opened by a feature says which feature", () => {
    assert.equal(paywallHeadline("sync"), "Sync is part of noFriction Pro");
    assert.equal(paywallHeadline("obsidian"), "Export to Obsidian is part of noFriction Pro");
    assert.equal(paywallHeadline("transcribe_playing"), "Transcribe what's playing is part of noFriction Pro");
    assert.equal(paywallHeadline(null), "noFriction Pro");
    for (const k of PRO_FEATURE_KEYS) assert.match(paywallHeadline(k), /part of noFriction Pro$/);
});

test("every feature maps to a listed group", () => {
    const ids = new Set(PRO_GROUPS.map((g) => g.id));
    for (const k of PRO_FEATURE_KEYS) assert.ok(ids.has(groupFor(k)!), k);
    assert.equal(groupFor(undefined), null);
});

test("copy: Free keeps recording, microphone transcription, search, Delete/Strike and export", () => {
    for (const w of ["recording", "microphone transcription", "search", "Delete and Strike", "JSON export"]) {
        assert.ok(FREE_SUMMARY.includes(w), w);
    }
    // Pro features never appear in the free line
    for (const w of ["Sync", "Obsidian", "Chat", "what's playing"]) assert.ok(!FREE_SUMMARY.includes(w), w);
});

test("copy: sentence case, no emoji", () => {
    const all = [PRO_VALUE, FREE_SUMMARY, ...PRO_GROUPS.flatMap((g) => [g.title, g.detail])];
    for (const s of all) {
        assert.ok(!/\p{Extended_Pictographic}/u.test(s), s);
        assert.notEqual(s, s.toUpperCase(), s);
    }
    for (const g of PRO_GROUPS) {
        // Only the first word (and proper nouns) are capitalized
        const rest = g.title.split(" ").slice(1).filter((w) => !["Obsidian", "iPhone", "Mac"].includes(w));
        for (const w of rest) assert.equal(w, w.toLowerCase(), `${g.title}: ${w}`);
    }
});
