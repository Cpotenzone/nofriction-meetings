// noFriction Meetings - AI provider presets (pure helpers, unit tested)
//
// A preset is UI convenience over the one custom OpenAI-compatible endpoint:
// picking a card fills the base URL and a default model in the form. The
// table itself comes from the backend (`ai.listPresets()`), so no provider
// URL is hardcoded here. Nothing is selected until the user clicks, the key
// is always the user's own, and the saved connection stays a custom endpoint
// with the same Keychain binding and consent rules.

import type { AiPreset, AiProviderInfo } from "./ai";

/** Card id for "enter your own endpoint" (not a preset; keeps the form as typed). */
export const CUSTOM_CARD = "custom";
/** Card id for Apple on-device (the existing provider, no URL). */
export const APPLE_CARD = "apple";

export interface EndpointForm {
    url: string;
    model: string;
    /** Key input is cleared on a switch so a key is never carried to another host. */
    key: string;
}

export const EMPTY_FORM: EndpointForm = { url: "", model: "", key: "" };

/** Fill the form from a preset. The user's own key is still required. */
export function applyPreset(preset: AiPreset): EndpointForm {
    return { url: preset.base_url, model: preset.default_model, key: "" };
}

/** The custom card leaves the fields empty: there is no default remote URL. */
export function applyCustom(): EndpointForm {
    return { ...EMPTY_FORM };
}

/** Which card matches the URL currently in the form (null = none / custom). */
export function presetForUrl(presets: AiPreset[], url: string): AiPreset | null {
    const u = normalizeBase(url);
    if (!u) return null;
    return presets.find((p) => normalizeBase(p.base_url) === u) ?? null;
}

/**
 * The card to highlight when the settings screen opens: the saved
 * connection's matched preset, or nothing. A fresh install has no saved URL,
 * so no preset is ever selected without the user's click.
 */
export function initialCard(providers: AiProviderInfo[], presets: AiPreset[]): string | null {
    const custom = providers.find((p) => p.id === "custom");
    const apple = providers.find((p) => p.id === "apple");
    if (apple?.active_text) return APPLE_CARD;
    if (!custom?.base_url) return null;
    return presetForUrl(presets, custom.base_url)?.id ?? CUSTOM_CARD;
}

/** Host part for "will send to" copy; falls back to the raw string. */
export function hostOf(url: string): string {
    try {
        return new URL(url.trim()).host || url.trim();
    } catch {
        return url.trim();
    }
}

/** What the consent dialog must show: the real destination, never just a brand. */
export function consentDestination(p: AiProviderInfo | undefined): { endpoint: string; host: string; name: string } | null {
    if (!p || p.id !== "custom" || !p.base_url) return null;
    return { endpoint: p.base_url, host: hostOf(p.base_url), name: p.name };
}

/** Plain-words line under the form for the chosen card. */
export function whatWillBeSent(form: EndpointForm, preset: AiPreset | null): string {
    const url = form.url.trim();
    if (!url) return "Enter a base URL to see where requests will go.";
    const host = hostOf(url);
    const model = form.model.trim() || "(choose a model)";
    const who = preset ? `${preset.name} at ${host}` : host;
    return `Requests go straight from this Mac to ${who} using model ${model} and your own key. Nothing is sent until you save and allow it; the connection test sends only the word "Hi".`;
}

function normalizeBase(url: string): string {
    const t = url.trim().replace(/\/+$/, "").toLowerCase();
    return t;
}
