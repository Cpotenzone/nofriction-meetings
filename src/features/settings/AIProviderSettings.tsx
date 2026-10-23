// Settings → AI: Apple on-device, or one OpenAI-compatible endpoint you
// enter (a preset card only fills the URL and model). Keys go straight to
// the macOS Keychain; this screen only ever sees the last 4 characters.

import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
    ai,
    friendlyAiError,
    notifyAiStatusChanged,
    requestConsent,
    type AiConnectResult,
    type AiKind,
    type AiLocalServer,
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
    connectMessage,
    hostOf,
    initialCard,
    isSignInCancelled,
    keyPresets,
    presetForUrl,
    signInPresets,
    whatWillBeSent,
    type EndpointForm,
} from "../../lib/aiPresets";
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
            <ConnectProvider providers={providers} onChange={refresh} />
            <SavedProviders saved={saved} onChange={refresh} />
            <ModelPickers providers={providers} byId={byId} status={status} onChange={refresh} />
            <AutomaticNotes configured={!!status?.text} />
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
            <h3>Your AI</h3>
            <p className="section-desc">
                What noFriction uses for notes, review guides and chat. Connect one below; you can change it any
                time. Keys stay in your Keychain, tied to the endpoint.
            </p>
            <div className="ai-active-grid">
                <div className="ai-active-item">
                    <span className="ai-active-label">Notes, review guides and chat</span>
                    <span className="ai-active-value">
                        {text ? `${text.name} · ${text.model}` : "Not set up"}
                        {text && <StateBadge state={text.state} />}
                    </span>
                </div>
                <div className="ai-active-item">
                    <span className="ai-active-label">Screens (a model that reads images)</span>
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
            <h3>Saved connections</h3>
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
                            {[
                                p.last4 ? `Key ••••${p.last4}` : p.key === "required" ? "Key saved" : p.base_url,
                                p.model,
                                !p.local && !p.consent ? "needs permission" : null,
                            ]
                                .filter(Boolean)
                                .join(" · ")}
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
    // Apple on-device is text-only: it can't be the Screens model
    const visionUsable = usable.filter((p) => p.protocol !== "apple");
    if (usable.length === 0) return null;
    return (
        <section className="settings-section">
            <h3>Models</h3>
            <ModelPicker kind="text" label="Notes, review guides and chat" usable={usable} byId={byId} current={status?.text ?? null} onChange={onChange} />
            {visionUsable.length > 0 && (
                <ModelPicker kind="vision" label="Screens" usable={visionUsable} byId={byId} current={status?.vision ?? null} onChange={onChange} />
            )}
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
                        ? "Needs a model that can read images. Off: the text model is used when it can."
                        : "Used for every AI feature except screens."}
                </span>
                {error && <span className="ai-error-text">{error}</span>}
            </div>
            <div className="ai-model-controls">
                <select className="ai-select" value={provider} onChange={(e) => setProvider(e.target.value)} aria-label={`${label} connection`}>
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
    autoReport: boolean;
}

/** The one thing AI does without a click: notes after a recording. */
function AutomaticNotes({ configured }: { configured: boolean }) {
    const [value, setValue] = useState<AiAutomation | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        invoke<AiAutomation>("get_ai_automation").then(setValue).catch((e) => setError(String(e)));
    }, []);

    const save = async (autoReport: boolean) => {
        if (!value) return;
        const prev = value;
        setValue({ autoReport });
        setError(null);
        try {
            setValue(await invoke<AiAutomation>("set_ai_automation", { autoReport }));
        } catch (e) {
            setValue(prev);
            setError(String(e));
        }
    };

    const on = value?.autoReport ?? true;
    return (
        <section className="settings-section">
            <h3>Notes</h3>
            <div className="settings-row">
                <div className="settings-label">
                    <span className="label-main">Make notes automatically</span>
                    <span className="label-sub">
                        When a recording longer than 6 minutes stops, notes are made with your AI.
                        {!configured && " Starts once AI is set up."} Off: use Make notes on the recording.
                    </span>
                </div>
                <div
                    className={`toggle-switch ${on ? "active" : ""}`}
                    onClick={() => save(!on)}
                    role="switch"
                    aria-checked={on}
                    aria-label="Make notes automatically"
                    tabIndex={0}
                    onKeyDown={(e) => (e.key === " " || e.key === "Enter") && save(!on)}
                >
                    <div className="toggle-knob"></div>
                </div>
            </div>
            {error && <p className="ai-error-text">{error}</p>}
        </section>
    );
}

