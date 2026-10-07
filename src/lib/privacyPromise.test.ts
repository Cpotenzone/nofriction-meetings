// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import { promiseCopy } from "./privacyPromise.ts";

test("with a transcription model: both recording and transcription are offline", () => {
    const c = promiseCopy(true);
    assert.equal(c.headline, "Recording and transcription work offline.");
    assert.equal(c.detail, "Nothing leaves this Mac unless you want it to.");
    assert.equal(c.pill, "Works offline · Nothing leaves this Mac unless you want it to");
    assert.equal(c.needsModel, false);
});

test("without a model: never claims offline transcription, offers the download", () => {
    const c = promiseCopy(false);
    assert.match(c.headline, /^Recording works offline\./);
    assert.match(c.headline, /Download a transcription model once/);
    assert.doesNotMatch(c.pill, /offline/i);
    assert.equal(c.needsModel, true);
});

test("while unknown: claims only what is true either way", () => {
    const c = promiseCopy(null);
    assert.equal(c.headline, "Recording works offline.");
    assert.doesNotMatch(c.pill, /offline/i);
    assert.equal(c.needsModel, false);
});

test("the explanation names every way content can leave, and the device", () => {
    const c = promiseCopy(true, "this iPad");
    assert.match(c.detail, /this iPad/);
    assert.match(c.explain, /AI through a server you set up/);
    assert.match(c.explain, /exporting and sharing/);
});
