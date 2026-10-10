// AI API: Apple on-device or the one endpoint the user entered
// Thin typed wrappers over the ai_* Tauri commands, plus the consent bus
// used to show the "Send recording content to your endpoint?" dialog whenever a
// call comes back with CONSENT_REQUIRED:<provider>.

import { invoke } from "@tauri-apps/api/core";
import { isProRequiredError, requestPaywall } from "./build";
import { proFeatureFromError, type ProFeature } from "./pro";

export type AiKind = "text" | "vision";

export interface AiModelInfo {
    id: string;
    context?: number;
    vision?: boolean;
}

export interface AiProviderInfo {
    id: string;
    /** The matched preset's name when the URL is a preset's, else the generic name */
    name: string;
    /** Preset id matching the saved URL (derived from the URL, never stored) */
    preset: string | null;
    protocol: "openai" | "anthropic" | "apple";
    base_url: string | null;
    key_url: string;
    key: "required" | "optional" | "none";
    local: boolean;
    editable_url: boolean;
    configured: boolean;
    last4: string | null;
    consent: boolean;
    needs_consent: boolean;
    model: string | null;
    vision_model: string | null;
    models: AiModelInfo[];
    active_text: boolean;
    active_vision: boolean;
}

export interface AiDetection {
    provider: string | null;
    name: string | null;
    alternatives: string[];
}

export interface AiSaveKeyResult {
    provider: string;
    name: string;
    models: string[];
    model: string | null;
    vision_model: string | null;
    needs_consent: boolean;
    last4: string | null;
}

export interface AiActiveInfo {
    provider: string;
    name: string;
    model: string;
    local: boolean;
    consent: boolean;
    state: string;
}

export interface AiStatus {
    text: AiActiveInfo | null;
    vision: AiActiveInfo | null;
    text_ready: boolean;
    vision_ready: boolean;
    what_leaves: string;
}

/** Result of the one-token connection test, already in plain words. */
export interface AiTestResult {
    ok: boolean;
    /** connected | wrong_key | no_credit | unreachable | bad_url | model_missing | no_key | other */
    class: string;
    message: string;
}

/**
 * A named provider preset: static data from the backend table. Choosing one
 * only fills the endpoint form; nothing is saved, selected or contacted.
 */
export interface AiPreset {
    id: string;
    name: string;
    base_url: string;
    default_model: string;
    model_hint: string;
    key_url: string;
    note: string;
}

