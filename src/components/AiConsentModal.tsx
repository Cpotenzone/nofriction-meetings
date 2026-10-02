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
    const [name, setName] = useState<string>("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const queue = useRef<ConsentRequest[]>([]);
    const current = useRef<ConsentRequest | null>(null);
    const dismissedAt = useRef<Record<string, number>>({});
    const names = useRef<Record<string, string>>({});

    const providerName = async (id: string) => {
        if (!names.current[id]) {
            try {
                const list = await ai.listProviders();
                list.forEach((p) => (names.current[p.id] = p.name));
            } catch {
                /* fall back to id */
            }
        }
        return names.current[id] ?? id;
    };

    const show = async (req: ConsentRequest) => {
        // Background prompts respect a recent "Not now"
        const last = dismissedAt.current[req.provider];
        if (!req.userInitiated && last && Date.now() - last < NOT_NOW_QUIET_MS) {
            req.resolve(false);
            return;
        }
        current.current = req;
        setName(await providerName(req.provider));
        setError(null);
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
        if (!request) return;
        setBusy(true);
        try {
            await ai.grantConsent(request.provider);
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
                    <h2 id="ai-consent-title">Send meeting content to {name}?</h2>
                </div>
                <div className="modal-body">
                    <p className="ai-consent-copy">
                        To write notes, summaries and emails, noFriction sends the transcript, the
                        meeting title, attendee names and (for screen features) screenshots to{" "}
                        {name} using your API key. {name}'s privacy policy and terms apply.
                        Nothing is sent to noFriction; we have no servers.
                    </p>
                    {error && <p className="ai-error-text">{error}</p>}
                    <div className="ai-consent-actions">
                        <button className="btn-secondary" onClick={() => finish(false)} disabled={busy}>
                            Not now
                        </button>
                        <button className="btn-primary" onClick={allow} disabled={busy} autoFocus>
                            {busy ? "Saving…" : "Allow"}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
