// "What is it?" — a recording's type (Meeting · Class · Personal) and the
// labels that follow it. Pure (no Tauri, no React), tested with `npm test`.
// Same vocabulary as the backend (src-tauri/src/recording_kind.rs) and iOS.
// See docs/TIMED_RECORDING_AND_NOTEBOOKS.md.

/** Stored and wire values. NULL / unknown means "meeting". */
export type RecordingKind = "meeting" | "class" | "personal";

export interface KindInfo {
    value: RecordingKind;
    /** Picker label */
    label: string;
    /** Key that picks it in the Record sheet (not while typing a notebook) */
    key: string;
    /** Notebook field placeholder */
    notebookPlaceholder: string;
}

export const RECORDING_KINDS: readonly KindInfo[] = [
    { value: "meeting", label: "Meeting", key: "m", notebookPlaceholder: "e.g. Acme project" },
    { value: "class", label: "Class", key: "c", notebookPlaceholder: "e.g. BIO 101" },
    { value: "personal", label: "Personal", key: "p", notebookPlaceholder: "e.g. Health" },
];

export const DEFAULT_KIND: RecordingKind = "meeting";

/** Help text under the picker. */
export const KIND_HELP = "Personal covers everything else: conversations, appointments, talks, ideas.";

/** The field label and the filter title. */
export const NOTEBOOK_LABEL = "Notebook";
export const NOTEBOOKS_LABEL = "Notebooks";

/** A stored or remembered value; anything unknown is a meeting. */
export function parseKind(value: string | null | undefined): RecordingKind {
    const v = (value ?? "").trim().toLowerCase();
    return RECORDING_KINDS.find((k) => k.value === v)?.value ?? DEFAULT_KIND;
}

function info(kind: RecordingKind): KindInfo {
    return RECORDING_KINDS.find((k) => k.value === kind) ?? RECORDING_KINDS[0];
}

/** "Meeting" / "Class" / "Personal" */
export function kindLabel(kind: RecordingKind): string {
    return info(kind).label;
}

export function notebookPlaceholder(kind: RecordingKind): string {
    return info(kind).notebookPlaceholder;
}

/** M / C / P → the type (any case). */
export function kindForKey(key: string): RecordingKind | null {
    const k = key.toLowerCase();
    return RECORDING_KINDS.find((i) => i.key === k)?.value ?? null;
}

/** "Study guide" for a class, "Review guide" otherwise. */
export function guideTitle(kind: RecordingKind): string {
    return kind === "class" ? "Study guide" : "Review guide";
}

/** The third mark's label (stored as `test` for every type). */
export function thirdMarkLabel(kind: RecordingKind): string {
    switch (kind) {
        case "class":
            return "On the test";
        case "personal":
            return "Remember";
        default:
            return "Follow up";
    }
}

/** The third mark's tooltip hint. */
export function thirdMarkHint(kind: RecordingKind): string {
    switch (kind) {
        case "class":
            return "Said to be on the exam";
        case "personal":
            return "Something to remember";
        default:
            return "Something to follow up on";
    }
}

/** How notes are written for a type (Record sheet hint, Notes view). */
export function notesStyleHint(kind: RecordingKind): string {
    switch (kind) {
        case "class":
            return "Notes are written as lecture notes: key concepts, definitions, announcements and deadlines.";
        case "personal":
            return "Notes are a summary, key points, and to-dos and reminders.";
        default:
            return "Notes are meeting notes: summary, decisions and action items.";
    }
}

/** `meeting_notes.model_used` → which layout the Notes view uses. */
export type NotesLayout = "meeting" | "lecture" | "personal";

export function notesLayout(modelUsed: string | null | undefined): NotesLayout {
    if (modelUsed === "lecture-notes") return "lecture";
    if (modelUsed === "personal-notes") return "personal";
    return "meeting";
}
