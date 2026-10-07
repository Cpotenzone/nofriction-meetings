// noFriction Meetings - AI consent dialog (App Review 5.1.2(i))
// Shown before the first request to any cloud AI provider. Opened either by
// withAiConsent() (user action hit CONSENT_REQUIRED) or by the backend's
// "ai-consent-required" event (e.g. a background report).

import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ai, onConsentRequest, type ConsentRequest } from "../lib/ai";
import "../features/settings/AIProviderSettings.css";

const NOT_NOW_QUIET_MS = 10 * 60 * 1000;

export function AiConsentModal() {
    const [request, setRequest] = useState<ConsentRequest | null>(null);
    const [endpoint, setEndpoint] = useState<string>("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const queue = useRef<ConsentRequest[]>([]);
    const current = useRef<ConsentRequest | null>(null);
    const dismissedAt = useRef<Record<string, number>>({});
    const show = async (req: ConsentRequest) => {
        // Background prompts respect a recent "Not now"
        const last = dismissedAt.current[req.provider];
        if (!req.userInitiated && last && Date.now() - last < NOT_NOW_QUIET_MS) {
            req.resolve(false);
            return;
        }
        current.current = req;
        setEndpoint("");
        setError(null);
        try {
            const selected = (await ai.listProviders()).find((p) => p.id === req.provider);
            if (!selected?.base_url || selected.id !== "custom") throw new Error("Configure your endpoint before allowing AI.");
            setEndpoint(selected.base_url);
        } catch (e) {
            setError(String(e));
        }
        setRequest(req);
    };

    const enqueue = (req: ConsentRequest) => {
        if (current.current || queue.current.length) {
            // Same provider already pending: share the answer
            const pending = [current.current, ...queue.current].find((r) => r?.provider === req.provider);
            if (pending) {
                const prev = pending.resolve;
                pending.resolve = (ok) => {
                    prev(ok);
                    req.resolve(ok);
                };
                return;
            }
            queue.current.push(req);
            return;
        }
        void show(req);
    };

    useEffect(() => onConsentRequest(enqueue), []);

    useEffect(() => {
        let unlisten: (() => void) | undefined;
        listen<string>("ai-consent-required", (event) => {
            enqueue({ provider: event.payload, userInitiated: false, resolve: () => {} });
        }).then((u) => (unlisten = u));
        return () => unlisten?.();
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    const finish = (allowed: boolean) => {
        const req = current.current;
        if (!req) return;
        if (!allowed) dismissedAt.current[req.provider] = Date.now();
        req.resolve(allowed);
        current.current = null;
        setRequest(null);
        const next = queue.current.shift();
        if (next) void show(next);
    };

    const allow = async () => {
        if (!request || !endpoint) return;
        setBusy(true);
        try {
            await ai.grantConsent(request.provider, endpoint);
            finish(true);
        } catch (e) {
            setError(String(e));
        } finally {
            setBusy(false);
        }
    };

    if (!request) return null;

    return (
        <div className="modal-overlay ai-consent-overlay" role="dialog" aria-modal="true" aria-labelledby="ai-consent-title">
            <div className="modal-content ai-consent-modal">
                <div className="modal-header">
                    <h2 id="ai-consent-title">Send recording content to your endpoint?</h2>
                </div>
                <div className="modal-body">
                    <p className="ai-consent-copy">
                        Destination: <strong style={{ overflowWrap: "anywhere" }}>{endpoint || "Unavailable"}</strong>.
                        If you allow it, noFriction sends transcripts, titles, attendee names,
                        email/company details and selected screenshots needed for AI features directly to this endpoint,
                        using your optional API key. Its operator's privacy policy and terms apply.
                        This is optional. You can use Apple on-device or a local endpoint instead.
                        noFriction offers no hosted models and receives none of this content.
                    </p>
                    {error && <p className="ai-error-text">{error}</p>}
                    <div className="ai-consent-actions">
                        <button className="btn-secondary" onClick={() => finish(false)} disabled={busy}>
                            Not now
                        </button>
                        <button className="btn-primary" onClick={allow} disabled={busy || !endpoint} autoFocus>
                            {busy ? "Saving…" : "Allow"}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
