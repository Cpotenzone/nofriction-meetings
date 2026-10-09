import { strict as assert } from "node:assert";
import { describe, it } from "node:test";
import type { AiPreset, AiProviderInfo } from "./ai.ts";
import {
    APPLE_CARD,
    CUSTOM_CARD,
    applyCustom,
    applyPreset,
    consentDestination,
    hostOf,
    initialCard,
    presetForUrl,
    whatWillBeSent,
} from "./aiPresets.ts";

// Fixture table: shaped like the backend's, with fixture hosts (the real
// table lives in src-tauri/src/ai/providers.rs and is policy-checked there).
const presets: AiPreset[] = [
    { id: "one", name: "Provider One", base_url: "https://one.example/v1", default_model: "one-large", model_hint: "one-small", key_url: "https://one.example/keys", note: "Your One key." },
    { id: "two", name: "Provider Two", base_url: "https://two.example/v1", default_model: "two-fast", model_hint: "", key_url: "https://two.example/keys", note: "Your Two key." },
];

function provider(over: Partial<AiProviderInfo>): AiProviderInfo {
    return {
        id: "custom", name: "Custom (OpenAI-compatible)", preset: null, protocol: "openai", base_url: null, key_url: "",
        key: "optional", local: false, editable_url: true, configured: false, last4: null, consent: false,
        needs_consent: false, model: null, vision_model: null, models: [], active_text: false, active_vision: false,
        ...over,
    };
}

describe("preset selection", () => {
    it("fills the URL and default model and never a key", () => {
        const form = applyPreset(presets[0]!);
        assert.deepEqual(form, { url: "https://one.example/v1", model: "one-large", key: "" });
    });

    it("switching presets replaces the URL and model and clears the typed key", () => {
        const a = { ...applyPreset(presets[0]!), key: "typed-key-fixture" };
        assert.equal(a.key, "typed-key-fixture");
        const b = applyPreset(presets[1]!);
        assert.equal(b.url, "https://two.example/v1");
        assert.equal(b.model, "two-fast");
        assert.equal(b.key, "", "a key typed for one host is never carried to another");
    });

    it("the custom card starts empty (no default remote URL)", () => {
        assert.deepEqual(applyCustom(), { url: "", model: "", key: "" });
    });

    it("matches a form URL back to its preset, tolerating a trailing slash and case", () => {
        assert.equal(presetForUrl(presets, "https://one.example/v1/")?.id, "one");
        assert.equal(presetForUrl(presets, "HTTPS://two.example/v1")?.id, "two");
        assert.equal(presetForUrl(presets, "https://proxy.example/v1"), null);
        assert.equal(presetForUrl(presets, ""), null);
    });
});

describe("no preset active by default", () => {
    it("a fresh install highlights no card", () => {
        assert.equal(initialCard([provider({}), provider({ id: "apple", name: "Apple on-device", protocol: "apple" })], presets), null);
    });

    it("a saved preset URL highlights its card; another URL highlights custom; Apple wins when active", () => {
        assert.equal(initialCard([provider({ base_url: "https://two.example/v1" })], presets), "two");
        assert.equal(initialCard([provider({ base_url: "http://127.0.0.1:11434/v1" })], presets), CUSTOM_CARD);
        assert.equal(
            initialCard([provider({ base_url: "https://two.example/v1" }), provider({ id: "apple", protocol: "apple", active_text: true })], presets),
            APPLE_CARD,
        );
    });
});

describe("destination copy", () => {
    it("the consent dialog shows the real host and endpoint of the saved connection", () => {
        const d = consentDestination(provider({ base_url: "https://one.example/v1", name: "Provider One", preset: "one" }));
        assert.deepEqual(d, { endpoint: "https://one.example/v1", host: "one.example", name: "Provider One" });
        assert.equal(consentDestination(provider({})), null, "no URL, no consent dialog");
        assert.equal(consentDestination(provider({ id: "apple", base_url: "apple://on-device" })), null);
    });

    it("hostOf is safe on junk", () => {
        assert.equal(hostOf("https://one.example:8443/v1"), "one.example:8443");
        assert.equal(hostOf("not a url"), "not a url");
    });

    it("explains where requests go and that the test sends only Hi", () => {
        const s = whatWillBeSent({ url: "https://one.example/v1", model: "one-large", key: "" }, presets[0]!);
        assert.ok(s.includes("Provider One at one.example"));
        assert.ok(s.includes("one-large"));
        assert.ok(s.includes('"Hi"'));
        assert.ok(whatWillBeSent({ url: "", model: "", key: "" }, null).startsWith("Enter a base URL"));
        assert.ok(whatWillBeSent({ url: "http://box.local:1234/v1", model: "", key: "" }, null).includes("box.local:1234"));
    });
});
