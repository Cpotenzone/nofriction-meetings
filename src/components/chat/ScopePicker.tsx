// The scope at the top of CHAT: All recordings · this Notebook · this
// Topic · this recording. The lists come from the recordings on this Mac.

import { useEffect, useId, useState } from "react";
import * as tauri from "../../lib/tauri";
import { listRecentNotebooks } from "../../lib/timedRecording";
import { topicsApi } from "../../lib/topics";
import type { TopicSummary } from "../../lib/topicsLogic";
import { sameScope, type Scope } from "../../lib/chatLogic";

interface Props {
    scope: Scope;
    onChange: (s: Scope) => void;
    disabled?: boolean;
    /** The recording open in REWIND, offered first under "this recording" */
    selectedMeetingId: string | null;
}

export function ScopePicker({ scope, onChange, disabled, selectedMeetingId }: Props) {
    const [notebooks, setNotebooks] = useState<string[]>([]);
    const [topics, setTopics] = useState<TopicSummary[]>([]);
    const [meetings, setMeetings] = useState<tauri.Meeting[]>([]);
    const id = useId();

    useEffect(() => {
        let live = true;
        listRecentNotebooks().then((n) => live && setNotebooks(n)).catch(() => {});
        topicsApi.index().then((i) => live && setTopics(i.topics)).catch(() => {});
        tauri.getMeetings(100).then((m) => live && setMeetings(m)).catch(() => {});
        return () => { live = false; };
    }, [scope.kind]);

    const kind = scope.kind;
    const pick = (next: Scope) => {
        if (!sameScope(next, scope)) onChange(next);
    };
    const orderedMeetings = selectedMeetingId
        ? [...meetings.filter((m) => m.id === selectedMeetingId), ...meetings.filter((m) => m.id !== selectedMeetingId)]
        : meetings;

    return (
        <div className="rc-scope" role="group" aria-label="Chat scope">
            <span className="rc-scope__title">Ask about</span>
            <select
                id={`${id}-kind`}
                className="rc-scope__kind"
                value={kind}
                disabled={disabled}
                aria-label="Scope"
                onChange={(e) => {
                    const k = e.target.value as Scope["kind"];
                    if (k === "all") pick({ kind: "all" });
                    else if (k === "notebook") pick({ kind: "notebook", value: notebooks[0] ?? "" });
                    else if (k === "topic") pick({ kind: "topic", value: topics[0]?.key ?? "" });
                    else pick({ kind: "meeting", value: selectedMeetingId ?? orderedMeetings[0]?.id ?? "" });
                }}
            >
                <option value="all">All recordings</option>
                <option value="notebook" disabled={notebooks.length === 0}>This Notebook</option>
                <option value="topic" disabled={topics.length === 0}>This Topic</option>
                <option value="meeting" disabled={meetings.length === 0}>This recording</option>
            </select>
            {kind === "notebook" && (
                <select
                    className="rc-scope__value"
                    value={scope.value ?? ""}
                    disabled={disabled}
                    aria-label="Notebook"
                    onChange={(e) => pick({ kind: "notebook", value: e.target.value })}
                >
                    {scope.value && !notebooks.includes(scope.value) && <option value={scope.value}>{scope.value}</option>}
                    {notebooks.map((n) => <option key={n} value={n}>{n}</option>)}
                </select>
            )}
            {kind === "topic" && (
                <select
                    className="rc-scope__value"
                    value={scope.value ?? ""}
                    disabled={disabled}
                    aria-label="Topic"
                    onChange={(e) => pick({ kind: "topic", value: e.target.value })}
                >
                    {scope.value && !topics.some((t) => t.key === scope.value) && <option value={scope.value}>{scope.value}</option>}
                    {topics.map((t) => <option key={t.key} value={t.key}>{t.label} ({t.count})</option>)}
                </select>
            )}
            {kind === "meeting" && (
                <select
                    className="rc-scope__value"
                    value={scope.value ?? ""}
                    disabled={disabled}
                    aria-label="Recording"
                    onChange={(e) => pick({ kind: "meeting", value: e.target.value })}
                >
                    {scope.value && !meetings.some((m) => m.id === scope.value) && <option value={scope.value}>This recording</option>}
                    {orderedMeetings.map((m) => (
                        <option key={m.id} value={m.id}>
                            {m.title}{m.class_name ? ` · ${m.class_name}` : ""} · {new Date(m.started_at).toLocaleDateString()}
                        </option>
                    ))}
                </select>
            )}
        </div>
    );
}
