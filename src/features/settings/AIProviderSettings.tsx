// noFriction Meetings - AI settings (bring your own key)
// Paste a key for any major provider (auto-detected), or point at a local
// Ollama / LM Studio / OpenAI-compatible server. Keys go straight to the
// macOS Keychain; this screen only ever sees the last 4 characters.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
    ai,
    aiErrorClass,
    friendlyAiError,
    requestConsent,
    type AiDetection,
    type AiKind,
    type AiProviderInfo,
    type AiStatus,
} from "../../lib/ai";
import { KnowledgeBaseSettings } from "./KnowledgeBaseSettings";
import "./AIProviderSettings.css";

type Feedback = { ok: boolean; text: string } | null;

const LOCAL_IDS = ["ollama", "lmstudio", "custom"];

export function AIProviderSettings() {
    const [providers, setProviders] = useState<AiProviderInfo[]>([]);
    const [status, setStatus] = useState<AiStatus | null>(null);
    const [loading, setLoading] = useState(true);

    const refresh = useCallback(async () => {
        try {
            const [p, s] = await Promise.all([ai.listProviders(), ai.status()]);
            setProviders(p);
            setStatus(s);
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
            <PasteKey providers={providers} onSaved={refresh} />
            <SavedProviders saved={saved} onChange={refresh} />
            <ModelPickers providers={providers} byId={byId} status={status} onChange={refresh} />
            <LocalEndpoints providers={providers.filter((p) => LOCAL_IDS.includes(p.id))} onChange={refresh} />
            <GetAKey providers={providers} />
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
                Bring your own API key. noFriction talks to the provider directly from this Mac; there is no
                noFriction server in between.
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

function PasteKey({ providers, onSaved }: { providers: AiProviderInfo[]; onSaved: () => void }) {
    const [key, setKey] = useState("");
    const [reveal, setReveal] = useState(false);
    const [detection, setDetection] = useState<AiDetection | null>(null);
    const [override, setOverride] = useState<string>("");
    const [busy, setBusy] = useState(false);
    const [feedback, setFeedback] = useState<Feedback>(null);
    const inputRef = useRef<HTMLInputElement>(null);
    const keyed = providers.filter((p) => p.key !== "none");

    // Live auto-detect (debounced)
    useEffect(() => {
        setFeedback(null);
        if (!key.trim()) {
            setDetection(null);
            return;
        }
        const t = setTimeout(() => {
            ai.detect(key).then(setDetection).catch(() => setDetection(null));
        }, 150);
        return () => clearTimeout(t);
    }, [key]);

    const chosen = override || detection?.provider || "";
    const chosenName = providers.find((p) => p.id === chosen)?.name;

    const paste = async () => {
        try {
            const text = await navigator.clipboard.readText();
            if (text) setKey(text.trim());
            else inputRef.current?.focus();
        } catch {
            inputRef.current?.focus();
            setFeedback({ ok: false, text: "Clipboard not available here. Click the field and press ⌘V." });
        }
    };

    const connect = async () => {
        if (!key.trim()) return;
        setBusy(true);
        setFeedback(null);
        try {
            const r = await ai.saveKey(key, override || null);
            setKey("");
            setOverride("");
            setDetection(null);
            setFeedback({
                ok: true,
                text: `✓ Connected to ${r.name} · ${r.models.length} models${r.model ? ` · using ${r.model}` : ""}`,
            });
            onSaved();
            if (r.needs_consent) {
                if (await requestConsent(r.provider)) onSaved();
            }
        } catch (e) {
            const cls = aiErrorClass(e);
            if (cls === "unknown_provider") {
                setFeedback({ ok: false, text: "We couldn't tell which service this key is for. Pick it from the list, then Connect." });
            } else {
                setFeedback({ ok: false, text: friendlyAiError(e) });
            }
        } finally {
            setBusy(false);
        }
    };

    const detectLabel = () => {
        if (!key.trim()) return null;
        if (override) return `Using ${chosenName}`;
        if (detection?.provider) {
            const alt = detection.alternatives.includes("deepseek") ? " (or DeepSeek; we'll check)" : "";
            return `Looks like ${article(detection.name!)} ${detection.name} key${alt}`;
        }
        return "Unrecognised key format. Pick the provider below.";
    };

    return (
        <section className="settings-section">
            <h3>Paste your API key</h3>
            <p className="section-desc">
                OpenAI, Anthropic, Gemini, xAI Grok, Groq, OpenRouter, Mistral, DeepSeek, Perplexity, Together. We
                detect the provider from the key and check it before saving it to your Keychain.
            </p>
            <div className="ai-key-row">
                <input
                    ref={inputRef}
                    className="ai-key-input"
                    type={reveal ? "text" : "password"}
                    placeholder="sk-… · sk-ant-… · AIza… · xai-… · gsk_…"
                    value={key}
                    onChange={(e) => setKey(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && connect()}
                    autoComplete="off"
                    spellCheck={false}
                    aria-label="API key"
                />
                <button className="btn-secondary" onClick={() => setReveal(!reveal)} disabled={!key}>
                    {reveal ? "Hide" : "Show"}
                </button>
                <button className="btn-secondary" onClick={paste}>
                    Paste
                </button>
                <button className="btn-primary" onClick={connect} disabled={busy || !key.trim()}>
                    {busy ? "Checking…" : "Connect"}
                </button>
            </div>
            <div className="ai-key-meta">
                <span className={`ai-detect ${detection?.provider || override ? "ok" : ""}`}>{detectLabel()}</span>
                {key.trim() && (
                    <select
                        className="ai-select ai-select-small"
                        value={override}
                        onChange={(e) => setOverride(e.target.value)}
                        aria-label="Provider"
                    >
                        <option value="">{detection?.provider ? "Auto-detect" : "Pick provider…"}</option>
                        {keyed.map((p) => (
                            <option key={p.id} value={p.id}>
                                {p.name}
                            </option>
                        ))}
                    </select>
                )}
            </div>
            {feedback && <p className={feedback.ok ? "ai-ok-text" : "ai-error-text"}>{feedback.text}</p>}
        </section>
    );
}

function article(name: string) {
    return /^[AEIOU]/i.test(name) ? "an" : "a";
}

// ---------------------------------------------------------------------------

function SavedProviders({ saved, onChange }: { saved: AiProviderInfo[]; onChange: () => void }) {
    const [results, setResults] = useState<Record<string, Feedback>>({});
    const [busy, setBusy] = useState<string | null>(null);

    const test = async (p: AiProviderInfo) => {
        setBusy(p.id);
        try {
            const r = await ai.test(p.id);
            setResults((x) => ({ ...x, [p.id]: { ok: r.ok, text: r.ok ? `✓ ${r.message}` : friendlyAiError(r.message) } }));
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

function LocalEndpoints({ providers, onChange }: { providers: AiProviderInfo[]; onChange: () => void }) {
    return (
        <section className="settings-section">
            <h3>Local & custom servers</h3>
            <p className="section-desc">
                Run models on this Mac or your own network: Ollama, LM Studio, vLLM, or any OpenAI-compatible
                server. Plain http:// is only allowed for localhost, *.local, LAN and Tailscale addresses.
            </p>
            {providers.map((p) => (
                <LocalEndpoint key={p.id} p={p} onChange={onChange} />
            ))}
        </section>
    );
}

function LocalEndpoint({ p, onChange }: { p: AiProviderInfo; onChange: () => void }) {
    const [url, setUrl] = useState(p.base_url ?? "");
    const [key, setKey] = useState("");
    const [busy, setBusy] = useState(false);
    const [feedback, setFeedback] = useState<Feedback>(null);

    useEffect(() => setUrl(p.base_url ?? ""), [p.base_url]);

    const connect = async () => {
        setBusy(true);
        setFeedback(null);
        try {
            if (url.trim() !== (p.base_url ?? "")) await ai.setEndpoint(p.id, url.trim());
            let count = 0;
            if (p.id === "custom" && key.trim()) {
                const r = await ai.saveKey(key, "custom");
                count = r.models.length;
                setKey("");
            } else {
                const models = await ai.listModels(p.id);
                count = models.length;
                // Keyless servers: connecting makes them usable; select if nothing active
                const status = await ai.status();
                if (!status.text) await ai.setActive(p.id, null, "text");
            }
            setFeedback({ ok: true, text: `✓ Connected · ${count} models` });
            onChange();
            const fresh = (await ai.listProviders()).find((x) => x.id === p.id);
            if (fresh?.needs_consent && fresh.active_text) await requestConsent(p.id);
        } catch (e) {
            setFeedback({ ok: false, text: friendlyAiError(e) });
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="settings-row ai-local-row">
            <div className="settings-label">
                <span className="label-main">
                    {p.name}
                    {p.configured && <span className="ai-badge ai-badge-ok">Connected</span>}
                </span>
                {feedback && <span className={feedback.ok ? "ai-ok-text" : "ai-error-text"}>{feedback.text}</span>}
            </div>
            <div className="ai-model-controls">
                <input
                    className="ai-text-input ai-url-input"
                    placeholder={p.id === "custom" ? "https://your-server/v1" : p.base_url ?? ""}
                    value={url}
                    onChange={(e) => setUrl(e.target.value)}
                    spellCheck={false}
                    aria-label={`${p.name} URL`}
                />
                {p.id === "custom" && (
                    <input
                        className="ai-text-input"
                        type="password"
                        placeholder={p.last4 ? `Key ••••${p.last4}` : "API key (optional)"}
                        value={key}
                        onChange={(e) => setKey(e.target.value)}
                        autoComplete="off"
                        aria-label="Custom server API key"
                    />
                )}
                <button className="btn-secondary" onClick={connect} disabled={busy || (p.id === "custom" && !url.trim())}>
                    {busy ? "Connecting…" : "Connect"}
                </button>
            </div>
        </div>
    );
}

// ---------------------------------------------------------------------------

function GetAKey({ providers }: { providers: AiProviderInfo[] }) {
    const cloud = providers.filter((p) => p.key_url && p.key === "required");
    const local = providers.filter((p) => p.key_url && p.key === "none");
    const open = (url: string) => openUrl(url).catch((e) => console.error("open failed", e));
    return (
        <section className="settings-section">
            <h3>Get a key</h3>
            <div className="ai-chips">
                {cloud.map((p) => (
                    <button key={p.id} className="ai-chip" onClick={() => open(p.key_url)}>
                        {p.name} ↗
                    </button>
                ))}
                {local.map((p) => (
                    <button key={p.id} className="ai-chip" onClick={() => open(p.key_url)}>
                        {p.name} ↗
                    </button>
                ))}
            </div>
        </section>
    );
}

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