// ---------------------------------------------------------------------------

/**
 * Connect your AI, as few clicks as each service allows:
 * - **Sign in with OpenRouter**: the browser opens, the user signs in, the
 *   key comes back on its own (OAuth PKCE, `ai::connect`). Every major model.
 * - **Free, on this Mac**: Apple on-device, or **Find AI on this Mac** (Ollama,
 *   LM Studio…) and Use.
 * - **Use your own key** (ChatGPT, Claude, Muse, Grok, Mistral: services that
 *   only issue keys): Get a key opens its page; **Paste key and connect**
 *   reads the clipboard in the backend (the key never reaches this screen),
 *   saves, and sends the one-word test.
 * - **Custom endpoint**: the full form.
 * Nothing is selected until clicked; nothing is sent until a Connect click,
 * and meeting content only after the user allows it.
 */
function ConnectProvider({ providers, onChange }: { providers: AiProviderInfo[]; onChange: () => void }) {
    const custom = providers.find((p) => p.id === "custom");
    const apple = providers.find((p) => p.id === "apple");
    const [presets, setPresets] = useState<AiPreset[]>([]);
    const [card, setCard] = useState<string | null>(null);
    const [form, setForm] = useState<EndpointForm>(EMPTY_FORM);
    const [busy, setBusy] = useState<"save" | "test" | "apple" | "connect" | "paste" | "signin" | "local" | null>(null);
    const [feedback, setFeedback] = useState<Feedback>(null);
    const [testResult, setTestResult] = useState<Feedback>(null);
    const [local, setLocal] = useState<AiLocalServer[] | null>(null);
    const [localModel, setLocalModel] = useState<Record<string, string>>({});
    const [showMore, setShowMore] = useState(false);

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

    const quick = signInPresets(presets);
    const keyed = keyPresets(presets);
    const selectedPreset = card && card !== CUSTOM_CARD && card !== APPLE_CARD ? presets.find((p) => p.id === card) ?? null : null;
    const formPreset = presetForUrl(presets, form.url);

    const choose = (id: string) => {
        setFeedback(null);
        setTestResult(null);
        setShowMore(false);
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

    /** After any connect: refresh, then ask before meeting content goes to a public endpoint. */
    const finish = async (r: AiConnectResult, preset: AiPreset | null) => {
        setFeedback(connectMessage(r, preset));
        setForm((f) => ({ ...f, key: "" }));
        onChange();
        if (r.ok && r.needs_consent) await requestConsent("custom");
    };

    const run = async (what: NonNullable<typeof busy>, work: () => Promise<void>) => {
        setBusy(what);
        setFeedback(null);
        setTestResult(null);
        try {
            await work();
        } catch (e) {
            if (!isSignInCancelled(e)) setFeedback({ ok: false, text: friendlyAiError(e) });
        } finally {
            setBusy(null);
        }
    };

    const signIn = (p: AiPreset) =>
        run("signin", async () => {
            setCard(null);
            await finish(await ai.signIn(p.id), p);
        });

    const pasteAndConnect = (p: AiPreset) =>
        run("paste", async () => finish(await ai.connect(form.url || p.base_url, form.model || p.default_model, null, true), p));

    const connectTyped = () =>
        run("connect", async () => finish(await ai.connect(form.url, form.model, form.key.trim() || null, false), selectedPreset));

    const findLocal = () =>
        run("local", async () => {
            const found = await ai.findLocal();
            setLocal(found);
            setLocalModel(Object.fromEntries(found.map((s) => [s.base_url, s.models[0] ?? ""])));
        });

    const useLocal = (s: AiLocalServer) =>
        run("connect", async () => finish(await ai.connect(s.base_url, localModel[s.base_url] || s.models[0], null, false), null));

    const useApple = () =>
        run("apple", async () => {
            await ai.setActive("apple", null, "text");
            setFeedback({ ok: true, text: "Apple on-device is now used for text AI. Nothing leaves this Mac." });
            onChange();
        });

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
    const connectedTo = (p: AiPreset) => !!custom?.configured && custom.base_url === p.base_url;

    return (
        <section className="settings-section">
            <h3>Connect your AI</h3>
            <p className="section-desc">
                Pick one. Nothing is sent until you connect, and your recordings only after you allow it. Keys stay in
                your Keychain.
            </p>

            <div className="ai-quick">
                {quick.map((p) => (
                    <div className="ai-quick-card ai-quick-primary" key={p.id}>
                        <span className="ai-quick-kicker">Easiest</span>
                        <span className="ai-preset-name">{p.name}</span>
                        <span className="ai-preset-note">{p.note}</span>
                        <div className="ai-quick-actions">
                            {busy === "signin" ? (
                                <>
                                    <span className="ai-wait">Finish signing in in your browser…</span>
                                    <button className="ai-link-button" onClick={() => ai.signInCancel()}>
                                        Cancel
                                    </button>
                                </>
                            ) : (
                                <button className="btn-primary" onClick={() => signIn(p)} disabled={busy !== null}>
                                    {connectedTo(p) ? `Sign in to ${p.name.split(" (")[0]} again` : `Sign in with ${p.name.split(" (")[0]}`}
                                </button>
                            )}
                        </div>
                    </div>
                ))}
                <div className="ai-quick-card">
                    <span className="ai-quick-kicker">Free, private</span>
                    <span className="ai-preset-name">On this Mac</span>
                    <span className="ai-preset-note">
                        {apple?.configured ? "Apple's on-device model, or AI you run here (Ollama, LM Studio). Nothing leaves this Mac." : "AI you run here (Ollama, LM Studio). Nothing leaves this Mac."}
                    </span>
                    <div className="ai-quick-actions">
                        {apple?.configured && (
                            <button className="btn-secondary" onClick={useApple} disabled={busy !== null || apple.active_text}>
                                {apple.active_text ? "Apple on-device: in use" : busy === "apple" ? "Switching…" : "Use Apple on-device"}
                            </button>
                        )}
                        <button className="btn-secondary" onClick={findLocal} disabled={busy !== null}>
                            {busy === "local" ? "Looking…" : "Find AI on this Mac"}
                        </button>
                    </div>
                    {local && (
                        <div className="ai-local-list">
                            {local.length === 0 ? (
                                <p className="ai-preset-note">
                                    None running. Install Ollama or LM Studio, start it with a model, then look again.
                                </p>
                            ) : (
                                local.map((s) => (
                                    <div className="ai-local-row" key={s.base_url}>
                                        <span className="ai-local-name">{s.name}</span>
                                        <select
                                            className="ai-select"
                                            value={localModel[s.base_url] ?? s.models[0]}
                                            onChange={(e) => setLocalModel((m) => ({ ...m, [s.base_url]: e.target.value }))}
                                            aria-label={`${s.name} model`}
                                        >
                                            {s.models.map((m) => (
                                                <option key={m} value={m}>
                                                    {m}
                                                </option>
                                            ))}
                                        </select>
                                        <button className="btn-primary" onClick={() => useLocal(s)} disabled={busy !== null}>
                                            Use
                                        </button>
                                    </div>
                                ))
                            )}
                        </div>
                    )}
                </div>
            </div>

            <h4 className="ai-subhead">Or use your own key</h4>
            <div className="ai-preset-grid" role="radiogroup" aria-label="AI service">
                {[...keyed.map((p) => ({ id: p.id, name: p.name, note: p.note })), { id: CUSTOM_CARD, name: "Custom endpoint", note: "Any OpenAI-compatible server, local or remote." }].map((c) => (
                    <button
                        key={c.id}
                        type="button"
                        role="radio"
                        aria-checked={card === c.id}
                        className={`ai-preset-card${card === c.id ? " selected" : ""}`}
                        onClick={() => choose(c.id)}
                    >
                        <span className="ai-preset-name">{c.name}</span>
                        <span className="ai-preset-note">{c.note}</span>
                    </button>
                ))}
            </div>

            {selectedPreset && !selectedPreset.sign_in_url && (
                <div className="ai-preset-form">
                    <ol className="ai-steps">
                        <li>
                            <span className="ai-step-n">1</span>
                            <button className="btn-secondary" onClick={() => openUrl(selectedPreset.key_url).catch(() => undefined)}>
                                Get a key at {hostOf(selectedPreset.key_url)} ↗
                            </button>
                            <span className="ai-preset-note">Create a key there and copy it.</span>
                        </li>
                        <li>
                            <span className="ai-step-n">2</span>
                            <button className="btn-primary" onClick={() => pasteAndConnect(selectedPreset)} disabled={busy !== null}>
                                {busy === "paste" ? "Connecting…" : "Paste key and connect"}
                            </button>
                            <span className="ai-preset-note">Takes the key you copied and checks it. Nothing else is read.</span>
                        </li>
                    </ol>
                    <button className="ai-link-button" onClick={() => setShowMore((v) => !v)} aria-expanded={showMore}>
                        {showMore ? "Hide" : "Type the key or change the model"}
                    </button>
                    {showMore && (
                        <div className="ai-preset-fields">
                            <label className="ai-field">
                                <span>API key</span>
                                <input
                                    className="ai-text-input"
                                    type="password"
                                    placeholder={custom?.last4 && savedMatchesForm ? `Saved ••••${custom.last4}` : "Paste or type your key"}
                                    value={form.key}
                                    onChange={(e) => setForm((f) => ({ ...f, key: e.target.value }))}
                                    autoComplete="off"
                                    aria-label="API key"
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
                                {selectedPreset.model_hint && <small className="ai-field-hint">Also: {selectedPreset.model_hint}</small>}
                            </label>
                            <div className="ai-model-controls ai-preset-actions">
                                <button
                                    className="btn-primary"
                                    onClick={connectTyped}
                                    disabled={busy !== null || !form.model.trim() || (!form.key.trim() && !savedMatchesForm)}
                                >
                                    {busy === "connect" ? "Connecting…" : "Connect"}
                                </button>
                            </div>
                        </div>
                    )}
                    <p className="ai-what-leaves">{whatWillBeSent(form, formPreset)}</p>
                </div>
            )}

            {card === CUSTOM_CARD && custom && (
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
                        </label>
                        <label className="ai-field">
                            <span>API key</span>
                            <input
                                className="ai-text-input"
                                type="password"
                                placeholder={custom.last4 && savedMatchesForm ? `Saved ••••${custom.last4}` : "API key (optional)"}
                                value={form.key}
                                onChange={(e) => setForm((f) => ({ ...f, key: e.target.value }))}
                                autoComplete="off"
                                aria-label="API key"
                            />
                        </label>
                    </div>
                    <p className="ai-what-leaves">{whatWillBeSent(form, formPreset)}</p>
                    <div className="ai-model-controls ai-preset-actions">
                        <button className="btn-primary" onClick={connectTyped} disabled={busy !== null || !form.url.trim() || !form.model.trim()}>
                            {busy === "connect" ? "Connecting…" : "Connect"}
                        </button>
                        <button className="btn-secondary" onClick={save} disabled={busy !== null || !form.url.trim() || !form.model.trim()}>
                            {busy === "save" ? "Saving…" : "Save without testing"}
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
                    {testResult && <p className={testResult.ok ? "ai-ok-text" : "ai-error-text"}>{testResult.text}</p>}
                </div>
            )}

            {feedback && (
                <p className={feedback.ok ? "ai-ok-text" : "ai-error-text"} role="status">
                    {feedback.text}
                </p>
            )}
        </section>
    );
}