export const ai = {
    listProviders: () => invoke<AiProviderInfo[]>("ai_list_providers"),
    listPresets: () => invoke<AiPreset[]>("ai_list_presets"),
    detect: (key: string) => invoke<AiDetection>("ai_detect_provider", { key }),
    saveKey: (key: string, provider: string, expectedBaseUrl: string) =>
        invoke<AiSaveKeyResult>("ai_save_key", { key, provider, expectedBaseUrl }),
    deleteKey: (provider: string) => invoke<AiStatus>("ai_delete_key", { provider }),
    setActive: (provider: string, model: string | null, kind: AiKind) =>
        invoke<AiStatus>("ai_set_active", { provider, model, kind }),
    clearVision: () => invoke<AiStatus>("ai_clear_vision"),
    setEndpoint: (provider: string, baseUrl: string) =>
        invoke<AiProviderInfo>("ai_set_custom_endpoint", { provider, baseUrl }),
    listModels: (provider: string) => invoke<AiModelInfo[]>("ai_list_models", { provider }),
    test: (provider: string) => invoke<AiTestResult>("ai_test", { provider }),
    grantConsent: (provider: string, expectedBaseUrl: string) => invoke<AiStatus>("ai_grant_consent", { provider, expectedBaseUrl }),
    revokeConsent: (provider: string) => invoke<AiStatus>("ai_revoke_consent", { provider }),
    status: () => invoke<AiStatus>("ai_status"),
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

const errText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Provider id if this error means "consent needed", else null. */
export function consentProviderFromError(e: unknown): string | null {
    const m = /CONSENT_REQUIRED:([a-z0-9_-]+)/.exec(errText(e));
    return m ? m[1] : null;
}

/** Short class for an AI error string (matches the backend prefixes). */
export function aiErrorClass(e: unknown): string {
    const s = errText(e);
    if (s.includes("CONSENT_REQUIRED:")) return "consent_required";
    if (s.includes("PRO_REQUIRED")) return "pro_required";
    // Commands often wrap the AI error ("Intel generation failed: AI_…: …")
    const m = /(?:^|\s)(AI_[A-Z_]+|UNKNOWN_PROVIDER):/.exec(s);
    if (!m) return "other";
    return (
        {
            AI_WRONG_KEY: "wrong_key",
            AI_NO_CREDIT: "no_credit",
            AI_UNREACHABLE: "unreachable",
            AI_BAD_URL: "bad_url",
            AI_NO_PROVIDER: "no_provider",
            AI_NO_KEY: "no_key",
            AI_NO_VISION: "vision_unavailable",
            AI_TRUNCATED: "truncated",
            UNKNOWN_PROVIDER: "unknown_provider",
        } as Record<string, string>
    )[m[1]] ?? "other";
}

/** Human-readable error without the machine prefix. */
export function friendlyAiError(e: unknown): string {
    const s = errText(e);
    const labels: Record<string, string> = {
        wrong_key: "Wrong key",
        no_credit: "No credit / rate-limited",
        unreachable: "Can't reach the endpoint",
        bad_url: "Check the URL",
        no_provider: "AI isn't set up",
        no_key: "No key saved",
        vision_unavailable: "No vision model",
        truncated: "Answer cut off",
        consent_required: "Needs your permission",
        pro_required: "noFriction Pro",
        unknown_provider: "Unknown AI connection",
    };
    const cls = aiErrorClass(s);
    if (cls === "no_provider" || cls === "no_key") return NO_AI_MESSAGE;
    const detail = s.replace(/^.*?(AI_[A-Z_]+|UNKNOWN_PROVIDER|CONSENT_REQUIRED|PRO_REQUIRED(?::[a-z_]+)?):\s*/, "");
    if (cls === "other") return detail;
    return `${labels[cls]}: ${detail}`;
}

/** Shown wherever an AI feature is used before any provider is set up. */
export const NO_AI_MESSAGE = "Set up AI in Settings → AI to use this.";

/** True when the error means "no AI provider / key set up yet". */
export function isNoProviderError(e: unknown): boolean {
    const cls = aiErrorClass(e);
    return cls === "no_provider" || cls === "no_key";
}

/** Fired after AI settings change so open screens re-check ai.status(). */
export const AI_STATUS_EVENT = "nf:ai-status-changed";

export function notifyAiStatusChanged(): void {
    window.dispatchEvent(new Event(AI_STATUS_EVENT));
}

// ---------------------------------------------------------------------------
// Consent bus
// ---------------------------------------------------------------------------

export interface ConsentRequest {
    provider: string;
    /** true when the user started this action (always show the dialog) */
    userInitiated: boolean;
    resolve: (allowed: boolean) => void;
}

type ConsentListener = (req: ConsentRequest) => void;
const listeners = new Set<ConsentListener>();

export function onConsentRequest(fn: ConsentListener): () => void {
    listeners.add(fn);
    return () => listeners.delete(fn);
}

/** Show the consent dialog for `provider`; resolves true on Allow. */
export function requestConsent(provider: string, userInitiated = true): Promise<boolean> {
    if (listeners.size === 0) return Promise.resolve(false);
    return new Promise((resolve) => {
        listeners.forEach((l) => l({ provider, userInitiated, resolve }));
    });
}

/**
 * Run a user-initiated AI call. If it needs noFriction Pro (Mac App Store
 * build), show the paywall titled for `feature`; if it needs consent for a
 * cloud provider, show the consent dialog. Retries after the user
 * subscribes / allows.
 */
export async function withAiConsent<T>(fn: () => Promise<T>, feature?: ProFeature): Promise<T> {
    for (let attempt = 0; ; attempt++) {
        try {
            return await fn();
        } catch (e) {
            if (attempt >= 2) throw e;
            if (isProRequiredError(e)) {
                // The AI gate says "ai"; the caller knows which feature it was
                const asked = proFeatureFromError(e);
                if (!(await requestPaywall(asked === "ai" ? feature ?? asked : asked))) throw e;
                continue;
            }
            const provider = consentProviderFromError(e);
            if (!provider) throw e;
            if (!(await requestConsent(provider, true))) throw e;
        }
    }
}
