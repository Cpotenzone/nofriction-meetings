// noFriction Meetings - noFriction Pro paywall (Mac App Store build only).
// Opened by withAiConsent() when an AI call returns PRO_REQUIRED, or from
// Settings → Subscription. Shows price, billing period, trial terms,
// Restore Purchases, and links to Terms (Apple standard EULA) and Privacy
// (App Review guideline 3.1.2).

import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
    PRIVACY_URL,
    TERMS_URL,
    getCapabilities,
    onPaywallRequest,
    store,
    type PaywallRequest,
    type StoreProduct,
} from "../lib/build";
import "../features/settings/AIProviderSettings.css";
import "./PaywallModal.css";

export const PRO_FEATURES = [
    "AI notes, summaries and to-dos",
    "Follow-up email drafts",
    "Chat with your recordings",
    "Pre-meeting attendee briefings",
];

export const FREE_FEATURES = "Recording, transcription, calendar & people, screenshots and export stay free.";

function billingLine(p: StoreProduct): string {
    return p.period ? `${p.displayPrice} per ${p.period.replace(/^1 /, "")}` : p.displayPrice;
}

/** Product list + buy/restore; shared by the modal and Settings → Subscription. */
export function ProOffer({ onPro }: { onPro: () => void }) {
    const [products, setProducts] = useState<StoreProduct[] | null>(null);
    const [busy, setBusy] = useState<string | null>(null);
    const [message, setMessage] = useState<string | null>(null);

    useEffect(() => {
        store
            .products()
            .then(setProducts)
            .catch((e) => {
                setProducts([]);
                setMessage(`Couldn't load prices from the App Store: ${String(e)}`);
            });
    }, []);

    const buy = async (id: string) => {
        setBusy(id);
        setMessage(null);
        try {
            const r = await store.purchase(id);
            if (r.status === "purchased" && r.entitlement.isPro) onPro();
            else if (r.status === "pending") setMessage("Purchase pending approval. Pro unlocks as soon as it's approved.");
        } catch (e) {
            setMessage(`Purchase failed: ${String(e)}`);
        } finally {
            setBusy(null);
        }
    };

    const restore = async () => {
        setBusy("restore");
        setMessage(null);
        try {
            const e = await store.restore();
            if (e.isPro) onPro();
            else setMessage("No active noFriction Pro subscription was found for this Apple ID.");
        } catch (e) {
            setMessage(`Restore failed: ${String(e)}`);
        } finally {
            setBusy(null);
        }
    };

    const open = (url: string) => openUrl(url).catch((e) => console.error("open failed", e));

    return (
        <div className="paywall-offer">
            {products === null ? (
                <div className="loading-spinner" style={{ margin: "16px auto" }} />
            ) : (
                <div className="paywall-products">
                    {products.map((p) => (
                        <button
                            key={p.id}
                            className="paywall-product"
                            onClick={() => buy(p.id)}
                            disabled={busy !== null}
                        >
                            <span className="paywall-product-name">{p.displayName}</span>
                            <span className="paywall-product-price">{billingLine(p)}</span>
                            {p.introOffer && (
                                <span className="paywall-product-trial">
                                    {p.introOffer}, then {billingLine(p)}
                                </span>
                            )}
                            <span className="paywall-product-cta">{busy === p.id ? "Opening App Store…" : "Subscribe"}</span>
                        </button>
                    ))}
                    {products.length === 0 && !message && <p className="ai-error-text">No subscriptions are available right now.</p>}
                </div>
            )}
            {message && <p className="paywall-message">{message}</p>}
            <p className="paywall-legal">
                Payment is charged to your Apple ID. Subscriptions renew automatically unless cancelled at
                least 24 hours before the end of the current period; manage or cancel them in your App Store
                account settings. Any unused free-trial time ends when you subscribe.
            </p>
            <div className="paywall-links">
                <button className="ai-link-button" onClick={restore} disabled={busy !== null}>
                    {busy === "restore" ? "Restoring…" : "Restore Purchases"}
                </button>
                <button className="ai-link-button" onClick={() => open(TERMS_URL)}>
                    Terms of Use
                </button>
                <button className="ai-link-button" onClick={() => open(PRIVACY_URL)}>
                    Privacy Policy
                </button>
            </div>
        </div>
    );
}

export function PaywallModal() {
    const [request, setRequest] = useState<PaywallRequest | null>(null);
    const current = useRef<PaywallRequest | null>(null);
    const pending = useRef<PaywallRequest[]>([]);

    useEffect(() => {
        return onPaywallRequest((req) => {
            void getCapabilities().then((c) => {
                // DMG build: no gating, nothing to sell
                if (!c.storekit) return req.resolve(false);
                // One modal at a time; everyone waiting gets the same answer
                if (current.current) {
                    pending.current.push(req);
                    return;
                }
                current.current = req;
                setRequest(req);
            });
        });
    }, []);

    const finish = (isPro: boolean) => {
        current.current?.resolve(isPro);
        pending.current.splice(0).forEach((r) => r.resolve(isPro));
        current.current = null;
        setRequest(null);
    };

    if (!request) return null;

    return (
        <div className="modal-overlay ai-consent-overlay" role="dialog" aria-modal="true" aria-labelledby="paywall-title">
            <div className="modal-content ai-consent-modal paywall-modal">
                <div className="modal-header">
                    <h2 id="paywall-title">Unlock noFriction Pro</h2>
                </div>
                <div className="modal-body">
                    <ul className="paywall-features">
                        {PRO_FEATURES.map((f) => (
                            <li key={f}>{f}</li>
                        ))}
                    </ul>
                    <p className="paywall-free">{FREE_FEATURES}</p>
                    <ProOffer onPro={() => finish(true)} />
                    <div className="ai-consent-actions">
                        <button className="btn-secondary" onClick={() => finish(false)}>
                            Not now
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
