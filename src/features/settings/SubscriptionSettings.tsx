// noFriction Meetings - Settings → Subscription (Mac App Store build)
// Current noFriction Pro status, the offer when not subscribed, Restore
// Purchases, and a link to Apple's subscription management page.

import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { SUBSCRIPTION_EVENT, store, type Entitlement } from "../../lib/build";
import { ProComparison, ProOffer } from "../../components/PaywallModal";
import { PRO_VALUE } from "../../lib/pro";
import "../../components/PaywallModal.css";

const PLAN_NAMES: Record<string, string> = {
    "com.nofriction.meetings.pro.monthly": "noFriction Pro (monthly)",
    "com.nofriction.meetings.pro.yearly": "noFriction Pro (yearly)",
};

export function SubscriptionSettings() {
    const [ent, setEnt] = useState<Entitlement | null>(null);
    const [error, setError] = useState<string | null>(null);

    const refresh = useCallback(() => {
        store
            .entitlement()
            .then((e) => {
                setEnt(e);
                setError(e.error ?? null);
            })
            .catch((e) => setError(String(e)));
    }, []);

    useEffect(() => {
        refresh();
        let off: (() => void) | undefined;
        listen<Entitlement>(SUBSCRIPTION_EVENT, (ev) => setEnt(ev.payload)).then((u) => (off = u));
        return () => off?.();
    }, [refresh]);

    const manage = () => store.manageSubscriptions().catch((e) => setError(String(e)));

    return (
        <div className="settings-content-panel fade-in">
            <section className="settings-section">
                <h3>noFriction Pro</h3>
                {ent === null ? (
                    <div className="loading-spinner" style={{ margin: "16px auto" }} />
                ) : ent.isPro ? (
                    <>
                        <p className="subscription-status">
                            {PLAN_NAMES[ent.productId ?? ""] ?? "noFriction Pro"} is active
                            {ent.expiration &&
                                ` · ${ent.willRenew === false ? "ends" : "renews"} ${new Date(ent.expiration).toLocaleDateString()}`}
                        </p>
                        <button className="btn-secondary" onClick={manage}>
                            Manage subscription
                        </button>
                    </>
                ) : (
                    <>
                        <p className="paywall-value">{PRO_VALUE}</p>
                        <ProComparison />
                        <ProOffer onPro={refresh} />
                    </>
                )}
                {error && <p className="ai-error-text">{error}</p>}
            </section>
        </div>
    );
}
