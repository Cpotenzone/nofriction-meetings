// Lessons subsystem — Tauri command wrappers and shared types (docs/LESSONS.md
// §17). Grading and persistence are authoritative in Rust; these are thin
// typed calls into the registered `lessons_*` commands. Taking a lesson is 100%
// offline and free: nothing here fetches a remote resource.

import { invoke } from "@tauri-apps/api/core";

// ── Authored content shapes (1:1 with the Rust schema) ──────────────────────

export type ExplainerKind =
    | "figure"
    | "equation_balancer"
    | "mole_ratio"
    | "limiting_reagent"
    | "dimensional_analysis"
    | "molecule_diagram"
    | "titration_curve"
    | "ideal_gas_pv"
    | "misconceptions";

export interface Explainer {
    id: string;
    title: string;
    conceptIds: string[];
    caption: string;
    a11yState?: string | null;
    kind: ExplainerKind;
    // Validated structured payload; shape depends on `kind`. Rendered by typed
    // components, never as HTML.
    data: Record<string, unknown>;
}

export interface Step {
    text: string;
    expression?: string | null;
    result?: string | null;
    unit?: string | null;
}

export interface WorkedExample {
    id: string;
    prompt: string;
    conceptIds: string[];
    steps: Step[];
    finalAnswer: { value: number; unit?: string | null };
}

export interface MisconceptionItem {
    claim: string;
    whyWrong: string;
    correct: string;
    conceptIds: string[];
}

export type ItemKind = "mc" | "numeric" | "balance" | "multistep";

export interface QuizItem {
    id: string;
    conceptIds: string[];
    kind: ItemKind;
    prompt: string;
    solution: Step[];
    explanation: string;
    choices?: string[] | null;
    answerIndex?: number | null;
    answer?: number | null;
    unit?: string | null;
    tolerance?: { kind: "exact" | "abs" | "rel"; value: number } | null;
    equation?: unknown;
    answerCoefficients?: number[] | null;
    steps?: MultistepStep[] | null;
    gated?: boolean | null;
}

export interface MultistepStep {
    prompt: string;
    kind: "mc" | "numeric" | "balance";
    choices?: string[] | null;
    answerIndex?: number | null;
    answer?: number | null;
    unit?: string | null;
    equation?: unknown;
    answerCoefficients?: number[] | null;
}

export interface Lesson {
    id: string;
    index: number;
    title: string;
    objectives: { text: string; conceptIds: string[] }[];
    conceptIds: string[];
    explainers: Explainer[];
    workedExamples: WorkedExample[];
    misconceptions: { items: MisconceptionItem[] };
    trainer: unknown;
    checkpoint: { presentCount: number; items: QuizItem[] };
}

export interface Concept {
    id: string;
    name: string;
    summary: string;
    prerequisites: string[];
    lessonIds: string[];
}

export interface LessonSet {
    id: string;
    schemaMajor: number;
    title: string;
    subtitle?: string | null;
    level: string;
    provenance: "seed" | "imported" | "generated";
    lessons: string[];
    concepts: Concept[];
    diagnostic?: { items: QuizItem[] } | null;
    exam: { items: QuizItem[]; questionCount: number; passPct: number };
}

export interface CatalogEntry {
    id: string;
    title: string;
    subtitle?: string | null;
    level?: string | null;
    provenance: string;
    sourceRecordingId?: string | null;
    contentFingerprint: string;
    schemaMajor: number;
    importedAt: string;
}

// ── Submitted answers + results ─────────────────────────────────────────────

export interface SubmittedAnswer {
    itemId: string;
    choiceIndex?: number;
    numeric?: string;
    coefficients?: number[];
    multistep?: { choiceIndex?: number; numeric?: string; coefficients?: number[] }[];
    firstTry: boolean;
}

export interface GradedItem {
    itemId: string;
    earned: number;
    possible: number;
    correct: boolean;
}

export interface CheckpointResult {
    scorePct: number;
    passed: boolean;
    graded: GradedItem[];
}

export interface ExamResult {
    scorePct: number;
    passed: boolean;
    graded: GradedItem[];
}

export interface StartedExam {
    attemptId: string;
    items: QuizItem[];
}

export interface Mastery {
    score: number;
    attempts: number;
    correctAttempts: number;
    firstTryCorrect: number;
    mastered: boolean;
    ease: number;
    intervalDays: number;
    reviews: number;
}

export interface Dashboard {
    setId: string;
    concepts: { conceptId: string; name: string; score: number; mastered: boolean }[];
    checkpointsPassed: number;
    totalLessons: number;
    examUnlocked: boolean;
    readinessPct: number;
    readinessLabel: string;
}

export interface DueConcept {
    conceptId: string;
    score: number;
    dueAt?: string | null;
}

export interface Certificate {
    id: string;
    courseTitle: string;
    level?: string | null;
    scorePct: number;
    issuedAt: string;
    provenance: string;
    resultHash: string;
}

export interface LessonProgress {
    explainersSeen: string[];
    workedExamplesSeen: string[];
    checkpointBestPct: number;
    checkpointPassed: boolean;
    lastViewedExplainerId?: string | null;
}

export type ImportSource =
    | { source: "File"; value: string }
    | { source: "Folder"; value: string }
    | { source: "Url"; value: string };

// ── Command wrappers ────────────────────────────────────────────────────────

export const listCatalog = (): Promise<CatalogEntry[]> => invoke("lessons_list_catalog");
export const getSet = (setId: string): Promise<LessonSet> => invoke("lessons_get_set", { setId });
export const setDir = (setId: string): Promise<string> => invoke("lessons_set_dir", { setId });
export const getLesson = (setId: string, lessonId: string): Promise<Lesson> =>
    invoke("lessons_get_lesson", { setId, lessonId });
export const importBundle = (source: ImportSource): Promise<string> =>
    invoke("lessons_import_bundle", { source });
export const removeSet = (setId: string): Promise<void> => invoke("lessons_remove_set", { setId });
export const saveProgress = (setId: string, lessonId: string, progress: LessonProgress): Promise<void> =>
    invoke("lessons_save_progress", { setId, lessonId, progress });
export const submitCheckpoint = (
    setId: string,
    lessonId: string,
    answers: SubmittedAnswer[],
): Promise<CheckpointResult> => invoke("lessons_submit_checkpoint", { setId, lessonId, answers });
export const reviewDue = (setId: string): Promise<DueConcept[]> => invoke("lessons_review_due", { setId });
export const dashboard = (setId: string): Promise<Dashboard> => invoke("lessons_dashboard", { setId });
export const startExam = (setId: string): Promise<StartedExam> => invoke("lessons_start_exam", { setId });
export const submitExam = (
    setId: string,
    attemptId: string,
    answers: SubmittedAnswer[],
): Promise<ExamResult> => invoke("lessons_submit_exam", { setId, attemptId, answers });
export const exportCertificate = (setId: string): Promise<Certificate | null> =>
    invoke("lessons_export_certificate", { setId });
export const resetProgress = (setId: string): Promise<void> => invoke("lessons_reset_progress", { setId });
export const submitTrainerItem = (
    setId: string,
    conceptId: string,
    correct: boolean,
    firstTry: boolean,
): Promise<Mastery> => invoke("lessons_submit_trainer_item", { setId, conceptId, correct, firstTry });
