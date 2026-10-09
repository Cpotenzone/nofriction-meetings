// "Set up AI" notice: shown by every AI entry point when no AI is set up
// (instead of a raw AI_NO_PROVIDER error), with a button to Settings → AI.

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
    /** What needs AI, e.g. "Notes" → "Notes need AI." */
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
                <strong>Set up AI in Settings → AI</strong>
                <span>
                    {feature ? `${feature} need${feature.endsWith("s") ? "" : "s"} AI. ` : ""}
                    Use Apple's on-device model where available, or enter one endpoint with your own key.
                </span>
            </div>
            <button className="nf-ai-setup__btn" type="button" onClick={() => openSettings("ai")}>
                Open Settings
            </button>
        </div>
    );
}
