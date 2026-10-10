// noFriction Meetings - build flavor, capabilities and StoreKit (noFriction Pro)
//
// The Rust side reports what this build can do (`get_build_capabilities`):
// the Developer ID "dmg" build has everything and no Pro gating; the Mac App
// Store "mas" build is sandboxed (no ffmpeg video, no Accessibility capture,
// no owner-infra settings) and gates AI, Sync and Export to Obsidian behind
// noFriction Pro (docs/PRO.md).

import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { proFeatureFromError, type ProFeature } from "./pro";

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
/** Owner-approved site; the meeting-app policy supplement remains a release gate. */
export const PRIVACY_URL = "https://nofriction.io/privacy";
/** Placeholder until the support page is published (site/ support page). */
export const SUPPORT_URL = "https://nofriction.io/contact";
export const SUPPORT_EMAIL = "casey@nofriction.io";

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
// Paywall bus (PRO_REQUIRED from any AI call or Pro feature → PaywallModal)
// ---------------------------------------------------------------------------

export interface PaywallRequest {
    /** The feature that opened the paywall (its title names it); none from Settings. */
    feature?: ProFeature | null;
    /** resolves true once the user is Pro */
    resolve: (isPro: boolean) => void;
}

type PaywallListener = (req: PaywallRequest) => void;
const paywallListeners = new Set<PaywallListener>();

export function onPaywallRequest(fn: PaywallListener): () => void {
    paywallListeners.add(fn);
    return () => paywallListeners.delete(fn);
}

/**
 * Show the paywall, titled for `feature` ("Sync is part of noFriction Pro");
 * resolves true if the user ends up with Pro. Never opens in the DMG build.
 */
export function requestPaywall(feature?: ProFeature | null): Promise<boolean> {
    if (paywallListeners.size === 0) return Promise.resolve(false);
    return new Promise((resolve) => paywallListeners.forEach((l) => l({ feature, resolve })));
}

export function isProRequiredError(e: unknown): boolean {
    return proFeatureFromError(e) !== null;
}

/**
 * Run a call to a non-AI Pro feature (Sync, Export to Obsidian). If the
 * backend answers `PRO_REQUIRED:<key>`, open the paywall for that feature
 * and retry once the user subscribes; otherwise rethrow.
 */
export async function withPro<T>(fn: () => Promise<T>, feature?: ProFeature): Promise<T> {
    try {
        return await fn();
    } catch (e) {
        const asked = proFeatureFromError(e);
        if (!asked) throw e;
        if (!(await requestPaywall(asked === "ai" ? feature ?? asked : asked))) throw e;
        return fn();
    }
}
