// noFriction Meetings - "Add an AI key" notice
// Shown by every AI entry point when no AI provider is set up (instead of a
// raw AI_NO_PROVIDER error), with a button to Settings → AI Engine.

import { useCallback, useEffect, useState } from "react";
import { ai, AI_STATUS_EVENT, type AiStatus } from "../lib/ai";
import { isOffline } from "../lib/offline";
import { openSettings } from "../lib/navigation";
import { SparkleIcon } from "./icons";

/**
 * Current AI status; `null` while loading. Re-checks when AI settings
 * change (AI_STATUS_EVENT) and when the window regains focus.
 */
export function useAiStatus(): { status: AiStatus | null; configured: boolean | null; refresh: () => void } {
    const [status, setStatus] = useState<AiStatus | null>(null);
    const [loaded, setLoaded] = useState(false);

    const refresh = useCallback(() => {
        if (isOffline()) {
            setLoaded(true);
            return;
        }
        ai.status()
            .then((s) => setStatus(s))
            .catch(() => setStatus(null))
            .finally(() => setLoaded(true));
    }, []);

    useEffect(() => {
        refresh();
        window.addEventListener(AI_STATUS_EVENT, refresh);
        window.addEventListener("focus", refresh);
        return () => {
            window.removeEventListener(AI_STATUS_EVENT, refresh);
            window.removeEventListener("focus", refresh);
        };
    }, [refresh]);

    // In a browser preview there is no backend: treat AI as configured so
    // previews show the real UI.
    const configured = !loaded ? null : isOffline() ? true : !!status?.text;
    return { status, configured, refresh };
}

interface AiSetupNoticeProps {
    /** What needs AI, e.g. "AI notes" → "AI notes need an AI provider." */
    feature?: string;
    compact?: boolean;
}

export function AiSetupNotice({ feature, compact = false }: AiSetupNoticeProps) {
    return (
        <div className={`nf-ai-setup ${compact ? "compact" : ""}`} role="status">
            {!compact && (
                <div className="nf-ai-setup__icon">
                    <SparkleIcon size={22} strokeWidth={1.5} />
                </div>
            )}
            <div className="nf-ai-setup__text">
                <strong>Add an AI key in Settings → AI Engine</strong>
                <span>
                    {feature ? `${feature} need${feature.endsWith("s") ? "" : "s"} an AI provider. ` : ""}
                    Paste a key from OpenAI or another provider, connect a local model, or use Apple's
                    on-device model where available.
                </span>
            </div>
            <button className="nf-ai-setup__btn" type="button" onClick={() => openSettings("ai")}>
                Open AI Engine
            </button>
        </div>
    );
}
