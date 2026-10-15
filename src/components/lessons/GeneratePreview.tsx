// The Generate affordance's "coming soon" preview (docs/LESSONS.md §12.3,
// owner-decision D2). This is an intentional, honest preview — not a disabled
// control and not an error. It makes NO AI call, no entitlement check, and no
// network request in this build.

import { SparkleIcon } from "../icons";

export function GeneratePreview({ onClose }: { onClose: () => void }) {
    return (
        <div className="generate-preview">
            <SparkleIcon size={44} className="generate-preview__hero" strokeWidth={1.5} />
            <h2 className="lessons__title" style={{ justifyContent: "center" }}>
                AI lesson generation — coming soon
            </h2>
            <p>
                Turn a Class recording into an interactive lesson — explainers, worked examples, a checkpoint,
                and a trainer, generated from your recording's transcript and screenshots.
            </p>
            <p>
                This is a preview of a planned capability. When it ships it will require noFriction Pro and an
                AI provider. Today you can take the built-in Stoichiometry course or use{" "}
                <strong>Import a lesson…</strong> to add your own.
            </p>
            <button className="lessons-btn" type="button" onClick={onClose}>
                Back to lessons
            </button>
        </div>
    );
}
