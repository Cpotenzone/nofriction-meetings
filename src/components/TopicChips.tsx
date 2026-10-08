// Topics in the library (docs/TOPICS_AND_CHAT.md): the "Topics: All · Q4
// roadmap (3) · …" filter chips beside Notebooks, the "Group by" control,
// a recording row's topic chips, and the editor on the Notes tab (rename,
// remove, add; Find topics with the user's AI).

import { useCallback, useEffect, useId, useState } from "react";
import { notifyTopicsChanged, TOPICS_CHANGED_EVENT, topicsApi } from "../lib/topics";
import {
    cleanLabels,
    EMPTY_INDEX,
    GROUP_BY_KEY,
    GROUP_BY_OPTIONS,
    parseGroupBy,
    topicsForRow,
    type GroupBy,
    type MeetingTopic,
    type TopicIndex,
    type TopicSummary,
} from "../lib/topicsLogic";
import { aiErrorClass, friendlyAiError, isNoProviderError } from "../lib/ai";
import { AiSetupNotice, useAiStatus } from "./AiSetupNotice";
import "./TopicChips.css";

/** The topic index, reloaded when `refreshKey` changes or any recording's topics change. */
export function useTopicIndex(refreshKey: unknown): TopicIndex {
    const [index, setIndex] = useState<TopicIndex>(EMPTY_INDEX);
    const load = useCallback(() => {
        topicsApi.index().then(setIndex).catch(() => {});
    }, []);
    useEffect(load, [load, refreshKey]);
    useEffect(() => {
        window.addEventListener(TOPICS_CHANGED_EVENT, load);
        return () => window.removeEventListener(TOPICS_CHANGED_EVENT, load);
    }, [load]);
    return index;
}

/** The remembered Group by (per user); storage can be unavailable. */
export function useGroupBy(): [GroupBy, (g: GroupBy) => void] {
    const [groupBy, set] = useState<GroupBy>(() => {
        try {
            return parseGroupBy(localStorage.getItem(GROUP_BY_KEY));
        } catch {
            return "date";
        }
    });
    const update = (g: GroupBy) => {
        set(g);
        try {
            localStorage.setItem(GROUP_BY_KEY, g);
        } catch {
            /* this session only */
        }
    };
    return [groupBy, update];
}

/** Filter chips titled "Topics"; hidden until at least one recording has a topic. */
export function TopicFilterChips({
    topics,
    value,
    onChange,
}: {
    topics: TopicSummary[];
    value: string | null;
    onChange: (key: string | null) => void;
}) {
    const titleId = useId();
    if (topics.length === 0 && !value) return null;
    const shown = value && !topics.some((t) => t.key === value) ? [{ key: value, label: value, count: 0 }, ...topics] : topics;
    return (
        <div className="class-filter" role="toolbar" aria-labelledby={titleId}>
            <span id={titleId} className="class-filter__title">Topics</span>
            <button type="button" className={`class-chip ${value === null ? "is-on" : ""}`} aria-pressed={value === null} onClick={() => onChange(null)}>
                All
            </button>
            {shown.map((t) => (
                <button
                    key={t.key}
                    type="button"
                    className={`class-chip ${value === t.key ? "is-on" : ""}`}
                    aria-pressed={value === t.key}
                    title={t.label}
                    onClick={() => onChange(value === t.key ? null : t.key)}
                >
                    {t.label}{t.count > 0 && <span className="topic-chip__count">{t.count}</span>}
                </button>
            ))}
        </div>
    );
}

/** "Group by: Date · Notebook · Topic". */
export function GroupByControl({ value, onChange }: { value: GroupBy; onChange: (g: GroupBy) => void }) {
    const id = useId();
    return (
        <div className="group-by" role="radiogroup" aria-labelledby={id}>
            <span id={id} className="class-filter__title">Group by</span>
            {GROUP_BY_OPTIONS.map((o) => (
                <button
                    key={o.value}
                    type="button"
                    role="radio"
                    aria-checked={value === o.value}
                    className={`class-chip ${value === o.value ? "is-on" : ""}`}
                    onClick={() => onChange(o.value)}
                >
                    {o.label}
                </button>
            ))}
        </div>
    );
}

/** A row's topic chips ("Q4 roadmap · Hiring +1"). */
export function RowTopics({ index, meetingId, onPick }: { index: TopicIndex; meetingId: string; onPick?: (key: string) => void }) {
    const { shown, more } = topicsForRow(index, meetingId);
    if (shown.length === 0) return null;
    return (
        <span className="row-topics">
            {shown.map((t) => (
                <span
                    key={t.key}
                    className="topic-tag"
                    title={t.label}
                    onClick={onPick ? (e) => { e.stopPropagation(); onPick(t.key); } : undefined}
                    role={onPick ? "button" : undefined}
                >
                    {t.label}
                </span>
            ))}
            {more > 0 && <span className="topic-tag topic-tag--more">+{more}</span>}
        </span>
    );
}

