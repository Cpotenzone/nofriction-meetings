// The Lessons library (docs/LESSONS.md §5): the local catalog, the Import
// control, and Make lesson (§13). Non-empty on first run (the six
// stoichiometry seed lessons). Empty / loading / error states per DESIGN.md.

import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { CatalogEntry } from "../../lib/lessons";
import { listCatalog, importBundle } from "../../lib/lessons";
import { currentMakeLessonJob, markMakeLessonOpened, onMakeLessonJob, type MakeLessonJob } from "../../lib/lessonMake";
import { AtomIcon, ImportIcon, SparkleIcon } from "../icons";

interface LessonLibraryProps {
    onOpenSet: (setId: string) => void;
    /** Make lesson: pick a Class recording and make a lesson from it */
    onMake: () => void;
}

function provenanceTag(p: string): string {
    return p === "seed" ? "Built in" : p === "imported" ? "Imported" : "Made from a recording";
}

export function LessonLibrary({ onOpenSet, onMake }: LessonLibraryProps) {
    const [catalog, setCatalog] = useState<CatalogEntry[] | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [job, setJob] = useState<MakeLessonJob | null>(currentMakeLessonJob());

    const refresh = () => {
        setError(null);
        listCatalog()
            .then(setCatalog)
            .catch((e) => setError(String(e)));
    };
    useEffect(refresh, []);
    // A lesson being made (or just made) while the library is open
    useEffect(
        () =>
            onMakeLessonJob((j) => {
                setJob(j);
                if (j?.status === "done") refresh();
            }),
        [],
    );

    const importFile = async () => {
        try {
            const selected = await open({
                multiple: false,
                title: "Import a lesson",
                filters: [{ name: "Lesson bundle", extensions: ["lesson", "zip"] }],
            });
            if (selected && typeof selected === "string") {
                setBusy(true);
                await importBundle({ source: "File", value: selected });
                refresh();
            }
        } catch (e) {
            setError(String(e));
        } finally {
            setBusy(false);
        }
    };

    const ready = job?.status === "done" && job.unopened && job.result ? job.result : null;

    return (
        <div className="lessons">
            <div className="lessons__header">
                <h1 className="lessons__title">
                    <AtomIcon size={18} /> Lessons
                </h1>
                <div className="lessons__actions">
                    <button className="lessons-btn" type="button" onClick={importFile} disabled={busy}>
                        <ImportIcon size={15} /> Import a lesson…
                    </button>
                    <button className="lessons-btn is-primary" type="button" onClick={onMake}>
                        <SparkleIcon size={15} /> Make lesson
                    </button>
                </div>
            </div>
            <div className="lessons__body">
                {job?.status === "running" && (
                    <div className="make-lesson__notice" role="status" aria-live="polite">
                        <span>
                            Making a lesson from “{job.title}”: {job.progress?.label ?? "starting…"}
                        </span>
                        <button className="lessons-btn" type="button" onClick={onMake}>
                            Show progress
                        </button>
                    </div>
                )}
                {ready && (
                    <div className="make-lesson__notice" role="status">
                        <span>“{ready.title}” is ready.</span>
                        <button
                            className="lessons-btn is-primary"
                            type="button"
                            onClick={() => {
                                markMakeLessonOpened();
                                onOpenSet(ready.setId);
                            }}
                        >
                            Open it
                        </button>
                    </div>
                )}
                {error && <div className="lessons-state is-error">Couldn't load your lessons: {error}</div>}
                {!catalog && !error && <div className="lessons-state">Loading your lessons…</div>}
                {catalog && catalog.length === 0 && (
                    <div className="lessons-state">
                        <AtomIcon size={44} className="lessons-state__hero" strokeWidth={1.5} />
                        <p>No lessons yet. Import a .lesson bundle, or make one from a Class recording.</p>
                        <div className="lessons__actions">
                            <button className="lessons-btn" type="button" onClick={importFile}>
                                <ImportIcon size={15} /> Import a lesson…
                            </button>
                            <button className="lessons-btn is-primary" type="button" onClick={onMake}>
                                <SparkleIcon size={15} /> Make lesson
                            </button>
                        </div>
                    </div>
                )}
                {catalog && catalog.length > 0 && (
                    <div className="lessons-grid">
                        {catalog.map((c) => (
                            <button key={c.id} type="button" className="lesson-card" onClick={() => onOpenSet(c.id)}>
                                <AtomIcon size={22} className="lesson-card__badge" strokeWidth={1.5} />
                                <span className="lesson-card__title">{c.title}</span>
                                {c.subtitle && <span className="lesson-card__meta">{c.subtitle}</span>}
                                <span className="lesson-card__tag">{provenanceTag(c.provenance)}</span>
                            </button>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
