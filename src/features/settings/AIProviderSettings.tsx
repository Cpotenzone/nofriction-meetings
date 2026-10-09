// noFriction Meetings - AI settings (bring your own key)
// Enter your own OpenAI-compatible endpoint and model, or use Apple on-device.
// Keys go straight to the
// macOS Keychain; this screen only ever sees the last 4 characters.

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
    ai,
    friendlyAiError,
    notifyAiStatusChanged,
    requestConsent,
    type AiKind,
    type AiPreset,
    type AiProviderInfo,
    type AiStatus,
} from "../../lib/ai";
import {
    APPLE_CARD,
    CUSTOM_CARD,
    EMPTY_FORM,
    applyCustom,
    applyPreset,
    hostOf,
    initialCard,
    presetForUrl,
    whatWillBeSent,
    type EndpointForm,
} from "../../lib/aiPresets";
import { KnowledgeBaseSettings } from "./KnowledgeBaseSettings";
import "./AIProviderSettings.css";

type Feedback = { ok: boolean; text: string } | null;

const LOCAL_IDS = ["custom"];

export function AIProviderSettings() {
    const [providers, setProviders] = useState<AiProviderInfo[]>([]);
    const [status, setStatus] = useState<AiStatus | null>(null);
    const [loading, setLoading] = useState(true);

    const refresh = useCallback(async () => {
        try {
            const [p, s] = await Promise.all([ai.listProviders(), ai.status()]);
            setProviders(p);
            setStatus(s);
            // Open screens with "Add an AI key" notices re-check
            notifyAiStatusChanged();
        } catch (e) {
            console.error("AI settings load failed:", e);
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        refresh();
    }, [refresh]);

    const byId = useMemo(() => Object.fromEntries(providers.map((p) => [p.id, p])), [providers]);
    const saved = providers.filter((p) => p.configured);

    if (loading) {
        return <div className="loading-spinner" style={{ margin: "40px auto" }} />;
    }

    return (
        <div className="ai-settings">
            <ActiveSummary status={status} onChange={refresh} />
            <SavedProviders saved={saved} onChange={refresh} />
            <ModelPickers providers={providers} byId={byId} status={status} onChange={refresh} />
            <AutomaticAi configured={!!status?.text} />
            <ConnectProvider providers={providers} onChange={refresh} />
            <Advanced />
        </div>
    );
}

// ---------------------------------------------------------------------------

function ActiveSummary({ status, onChange }: { status: AiStatus | null; onChange: () => void }) {
    const text = status?.text;
    const vision = status?.vision;
    const needsConsent = text && !text.local && !text.consent;

    const allow = async () => {
        if (text && (await requestConsent(text.provider))) onChange();
    };
    const revoke = async () => {
        if (text) {
            await ai.revokeConsent(text.provider);
            onChange();
        }
    };

    return (
        <section className="settings-section">
            <h3>AI provider</h3>
            <p className="section-desc">
                Use Apple on-device, pick a provider preset (your own key), or enter any OpenAI-compatible
                endpoint. No provider is active by default. Keys stay in your macOS Keychain, tied to the endpoint.
            </p>
            <div className="ai-active-grid">
                <div className="ai-active-item">
                    <span className="ai-active-label">Notes, summaries & chat</span>
                    <span className="ai-active-value">
                        {text ? `${text.name} · ${text.model}` : "Not set up"}
                        {text && <StateBadge state={text.state} />}
                    </span>
                </div>
                <div className="ai-active-item">
                    <span className="ai-active-label">Screenshots (vision)</span>
                    <span className="ai-active-value">
                        {vision ? `${vision.name} · ${vision.model}` : "Off"}
                        {vision && <StateBadge state={vision.state} />}
                    </span>
                </div>
            </div>
            <p className="ai-what-leaves">
                <strong>What leaves this device:</strong> {status?.what_leaves ?? "…"}
            </p>
            {text && !text.local && (
                <div className="ai-inline-actions">
                    {needsConsent ? (
                        <button className="btn-primary" onClick={allow}>
                            Allow sending to {text.name}
                        </button>
                    ) : (
                        <button className="ai-link-button" onClick={revoke}>
                            Revoke permission to send to {text.name}
                        </button>
                    )}
                </div>
            )}
        </section>
    );
}

function StateBadge({ state }: { state: string }) {
    const labels: Record<string, [string, string]> = {
        ready: ["Ready", "ok"],
        consent_required: ["Needs permission", "warn"],
        no_key: ["No key", "err"],
        bad_url: ["Check URL", "err"],
    };
    const [label, cls] = labels[state] ?? [state, "warn"];
    return <span className={`ai-badge ai-badge-${cls}`}>{label}</span>;
}

// ---------------------------------------------------------------------------

function SavedProviders({ saved, onChange }: { saved: AiProviderInfo[]; onChange: () => void }) {
    const [results, setResults] = useState<Record<string, Feedback>>({});
    const [busy, setBusy] = useState<string | null>(null);

    const test = async (p: AiProviderInfo) => {
        setBusy(p.id);
        try {
            const r = await ai.test(p.id);
            setResults((x) => ({ ...x, [p.id]: { ok: r.ok, text: r.ok ? `✓ ${r.message}` : r.message } }));
        } catch (e) {
            setResults((x) => ({ ...x, [p.id]: { ok: false, text: friendlyAiError(e) } }));
        } finally {
            setBusy(null);
            onChange();
        }
    };

    const remove = async (p: AiProviderInfo) => {
        if (!confirm(`Remove ${p.name}? Its key is deleted from your Keychain.`)) return;
        await ai.deleteKey(p.id);
        onChange();
    };

    const use = async (p: AiProviderInfo) => {
        try {
            await ai.setActive(p.id, p.model, "text");
            onChange();
        } catch (e) {
            setResults((x) => ({ ...x, [p.id]: { ok: false, text: friendlyAiError(e) } }));
        }
    };

    if (saved.length === 0) return null;

    return (
        <section className="settings-section">
            <h3>Saved providers</h3>
            {saved.map((p) => (
                <div className="settings-row ai-provider-row" key={p.id}>
                    <div className="settings-label">
                        <span className="label-main">
                            {p.name}
                            {p.active_text && <span className="ai-badge ai-badge-ok">Text</span>}
                            {p.active_vision && <span className="ai-badge ai-badge-ok">Vision</span>}
                            {p.local && <span className="ai-badge">Local</span>}
                        </span>
                        <span className="label-sub">
                            {p.last4 ? `Key ••••${p.last4}` : p.key === "required" ? "Key saved" : p.base_url}
                            {p.model ? ` · ${p.model}` : ""}
                            {!p.local && !p.consent ? " · needs permission" : ""}
                        </span>
                        {results[p.id] && (
                            <span className={results[p.id]!.ok ? "ai-ok-text" : "ai-error-text"}>{results[p.id]!.text}</span>
                        )}
                    </div>
                    <div className="ai-row-actions">
                        {!p.active_text && (
                            <button className="btn-secondary" onClick={() => use(p)}>
                                Use
                            </button>
                        )}
                        <button className="btn-secondary" onClick={() => test(p)} disabled={busy === p.id}>
                            {busy === p.id ? "Testing…" : "Test"}
                        </button>
                        {p.key !== "none" && (
                            <button className="btn-danger-outline" onClick={() => remove(p)}>
                                Remove
                            </button>
                        )}
                    </div>
                </div>
            ))}
        </section>
    );
}

// ---------------------------------------------------------------------------

function ModelPickers({
    providers,
    byId,
    status,
    onChange,
}: {
    providers: AiProviderInfo[];
    byId: Record<string, AiProviderInfo>;
    status: AiStatus | null;
    onChange: () => void;
}) {
    const usable = providers.filter((p) => p.configured);
    if (usable.length === 0) return null;
    return (
        <section className="settings-section">
            <h3>Models</h3>
            <ModelPicker kind="text" label="Notes, summaries & chat" usable={usable} byId={byId} current={status?.text ?? null} onChange={onChange} />
            <ModelPicker kind="vision" label="Screenshots (vision)" usable={usable} byId={byId} current={status?.vision ?? null} onChange={onChange} />
        </section>
    );
}

function ModelPicker({
    kind,
    label,
    usable,
    byId,
    current,
    onChange,
}: {
    kind: AiKind;
    label: string;
    usable: AiProviderInfo[];
    byId: Record<string, AiProviderInfo>;
    current: { provider: string; model: string } | null;
    onChange: () => void;
}) {
    const [provider, setProvider] = useState(current?.provider ?? usable[0]?.id ?? "");
    const [custom, setCustom] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [refreshing, setRefreshing] = useState(false);

    useEffect(() => {
        if (current?.provider) setProvider(current.provider);
    }, [current?.provider]);

    const p = byId[provider];
    const models = (p?.models ?? []).filter((m) => kind === "text" || m.vision !== false);
    const allowFreeText = p && (LOCAL_IDS.includes(p.id) || p.id === "openrouter");

    const apply = async (model: string | null) => {
        setError(null);
        try {
            await ai.setActive(provider, model, kind);
            onChange();
        } catch (e) {
            setError(friendlyAiError(e));
        }
    };

    const refreshModels = async () => {
        setRefreshing(true);
        setError(null);
        try {
            await ai.listModels(provider);
            onChange();
        } catch (e) {
            setError(friendlyAiError(e));
        } finally {
            setRefreshing(false);
        }
    };

    return (
        <div className="settings-row ai-model-row">
            <div className="settings-label">
                <span className="label-main">{label}</span>
                <span className="label-sub">
                    {kind === "vision"
                        ? "Needs a model that can read images. Off = use the text model when it can."
                        : "Used for every AI feature except screenshots."}
                </span>
                {error && <span className="ai-error-text">{error}</span>}
            </div>
            <div className="ai-model-controls">
                <select className="ai-select" value={provider} onChange={(e) => setProvider(e.target.value)} aria-label={`${label} provider`}>
                    {usable.map((u) => (
                        <option key={u.id} value={u.id}>
                            {u.name}
                        </option>
                    ))}
                </select>
                <select
                    className="ai-select"
                    value={current?.provider === provider ? current.model : ""}
                    onChange={(e) => (e.target.value === "__off" ? ai.clearVision().then(onChange) : apply(e.target.value))}
                    aria-label={`${label} model`}
                >
                    <option value="" disabled>
                        {models.length ? "Choose a model…" : "No models loaded"}
                    </option>
                    {kind === "vision" && <option value="__off">Off</option>}
                    {current?.provider === provider && !models.some((m) => m.id === current.model) && (
                        <option value={current.model}>{current.model}</option>
                    )}
                    {models.map((m) => (
                        <option key={m.id} value={m.id}>
                            {m.id}
                        </option>
                    ))}
                </select>
                <button className="btn-secondary" onClick={refreshModels} disabled={refreshing} title="Reload the model list">
                    {refreshing ? "…" : "↻"}
                </button>
                {allowFreeText && (
                    <>
                        <input
                            className="ai-text-input ai-model-free"
                            placeholder="or type a model id"
                            value={custom}
                            onChange={(e) => setCustom(e.target.value)}
                            onKeyDown={(e) => e.key === "Enter" && custom.trim() && apply(custom.trim())}
                        />
                        <button className="btn-secondary" disabled={!custom.trim()} onClick={() => apply(custom.trim())}>
                            Set
                        </button>
                    </>
                )}
            </div>
        </div>
    );
}

// ---------------------------------------------------------------------------

interface AiAutomation {
    liveInsights: boolean;
    autoReport: boolean;
}

/** Off switches for the AI work that happens without a click. */
function AutomaticAi({ configured }: { configured: boolean }) {
    const [value, setValue] = useState<AiAutomation | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        invoke<AiAutomation>("get_ai_automation").then(setValue).catch((e) => setError(String(e)));
    }, []);

    const save = async (patch: Partial<AiAutomation>) => {
        if (!value) return;
        const prev = value;
        setValue({ ...value, ...patch });
        setError(null);
        try {
            setValue(await invoke<AiAutomation>("set_ai_automation", patch));
        } catch (e) {
            setValue(prev);
            setError(String(e));
        }
    };

    const row = (label: string, sub: string, on: boolean, patch: (v: boolean) => Partial<AiAutomation>) => (
        <div className="settings-row">
            <div className="settings-label">
                <span className="label-main">{label}</span>
                <span className="label-sub">{sub}</span>
            </div>
            <div
                className={`toggle-switch ${on ? "active" : ""}`}
                onClick={() => save(patch(!on))}
                style={{ cursor: "pointer" }}
                role="switch"
                aria-checked={on}
                aria-label={label}
            >
                <div className="toggle-knob"></div>
            </div>
        </div>
    );

    return (
        <section className="settings-section">
            <h3>Automatic AI</h3>
            <p className="section-desc">
                What happens without you clicking anything.
                {!configured && " These take effect once an AI provider is connected."}
            </p>
            {value &&
                row(
                    "Live insights while recording",
                    "Spot action items, decisions and risks in the live transcript while you record (runs on this Mac).",
                    value.liveInsights,
                    (v) => ({ liveInsights: v }),
                )}
            {value &&
                row(
                    "Write notes after each recording",
                    "When a recording longer than 6 minutes stops, write AI notes with your AI provider. Off: generate notes yourself from Recordings → Notes.",
                    value.autoReport,
                    (v) => ({ autoReport: v }),
                )}
            {error && <p className="ai-error-text">{error}</p>}
        </section>
    );
}

