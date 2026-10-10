// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import { countdown, lastSyncedLabel, plainInline, syncedNotesSections } from "./syncLogic.ts";

test("iPhone notes become sections: bold headings, bullets, paragraphs", () => {
    const md = "**Summary**\nCells divide.\n\n**Action items**\n• Alex: send the **agreement**\n• Read ch. 4";
    assert.deepEqual(syncedNotesSections(md), [
        { heading: "Summary", paragraphs: ["Cells divide."], bullets: [] },
        { heading: "Action items", paragraphs: [], bullets: ["Alex: send the agreement", "Read ch. 4"] },
    ]);
});

test("plain Markdown headings and lists work too; text before a heading has no heading", () => {
    assert.deepEqual(syncedNotesSections("Intro line\n## Decisions\n- Ship it\n1. First"), [
        { heading: null, paragraphs: ["Intro line"], bullets: [] },
        { heading: "Decisions", paragraphs: [], bullets: ["Ship it", "First"] },
    ]);
    assert.deepEqual(syncedNotesSections(""), []);
    assert.equal(plainInline("a **b** __c__"), "a b c");
});

test("last synced line", () => {
    const now = new Date("2026-10-10T12:00:00Z");
    assert.equal(lastSyncedLabel(null, now), "Not synced yet");
    assert.equal(lastSyncedLabel("garbage", now), "Not synced yet");
    assert.equal(lastSyncedLabel("2026-10-10T11:59:40Z", now), "Last synced just now");
    assert.equal(lastSyncedLabel("2026-10-10T11:59:00Z", now), "Last synced 1 minute ago");
    assert.equal(lastSyncedLabel("2026-10-10T11:15:00Z", now), "Last synced 45 minutes ago");
    assert.equal(lastSyncedLabel("2026-10-10T09:00:00Z", now), "Last synced 3 hours ago");
    assert.match(lastSyncedLabel("2026-10-01T09:00:00Z", now), /^Last synced Oct 1$/);
});

test("pairing countdown", () => {
    assert.equal(countdown(300), "5:00");
    assert.equal(countdown(61.9), "1:01");
    assert.equal(countdown(-3), "0:00");
});
