// Topics on a recording's Notes view (docs/TOPICS_AND_CHAT.md): the chips,
// and one Edit that opens rename / remove / add and "Find topics" with the
// user's AI. Topics are also a search facet in Recordings.

import { useCallback, useEffect, useState } from "react";
import { notifyTopicsChanged, TOPICS_CHANGED_EVENT, topicsApi } from "../lib/topics";
import { cleanLabels, EMPTY_INDEX, type MeetingTopic, type TopicIndex } from "../lib/topicsLogic";
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
 * The recording's topics, under the notes. One Edit: rename, remove, add
 * (saved as the user's own) and Find topics (the AI fills the rest; user
 * topics and removed ones are respected).
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
            setDrafts(t.map((x) => x.label));
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
                    <button className="mn-btn small" onClick={startEdit}>Edit</button>
                )}
            </div>
            {editing && showSetup && <AiSetupNotice feature="Finding topics" compact />}
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
                        <button className="mn-btn small" onClick={find} disabled={finding || showSetup} title="Your AI names the topics; yours are kept">
                            {finding ? "Finding…" : topics.length > 0 ? "Find again" : "Find topics"}
                        </button>
                    </div>
                    <p className="mn-muted topics-editor__hint">
                        Your topics are kept when topics are found again; a topic you remove doesn't come back.
                    </p>
                </div>
            ) : topics.length === 0 ? (
                <p className="mn-muted">No topics yet. They're named when notes are made, or with Edit → Find topics.</p>
            ) : (
                <div className="topics-editor__chips">
                    {topics.map((t) => (
                        <span key={t.id} className={`topic-tag ${t.source === "user" ? "topic-tag--user" : ""}`} title={t.source === "user" ? "Your topic" : "Named by your AI"}>
                            {t.label}
                        </span>
                    ))}
                </div>
            )}
        </section>
    );
}
