// noFriction Meetings - build flavor, capabilities and StoreKit (noFriction Pro)
//
// The Rust side reports what this build can do (`get_build_capabilities`):
// the Developer ID "dmg" build has everything and no Pro gating; the Mac App
// Store "mas" build is sandboxed (no ffmpeg video, no Accessibility capture,
// no owner-infra settings) and gates AI behind noFriction Pro.

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

export interface BuildCapabilities {
    flavor: "mas" | "dmg";
    sandboxed: boolean;
    video_recording: boolean;
    accessibility_capture: boolean;
    owner_infra: boolean;
    storekit: boolean;
    pro_gating: boolean;
    apple_intelligence: boolean;
    apple_intelligence_reason: string;
    version: string;
    /** build number (src-tauri/build_number.txt) */
    build: string;
}

/** Used until the backend answers (and if it can't): the full DMG feature set. */
export const DEFAULT_CAPABILITIES: BuildCapabilities = {
    flavor: "dmg",
    sandboxed: false,
    video_recording: true,
    accessibility_capture: true,
    owner_infra: true,
    storekit: false,
    pro_gating: false,
    apple_intelligence: false,
    apple_intelligence_reason: "",
    version: "",
    build: "",
};

let cached: Promise<BuildCapabilities> | null = null;

export function getCapabilities(): Promise<BuildCapabilities> {
    if (!cached) {
        cached = invoke<BuildCapabilities>("get_build_capabilities").catch((e) => {
            console.warn("get_build_capabilities failed; assuming DMG build", e);
            cached = null;
            return DEFAULT_CAPABILITIES;
        });
    }
    return cached;
}

/** React hook; `null` until known so gated UI never flashes in the MAS build. */
export function useCapabilities(): BuildCapabilities | null {
    const [caps, setCaps] = useState<BuildCapabilities | null>(null);
    useEffect(() => {
        let alive = true;
        getCapabilities().then((c) => alive && setCaps(c));
        return () => {
            alive = false;
        };
    }, []);
    return caps;
}

// ---------------------------------------------------------------------------
// StoreKit (Mac App Store build)
// ---------------------------------------------------------------------------

/** Apple's standard EULA (Terms of Use), linked from the paywall (guideline 3.1.2). */
export const TERMS_URL = "https://www.apple.com/legal/internet-services/itunes/dev/stdeula/";
/** Placeholder until the privacy page is published (docs/APP_STORE_RELEASE.md §5.3 step 9). */
export const PRIVACY_URL = "https://nofriction.ai/privacy";
/** Placeholder until the support page is published (site/ support page). */
export const SUPPORT_URL = "https://nofriction.ai/support";
export const SUPPORT_EMAIL = "support@nofriction.ai";

export interface StoreProduct {
    id: string;
    displayName: string;
    description: string;
    displayPrice: string;
    /** e.g. "1 month" */
    period?: string;
    periodUnit?: "day" | "week" | "month" | "year";
    periodValue?: number;
    /** e.g. "1 week free" (only when the user is eligible) */
    introOffer?: string;
}

export interface Entitlement {
    isPro: boolean;
    productId?: string | null;
    expiration?: string | null;
    willRenew?: boolean | null;
    loaded?: boolean;
    error?: string;
}

export interface PurchaseResult {
    status: "purchased" | "cancelled" | "pending" | "unknown";
    entitlement: Entitlement;
}

export const SUBSCRIPTION_EVENT = "subscription-changed";

export const store = {
    products: () => invoke<{ products: StoreProduct[] }>("store_products").then((r) => r.products ?? []),
    purchase: (productId: string) => invoke<PurchaseResult>("store_purchase", { productId }),
    entitlement: () => invoke<Entitlement>("store_entitlement"),
    restore: () => invoke<Entitlement>("store_restore"),
    manageSubscriptions: () => invoke<void>("store_manage_subscriptions"),
};

// ---------------------------------------------------------------------------
// Paywall bus (PRO_REQUIRED from any AI call → PaywallModal)
// ---------------------------------------------------------------------------

export interface PaywallRequest {
    /** resolves true once the user is Pro */
    resolve: (isPro: boolean) => void;
}

type PaywallListener = (req: PaywallRequest) => void;
const paywallListeners = new Set<PaywallListener>();

export function onPaywallRequest(fn: PaywallListener): () => void {
    paywallListeners.add(fn);
    return () => paywallListeners.delete(fn);
}

/** Show the paywall; resolves true if the user ends up with Pro. */
export function requestPaywall(): Promise<boolean> {
    if (paywallListeners.size === 0) return Promise.resolve(false);
    return new Promise((resolve) => paywallListeners.forEach((l) => l({ resolve })));
}

export function isProRequiredError(e: unknown): boolean {
    const s = e instanceof Error ? e.message : String(e);
    return s.includes("PRO_REQUIRED");
}
