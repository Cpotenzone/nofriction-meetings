// Small shared pieces for moment markers: the kind glyph, the kind chooser
// (★ Important · ? Question · ✎ On the test / Follow up / Remember), and
// the recording type that labels them. docs/STUDY_TOOLS.md

import { createContext, useContext } from "react";
import { PencilIcon, QuestionIcon, StarIcon } from "../icons";
import { MARKER_KINDS, markerMeta, type MarkerKind, type MarkerMeta } from "../../lib/studyLogic";
import type { RecordingKind } from "../../lib/recordingKind";
import "./Study.css";

/**
 * The type of the recording whose marks are shown. It only changes the
 * third mark's label (stored as `test` for every type). Views provide it
 * (Recordings, Review, the capture bar's mark card).
 */
export const MarkKindContext = createContext<RecordingKind>("meeting");

/** Labels for marks in the current recording's type. */
export function useMarkerMeta(): (kind: MarkerKind) => MarkerMeta {
    const rec = useContext(MarkKindContext);
    return (kind) => markerMeta(kind, rec);
}

export function MarkerGlyph({ kind, size = 13 }: { kind: MarkerKind; size?: number }) {
    const Icon = kind === "question" ? QuestionIcon : kind === "test" ? PencilIcon : StarIcon;
    return (
        <span className={`study-glyph is-${kind}`} aria-hidden>
            <Icon size={size} strokeWidth={2.2} />
        </span>
    );
}

/** Three chips; the current kind is pressed. */
export function KindPicker({
    value,
    onChange,
    disabled,
    compact,
}: {
    value: MarkerKind;
    onChange: (k: MarkerKind) => void;
    disabled?: boolean;
    compact?: boolean;
}) {
    const meta = useMarkerMeta();
    return (
        <div className={`study-kinds${compact ? " is-compact" : ""}`} role="group" aria-label="Marker type">
            {MARKER_KINDS.map((k) => (
                <button
                    key={k}
                    type="button"
                    className={`study-kind is-${k}${value === k ? " is-on" : ""}`}
                    aria-pressed={value === k}
                    aria-label={compact ? meta(k).label : undefined}
                    disabled={disabled}
                    title={`${meta(k).symbol} ${meta(k).label}: ${meta(k).hint}`}
                    onClick={() => value !== k && onChange(k)}
                >
                    <MarkerGlyph kind={k} />
                    {!compact && <span>{meta(k).label}</span>}
                </button>
            ))}
        </div>
    );
}
