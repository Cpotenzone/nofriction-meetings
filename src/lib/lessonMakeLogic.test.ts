// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    classRecordings,
    durationLabel,
    failedPartsSummary,
    isClassRecording,
    isStaleMakeLessonJob,
    madeLessonFor,
} from "./lessonMakeLogic.ts";

const rec = (id: string, kind: string | undefined, started: string) => ({
    id,
    title: id,
    started_at: started,
    duration_seconds: 600,
    recording_kind: kind,
});

test("only Class recordings can make a lesson, newest first", () => {
    const list = [
        rec("a", "class", "2026-10-01T09:00:00Z"),
        rec("b", "meeting", "2026-10-03T09:00:00Z"),
        rec("c", undefined, "2026-10-04T09:00:00Z"),
        rec("d", "Class", "2026-10-02T09:00:00Z"),
        rec("e", "personal", "2026-10-05T09:00:00Z"),
    ];
    assert.deepEqual(
        classRecordings(list).map((m) => m.id),
        ["d", "a"],
    );
    assert.equal(isClassRecording({ recording_kind: null }), false);
});

test("the lesson already made from a recording is found by its source", () => {
    const entry = (id: string, provenance: string, src: string | null) => ({
        id,
        title: id,
        provenance,
        sourceRecordingId: src,
        contentFingerprint: "fp",
        schemaMajor: 1,
        importedAt: "",
    });
    const catalog = [entry("seed", "seed", null), entry("imp", "imported", "m1"), entry("gen", "generated", "m1")];
    assert.equal(madeLessonFor(catalog, "m1")?.id, "gen");
    assert.equal(madeLessonFor(catalog, "m2"), null);
    assert.equal(madeLessonFor(null, "m1"), null);
    assert.equal(madeLessonFor(catalog, null), null);
});

test("the result names what the lesson was made without", () => {
    assert.equal(failedPartsSummary([]), null);
    const s = failedPartsSummary([
        { part: "mistakes", lesson: 1, error: "x" },
        { part: "checkpoint", lesson: 2, error: "y" },
        { part: "exam", lesson: null, error: "z" },
    ])!;
    assert.match(s, /^Left out lesson 2 \(no checkpoint\); made without the common mistakes for lesson 1, the final exam questions/);
    assert.match(s, /the exam uses the checkpoint questions/);
    assert.match(failedPartsSummary([{ part: "worked_examples", lesson: 3, error: "e" }])!, /^Made without the worked examples for lesson 3\./);
});

test("durations read plainly", () => {
    assert.equal(durationLabel(null), "");
    assert.equal(durationLabel(0), "");
    assert.equal(durationLabel(20), "1 min");
    assert.equal(durationLabel(48 * 60), "48 min");
    assert.equal(durationLabel(65 * 60), "1 h 05 min");
});

test("an old failed or finished run is forgotten when it no longer applies", () => {
    const noProvider = (e: unknown) => String(e).includes("AI_NO_PROVIDER");
    const failedNoAi = { meetingId: "m1", status: "failed" as const, error: "AI_NO_PROVIDER: set up AI" };
    const failedOther = { meetingId: "m1", status: "failed" as const, error: "AI_UNREACHABLE: down" };
    const done = { meetingId: "m1", status: "done" as const };
    const running = { meetingId: "m1", status: "running" as const };
    // AI set up after a "no provider" failure: the setup notice and error go
    assert.equal(isStaleMakeLessonJob(failedNoAi, "m1", true, noProvider), true);
    assert.equal(isStaleMakeLessonJob(failedNoAi, "m1", false, noProvider), false, "still not set up");
    assert.equal(isStaleMakeLessonJob(failedNoAi, "m1", null, noProvider), false, "status still loading");
    // Another failure for the same recording stays visible…
    assert.equal(isStaleMakeLessonJob(failedOther, "m1", true, noProvider), false);
    // …until another recording is chosen
    assert.equal(isStaleMakeLessonJob(failedOther, "m2", true, noProvider), true);
    assert.equal(isStaleMakeLessonJob(done, "m2", true, noProvider), true);
    // A running job is never forgotten; nothing to forget is not stale
    assert.equal(isStaleMakeLessonJob(running, "m2", true, noProvider), false);
    assert.equal(isStaleMakeLessonJob(null, "m1", true, noProvider), false);
});
