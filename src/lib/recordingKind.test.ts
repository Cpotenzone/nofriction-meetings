// Run with `npm test` (node's test runner; Node strips the types).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
    KIND_HELP,
    NOTEBOOKS_LABEL,
    NOTEBOOK_LABEL,
    RECORDING_KINDS,
    guideTitle,
    kindForKey,
    kindLabel,
    notebookPlaceholder,
    notesLayout,
    notesStyleHint,
    parseKind,
    thirdMarkLabel,
} from "./recordingKind.ts";
import { MARKER_KINDS, markerLabel, markerMeta } from "./studyLogic.ts";

test("three types in picker order, Meeting first and the default", () => {
    assert.deepEqual(RECORDING_KINDS.map((k) => k.label), ["Meeting", "Class", "Personal"]);
    assert.deepEqual(RECORDING_KINDS.map((k) => k.value), ["meeting", "class", "personal"]);
    assert.deepEqual(RECORDING_KINDS.map((k) => k.key), ["m", "c", "p"]);
    assert.equal(parseKind(null), "meeting", "never picked: Meeting");
    assert.equal(parseKind(undefined), "meeting");
    assert.equal(parseKind(" CLASS "), "class");
    assert.equal(parseKind("personal"), "personal");
    assert.equal(parseKind("lecture"), "meeting", "unknown values are meetings (like a NULL column)");
    assert.equal(kindLabel("personal"), "Personal");
    assert.equal(KIND_HELP, "Personal covers everything else: conversations, appointments, talks, ideas.");
    assert.equal(NOTEBOOK_LABEL, "Notebook");
    assert.equal(NOTEBOOKS_LABEL, "Notebooks");
});

test("M / C / P pick a type", () => {
    assert.equal(kindForKey("m"), "meeting");
    assert.equal(kindForKey("C"), "class");
    assert.equal(kindForKey("p"), "personal");
    assert.equal(kindForKey("x"), null);
    assert.equal(kindForKey("1"), null);
});

test("notebook placeholder by type", () => {
    assert.equal(notebookPlaceholder("meeting"), "e.g. Acme project");
    assert.equal(notebookPlaceholder("class"), "e.g. BIO 101");
    assert.equal(notebookPlaceholder("personal"), "e.g. Health");
});

test("the guide is a Study guide for a class and a Review guide otherwise", () => {
    assert.equal(guideTitle("class"), "Study guide");
    assert.equal(guideTitle("meeting"), "Review guide");
    assert.equal(guideTitle("personal"), "Review guide");
});

test("marker labels per type: only the third mark's label changes", () => {
    assert.equal(thirdMarkLabel("class"), "On the test");
    assert.equal(thirdMarkLabel("meeting"), "Follow up");
    assert.equal(thirdMarkLabel("personal"), "Remember");
    assert.deepEqual(MARKER_KINDS, ["important", "question", "test"], "stored kinds are the same for every type");
    for (const rec of ["meeting", "class", "personal"] as const) {
        assert.equal(markerLabel("important", rec), "Important");
        assert.equal(markerLabel("question", rec), "Question");
        assert.equal(markerLabel("test", rec), thirdMarkLabel(rec));
        assert.deepEqual(
            MARKER_KINDS.map((k) => markerMeta(k, rec).symbol),
            ["★", "?", "✎"],
        );
    }
    assert.equal(markerMeta("test", "class").hint, "Said to be on the exam");
    assert.equal(markerMeta("test", "meeting").hint, "Something to follow up on");
});

test("notes layout follows the prompt that wrote them; style hint follows the type", () => {
    assert.equal(notesLayout("lecture-notes"), "lecture");
    assert.equal(notesLayout("personal-notes"), "personal");
    assert.equal(notesLayout("auto-report"), "meeting");
    assert.equal(notesLayout(null), "meeting");
    assert.match(notesStyleHint("class"), /lecture notes/);
    assert.match(notesStyleHint("personal"), /to-dos and reminders/);
    assert.match(notesStyleHint("meeting"), /meeting notes/);
});