function aiFailure(e: unknown): string {
    switch (aiErrorClass(e)) {
        case "pro_required":
            return "Finding topics is part of noFriction Pro.";
        case "consent_required":
            return "Finding topics needs your permission to send this recording to your AI endpoint. Try again and choose Allow.";
        default:
            return friendlyAiError(e);
    }
}

/**
 * The recording's topics on the Notes tab, with "edit" (rename, remove,
 * add; saved as the user's own) and "Find topics" (the AI fills the rest;
 * user topics and removed ones are respected).
 */
export function TopicsEditor({ meetingId }: { meetingId: string }) {
    const [topics, setTopics] = useState<MeetingTopic[]>([]);
    const [editing, setEditing] = useState(false);
    const [drafts, setDrafts] = useState<string[]>([]);
    const [added, setAdded] = useState("");
    const [finding, setFinding] = useState(false);
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [needsAi, setNeedsAi] = useState(false);
    const { configured } = useAiStatus();
    const showSetup = configured === false || needsAi;

    useEffect(() => {
        if (configured) setNeedsAi(false);
    }, [configured]);

    useEffect(() => {
        let live = true;
        setEditing(false);
        setError(null);
        setNeedsAi(false);
        topicsApi.get(meetingId).then((t) => live && setTopics(t)).catch(() => {});
        return () => { live = false; };
    }, [meetingId]);

    const find = async () => {
        setFinding(true);
        setError(null);
        try {
            const t = await topicsApi.find(meetingId);
            setTopics(t);
            notifyTopicsChanged(meetingId);
        } catch (e) {
            if (isNoProviderError(e)) setNeedsAi(true);
            else setError(aiFailure(e));
        } finally {
            setFinding(false);
        }
    };

    const startEdit = () => {
        setDrafts(topics.map((t) => t.label));
        setAdded("");
        setEditing(true);
        setError(null);
    };

    const save = async () => {
        setSaving(true);
        setError(null);
        try {
            const labels = cleanLabels([...drafts, added]);
            const t = await topicsApi.set(meetingId, labels);
            setTopics(t);
            setEditing(false);
            notifyTopicsChanged(meetingId);
        } catch (e) {
            setError(String(e));
        } finally {
            setSaving(false);
        }
    };

    return (
        <section className="topics-editor" aria-label="Topics">
            <div className="topics-editor__head">
                <h4>Topics</h4>
                {!editing && (
                    <div className="topics-editor__actions">
                        {topics.length > 0 && (
                            <button className="mn-btn small" onClick={startEdit}>Edit</button>
                        )}
                        <button className="mn-btn small" onClick={find} disabled={finding || showSetup}>
                            {finding ? "Finding…" : topics.length > 0 ? "Find again" : "Find topics"}
                        </button>
                    </div>
                )}
            </div>
            {showSetup && <AiSetupNotice feature="Topics" compact />}
            {error && <p className="mn-error" role="alert">{error}</p>}
            {editing ? (
                <div className="topics-editor__form">
                    {drafts.map((d, i) => (
                        <div key={i} className="topics-editor__row">
                            <input
                                value={d}
                                maxLength={40}
                                aria-label={`Topic ${i + 1}`}
                                onChange={(e) => setDrafts((prev) => prev.map((x, j) => (j === i ? e.target.value : x)))}
                            />
                            <button className="mn-btn small" onClick={() => setDrafts((prev) => prev.filter((_, j) => j !== i))} aria-label="Remove topic">
                                Remove
                            </button>
                        </div>
                    ))}
                    <div className="topics-editor__row">
                        <input
                            value={added}
                            maxLength={40}
                            placeholder="Add a topic"
                            aria-label="New topic"
                            onChange={(e) => setAdded(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === "Enter" && added.trim()) {
                                    setDrafts((prev) => [...prev, added.trim()]);
                                    setAdded("");
                                }
                            }}
                        />
                    </div>
                    <div className="topics-editor__actions">
                        <button className="mn-btn primary small" onClick={save} disabled={saving}>{saving ? "Saving…" : "Save"}</button>
                        <button className="mn-btn small" onClick={() => setEditing(false)} disabled={saving}>Cancel</button>
                    </div>
                    <p className="mn-muted topics-editor__hint">
                        Your topics are kept when topics are found again; a topic you remove doesn't come back.
                    </p>
                </div>
            ) : topics.length === 0 ? (
                <p className="mn-muted">No topics yet. They're found when notes are written, or now with Find topics.</p>
            ) : (
                <div className="topics-editor__chips">
                    {topics.map((t) => (
                        <span key={t.id} className={`topic-tag ${t.source === "user" ? "topic-tag--user" : ""}`} title={t.source === "user" ? "Your topic" : "Found by your AI"}>
                            {t.label}
                        </span>
                    ))}
                    {topics.length > 0 && <button className="topics-editor__edit" onClick={startEdit}>edit</button>}
                </div>
            )}
        </section>
    );
}
