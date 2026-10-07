// Small shared pieces for moment markers: the kind glyph and the kind
// chooser (★ Important · ? Question · ✎ On the test). docs/STUDY_TOOLS.md

import { PencilIcon, QuestionIcon, StarIcon } from "../icons";
import { MARKER_KINDS, MARKER_META, type MarkerKind } from "../../lib/studyLogic";
import "./Study.css";

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
    return (
        <div className={`study-kinds${compact ? " is-compact" : ""}`} role="group" aria-label="Marker type">
            {MARKER_KINDS.map((k) => (
                <button
                    key={k}
                    type="button"
                    className={`study-kind is-${k}${value === k ? " is-on" : ""}`}
                    aria-pressed={value === k}
                    disabled={disabled}
                    title={`${MARKER_META[k].symbol} ${MARKER_META[k].label}: ${MARKER_META[k].hint}`}
                    onClick={() => value !== k && onChange(k)}
                >
                    <MarkerGlyph kind={k} />
                    {!compact && <span>{MARKER_META[k].label}</span>}
                </button>
            ))}
        </div>
    );
}