// ---------------------------------------------------------------------------

/**
 * Connect a provider: preset cards (Apple on-device, ChatGPT, Anthropic, Grok,
 * Mistral, Custom) over the one custom endpoint form. A card only fills the
 * URL and model; the user pastes their own key and clicks Save. Nothing is
 * selected until clicked, and "Test connection" is the only request that can
 * happen before consent (one fixed word, no meeting content).
 */
function ConnectProvider({ providers, onChange }: { providers: AiProviderInfo[]; onChange: () => void }) {
    const custom = providers.find((p) => p.id === "custom");
    const apple = providers.find((p) => p.id === "apple");
    const [presets, setPresets] = useState<AiPreset[]>([]);
    const [card, setCard] = useState<string | null>(null);
    const [form, setForm] = useState<EndpointForm>(EMPTY_FORM);
    const [busy, setBusy] = useState<"save" | "test" | "apple" | null>(null);
    const [feedback, setFeedback] = useState<Feedback>(null);
    const [testResult, setTestResult] = useState<Feedback>(null);

    // Static table from the backend: the only place provider URLs exist.
    useEffect(() => {
        ai.listPresets().then(setPresets).catch((e) => console.error("AI presets unavailable:", e));
    }, []);

    // Reflect the saved connection (a fresh install highlights nothing).
    useEffect(() => {
        setCard(initialCard(providers, presets));
        setForm({ url: custom?.base_url ?? "", model: custom?.model ?? "", key: "" });
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [custom?.base_url, custom?.model, presets]);

    const selectedPreset = card && card !== CUSTOM_CARD && card !== APPLE_CARD ? presets.find((p) => p.id === card) ?? null : null;
    const formPreset = presetForUrl(presets, form.url);

    const choose = (id: string) => {
        setFeedback(null);
        setTestResult(null);
        setCard(id);
        if (id === APPLE_CARD) return;
        if (id === CUSTOM_CARD) {
            // Custom keeps what is saved; a blank form otherwise (no default remote URL)
            setForm(custom?.base_url && !presetForUrl(presets, custom.base_url) ? { url: custom.base_url, model: custom.model ?? "", key: "" } : applyCustom());
            return;
        }
        const preset = presets.find((p) => p.id === id);
        if (preset) setForm(applyPreset(preset));
    };

    const useApple = async () => {
        setBusy("apple");
        setFeedback(null);
        try {
            await ai.setActive("apple", null, "text");
            setFeedback({ ok: true, text: "Apple on-device is now used for text AI. Nothing leaves this Mac." });
            onChange();
        } catch (e) {
            setFeedback({ ok: false, text: friendlyAiError(e) });
        } finally {
            setBusy(null);
        }
    };

    const save = async () => {
        if (!custom) return;
        setBusy("save");
        setFeedback(null);
        setTestResult(null);
        const url = form.url.trim();
        try {
            // A changed URL deletes the old key and clears consent in the backend
            if (url !== (custom.base_url ?? "")) await ai.setEndpoint(custom.id, url);
            if (form.key.trim()) {
                await ai.saveKey(form.key, "custom", url);
                setForm((f) => ({ ...f, key: "" }));
            }
            await ai.setActive(custom.id, form.model.trim(), "text");
            setFeedback({ ok: true, text: `Saved. Requests will go to ${hostOf(url)}. No request was sent; use Test connection to check the key.` });
            onChange();
            const fresh = (await ai.listProviders()).find((x) => x.id === custom.id);
            if (fresh?.needs_consent && fresh.active_text) await requestConsent(custom.id);
        } catch (e) {
            setFeedback({ ok: false, text: friendlyAiError(e) });
        } finally {
            setBusy(null);
        }
    };

    const test = async () => {
        if (!custom) return;
        setBusy("test");
        setTestResult(null);
        try {
            const r = await ai.test(custom.id);
            setTestResult({ ok: r.ok, text: r.message });
        } catch (e) {
            setTestResult({ ok: false, text: friendlyAiError(e) });
        } finally {
            setBusy(null);
        }
    };

    const savedMatchesForm =
        !!custom?.base_url && form.url.trim() === custom.base_url && !!custom.model && form.model.trim() === custom.model;
    const canTest = !!custom?.configured && savedMatchesForm && !form.key.trim();

    const cards: { id: string; name: string; note: string; disabled?: boolean }[] = [
        {
            id: APPLE_CARD,
            name: "Apple on-device",
            note: apple?.configured ? "Runs on this Mac. Nothing leaves it." : "Needs macOS 26 with Apple Intelligence.",
            disabled: !apple?.configured,
        },
        ...presets.map((p) => ({ id: p.id, name: p.name, note: p.note })),
        { id: CUSTOM_CARD, name: "Custom endpoint", note: "Any OpenAI-compatible server, local or remote." },
    ];

    return (
        <section className="settings-section">
            <h3>Connect a provider</h3>
            <p className="section-desc">
                Pick a provider to fill in its endpoint and a model, then paste your own API key. Presets are
                only a shortcut: no provider is active until you save, and nothing is sent until you allow it.
            </p>
            <div className="ai-preset-grid" role="radiogroup" aria-label="AI provider">
                {cards.map((c) => (
                    <button
                        key={c.id}
                        type="button"
                        role="radio"
                        aria-checked={card === c.id}
                        className={`ai-preset-card${card === c.id ? " selected" : ""}`}
                        onClick={() => choose(c.id)}
                        disabled={c.disabled}
                    >
                        <span className="ai-preset-name">{c.name}</span>
                        <span className="ai-preset-note">{c.note}</span>
                    </button>
                ))}
            </div>
            {card === APPLE_CARD && (
                <div className="ai-preset-form">
                    <p className="ai-what-leaves">AI runs on Apple's on-device model. No key, no network, no consent needed.</p>
                    <div className="ai-model-controls">
                        <button className="btn-primary" onClick={useApple} disabled={busy !== null || apple?.active_text}>
                            {apple?.active_text ? "In use" : busy === "apple" ? "Switching…" : "Use Apple on-device"}
                        </button>
                    </div>
                    {feedback && <p className={feedback.ok ? "ai-ok-text" : "ai-error-text"}>{feedback.text}</p>}
                </div>
            )}
            {card && card !== APPLE_CARD && custom && (
                <div className="ai-preset-form">
                    <div className="ai-preset-fields">
                        <label className="ai-field">
                            <span>Base URL</span>
                            <input
                                className="ai-text-input ai-url-input"
                                placeholder="https://your-server/v1"
                                value={form.url}
                                onChange={(e) => setForm((f) => ({ ...f, url: e.target.value }))}
                                spellCheck={false}
                                aria-label="Endpoint base URL"
                            />
                        </label>
                        <label className="ai-field">
                            <span>Model</span>
                            <input
                                className="ai-text-input"
                                value={form.model}
                                onChange={(e) => setForm((f) => ({ ...f, model: e.target.value }))}
                                placeholder="Model id"
                                aria-label="Model id"
                                autoComplete="off"
                            />
                            {selectedPreset?.model_hint && <small className="ai-field-hint">Also: {selectedPreset.model_hint}</small>}
                        </label>
                        <label className="ai-field">
                            <span>API key</span>
                            <input
                                className="ai-text-input"
                                type="password"
                                placeholder={custom.last4 && savedMatchesForm ? `Saved ••••${custom.last4}` : selectedPreset ? "Paste your key" : "API key (optional)"}
                                value={form.key}
                                onChange={(e) => setForm((f) => ({ ...f, key: e.target.value }))}
                                autoComplete="off"
                                aria-label="API key"
                            />
                            {selectedPreset && (
                                <button type="button" className="ai-link-button" onClick={() => openUrl(selectedPreset.key_url).catch(() => undefined)}>
                                    Get a key from {hostOf(selectedPreset.key_url)}
                                </button>
                            )}
                        </label>
                    </div>
                    <p className="ai-what-leaves">{whatWillBeSent(form, formPreset)}</p>
                    <div className="ai-model-controls ai-preset-actions">
                        <button className="btn-primary" onClick={save} disabled={busy !== null || !form.url.trim() || !form.model.trim()}>
                            {busy === "save" ? "Saving…" : "Save connection"}
                        </button>
                        <button
                            className="btn-secondary"
                            onClick={test}
                            disabled={busy !== null || !canTest}
                            title={canTest ? 'Sends the word "Hi" with a one-token answer. No meeting content.' : "Save the connection first"}
                        >
                            {busy === "test" ? "Testing…" : "Test connection"}
                        </button>
                    </div>
                    {feedback && <p className={feedback.ok ? "ai-ok-text" : "ai-error-text"}>{feedback.text}</p>}
                    {testResult && <p className={testResult.ok ? "ai-ok-text" : "ai-error-text"}>{testResult.text}</p>}
                </div>
            )}
        </section>
    );
}

// ---------------------------------------------------------------------------

function Advanced() {
    const [open, setOpen] = useState(false);
    return (
        <section className="settings-section ai-advanced">
            <button className="ai-advanced-toggle" onClick={() => setOpen(!open)} aria-expanded={open}>
                {open ? "▾" : "▸"} Advanced: screen analysis
            </button>
            {open && (
                <>
                    <p className="section-desc">
                        Screenshot processing.
                        Not required for anything else in the app.
                    </p>
                    <KnowledgeBaseSettings />
                </>
            )}
        </section>
    );
}
