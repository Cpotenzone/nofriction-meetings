// The Lessons view (docs/LESSONS.md §2/§17): the fourth top-bar view. Routes
// between the library, a course (dashboard + lessons + exam + certificate), and
// Make lesson (§13: a Class recording → a lesson, with the user's AI). Taking a
// lesson runs fully offline; Make lesson is the one AI call.

import { useCallback, useEffect, useLayoutEffect, useState } from "react";
import type { ReactNode } from "react";
import type { LessonSet } from "../../lib/lessons";
import { getSet, setDir } from "../../lib/lessons";
import { currentMakeLessonJob, markMakeLessonOpened } from "../../lib/lessonMake";
import { failedPartsSummary, type MakeLessonPanelProps } from "../../lib/lessonMakeLogic";
import { onMakeLessonRequest, takeMakeLessonRequest, type MakeLessonRequest } from "../../lib/navigation";
import { LessonLibrary } from "../lessons/LessonLibrary";
import { CourseView } from "../lessons/CourseView";
import "../lessons/Lessons.css";

type Route = "library" | { make: MakeLessonRequest | null } | { setId: string; notice: string | null };

interface LessonsViewProps {
    /**
     * The Make lesson panel (components/MakeLessonPanel.tsx), passed in by the
     * shell so this chunk never imports the AI modules (their legal and help
     * links would trip the offline guard on the Lessons bundle).
     */
    makePanel: (props: MakeLessonPanelProps) => ReactNode;
}

export function LessonsView({ makePanel }: LessonsViewProps) {
    const [route, setRoute] = useState<Route>("library");
    const [set, setSet] = useState<LessonSet | null>(null);
    const [figureDir, setFigureDir] = useState<string | undefined>(undefined);
    const [error, setError] = useState<string | null>(null);

    // Arriving from Recordings → Make lesson, or back while a lesson is being
    // made (or was made while you were elsewhere)
    useLayoutEffect(() => {
        const req = takeMakeLessonRequest();
        if (req) {
            setRoute({ make: req });
        } else {
            const job = currentMakeLessonJob();
            if (job?.status === "running") setRoute({ make: null });
            else if (job?.status === "done" && job.unopened && job.result) {
                markMakeLessonOpened();
                setRoute({ setId: job.result.setId, notice: failedPartsSummary(job.result.failed) });
            }
        }
        return onMakeLessonRequest(() => {
            const r = takeMakeLessonRequest();
            if (r) setRoute({ make: r });
        });
    }, []);

    const openSet = useCallback((setId: string, notice: string | null) => setRoute({ setId, notice }), []);
    const setId = typeof route === "object" && "setId" in route ? route.setId : null;

    useEffect(() => {
        if (!setId) {
            setSet(null);
            return;
        }
        let alive = true;
        setError(null);
        setSet(null);
        Promise.all([getSet(setId), setDir(setId).catch(() => undefined)])
            .then(([s, dir]) => {
                if (!alive) return;
                setSet(s);
                setFigureDir(dir);
            })
            .catch((e) => alive && setError(String(e)));
        return () => {
            alive = false;
        };
    }, [setId]);

    if (typeof route === "object" && "make" in route) {
        return (
            <div className="lessons">
                <div className="lessons__body">
                    {makePanel({ request: route.make, onOpenSet: openSet, onBack: () => setRoute("library") })}
                </div>
            </div>
        );
    }

    if (typeof route === "object") {
        if (error) {
            return (
                <div className="lessons">
                    <div className="lessons-state is-error">
                        Couldn't open this course: {error}
                        <button className="lessons-btn" type="button" onClick={() => setRoute("library")}>
                            Back
                        </button>
                    </div>
                </div>
            );
        }
        if (!set) {
            return (
                <div className="lessons">
                    <div className="lessons-state">Loading the course…</div>
                </div>
            );
        }
        return (
            <div className="lessons">
                <div className="lessons__body">
                    {route.notice && (
                        <p className="make-lesson__notice" role="status">
                            {route.notice}
                        </p>
                    )}
                    <CourseView set={set} figureDir={figureDir} onBack={() => setRoute("library")} />
                </div>
            </div>
        );
    }

    return (
        <LessonLibrary
            onOpenSet={(id) => setRoute({ setId: id, notice: null })}
            onMake={() => setRoute({ make: null })}
        />
    );
}
