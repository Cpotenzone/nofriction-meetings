// The Lessons view (docs/LESSONS.md §2/§17): the fourth top-bar view. Routes
// between the library, a course (dashboard + lessons + exam + certificate), and
// the Generate coming-soon preview. Everything here runs fully offline.

import { useEffect, useState } from "react";
import type { LessonSet } from "../../lib/lessons";
import { getSet, setDir } from "../../lib/lessons";
import { LessonLibrary } from "../lessons/LessonLibrary";
import { CourseView } from "../lessons/CourseView";
import { GeneratePreview } from "../lessons/GeneratePreview";
import "../lessons/Lessons.css";

type Route = "library" | "generate" | { setId: string };

export function LessonsView() {
    const [route, setRoute] = useState<Route>("library");
    const [set, setSet] = useState<LessonSet | null>(null);
    const [figureDir, setFigureDir] = useState<string | undefined>(undefined);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (typeof route !== "object") {
            setSet(null);
            return;
        }
        let alive = true;
        setError(null);
        Promise.all([getSet(route.setId), setDir(route.setId).catch(() => undefined)])
            .then(([s, dir]) => {
                if (!alive) return;
                setSet(s);
                setFigureDir(dir);
            })
            .catch((e) => alive && setError(String(e)));
        return () => {
            alive = false;
        };
    }, [route]);

    if (route === "generate") {
        return (
            <div className="lessons">
                <div className="lessons__body">
                    <GeneratePreview onClose={() => setRoute("library")} />
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
                    <CourseView set={set} figureDir={figureDir} onBack={() => setRoute("library")} />
                </div>
            </div>
        );
    }

    return <LessonLibrary onOpenSet={(setId) => setRoute({ setId })} onGenerate={() => setRoute("generate")} />;
}
