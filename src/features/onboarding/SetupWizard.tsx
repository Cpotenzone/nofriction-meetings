// noFriction Meetings - first-run setup assistant
//
// Welcome & consent → macOS permissions → on-device transcription model →
// AI provider (paste a key / Apple on-device / skip) → noFriction Pro (Mac
// App Store build only, informational) → done. Every step can be skipped,
// and the whole assistant can be run again from Settings → General.
// Each step shows the live state, so re-running never resets anything.

import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ai, friendlyAiError, notifyAiStatusChanged, type AiProviderInfo, type AiStatus } from "../../lib/ai";
import { PRIVACY_URL, useCapabilities } from "../../lib/build";
import { SETUP_COMPLETE_KEY } from "../../lib/navigation";
import { FREE_FEATURES, PRO_FEATURES } from "../../components/PaywallModal";
import { MicIcon, SparkleIcon, WarningIcon, GearIcon, CheckIcon } from "../../components/icons";
import "./SetupWizard.css";

interface SetupWizardProps {
    onComplete: () => void;
}

type StepId = "welcome" | "permissions" | "transcription" | "ai" | "pro" | "done";

const RECOMMENDED_MODEL = "large-v3-turbo-q5_0";
const LIGHT_MODEL = "base.en";

export function SetupWizard({ onComplete }: SetupWizardProps) {
    const caps = useCapabilities();
    const steps: StepId[] = useMemo(
        () => ["welcome", "permissions", "transcription", "ai", ...(caps?.pro_gating ? (["pro"] as StepId[]) : []), "done"],
        [caps?.pro_gating],
    );
    const [index, setIndex] = useState(0);
    const step = steps[Math.min(index, steps.length - 1)];

    const next = () => setIndex((i) => Math.min(i + 1, steps.length - 1));
    const back = () => setIndex((i) => Math.max(i - 1, 0));
    const finish = () => {
        try {
            localStorage.setItem(SETUP_COMPLETE_KEY, "true");
        } catch {
            /* storage unavailable: the assistant shows again next launch */
        }
        onComplete();
    };

    return (
        <div className="setup-wizard">
            <div className="setup-header">
                <h1>Welcome to noFriction Meetings</h1>
                <p className="setup-subtitle">A few quick steps. Skip anything and change it later in Settings.</p>
                <div className="setup-progress" aria-label={`Step ${index + 1} of ${steps.length}`}>
                    {steps.map((s, i) => (
                        <div key={s} className={`progress-dot ${i === index ? "active" : ""} ${i < index ? "complete" : ""}`} />
                    ))}
                </div>
            </div>

            <div className="setup-content">
                {step === "welcome" && <WelcomeStep />}
                {step === "permissions" && <PermissionsStep />}
                {step === "transcription" && <TranscriptionStep />}
                {step === "ai" && <AiStep appleAvailable={!!caps?.apple_intelligence} />}
                {step === "pro" && <ProStep />}
                {step === "done" && <DoneStep />}
            </div>

            <div className="setup-actions">
                {index > 0 && (
                    <button className="setup-btn secondary" onClick={back}>
                        Back
                    </button>
                )}
                <div className="spacer" />
                {step !== "done" && step !== "welcome" && (
                    <button className="setup-btn secondary" onClick={next}>
                        Skip for now
                    </button>
                )}
                {step === "done" ? (
                    <button className="setup-btn primary" onClick={finish}>
                        Start Using noFriction Meetings
                    </button>
                ) : (
                    <button className="setup-btn primary" onClick={next}>
                        {step === "welcome" ? "Get Started" : "Continue"}
                    </button>
                )}
            </div>
            {step !== "done" && (
                <button className="setup-skip-all" onClick={finish}>
                    Skip setup — I'll do it in Settings
                </button>
            )}
        </div>
    );
}

// ---------------------------------------------------------------------------

function WelcomeStep() {
    return (
        <div className="setup-step">
            <div className="step-icon">
                <MicIcon size={22} />
            </div>
            <h2>Record, transcribe and remember your meetings</h2>
            <p className="step-description">
                noFriction captures your microphone, the call audio and screenshots of your screen while you record,
                transcribes on this Mac, and keeps everything in a private library you can search, edit and delete.
            </p>
            <div className="setup-summary">
                <div className="summary-item">
                    <span className="summary-label">Where your data lives</span>
                    <span className="summary-value">On this Mac</span>
                </div>
                <div className="summary-item">
                    <span className="summary-label">Transcription</span>
                    <span className="summary-value">On-device (no account, no internet)</span>
                </div>
                <div className="summary-item">
                    <span className="summary-label">AI notes &amp; chat</span>
                    <span className="summary-value">Your own AI provider, only after you allow it</span>
                </div>
                <div className="summary-item">
                    <span className="summary-label">noFriction servers</span>
                    <span className="summary-value">None</span>
                </div>
            </div>
            <div className="warning-box consent">
                <WarningIcon size={16} />
                <span>
                    <strong>Recording consent is your responsibility.</strong> Many places require everyone in a
                    conversation to agree before it is recorded. Tell people when you record, and stop if anyone
                    objects. Nothing is captured until you press Start.
                </span>
            </div>
            <button className="get-key-link as-button" onClick={() => openUrl(PRIVACY_URL).catch(() => undefined)}>
                Privacy policy →
            </button>
        </div>
    );
}

// ---------------------------------------------------------------------------

interface PermissionStatus {
    screen_recording: boolean;
    microphone: boolean;
    accessibility: boolean;
    calendar: boolean;
}

function PermissionsStep() {
    const [perms, setPerms] = useState<PermissionStatus | null>(null);
    const [micStatus, setMicStatus] = useState("not_determined");
    const [calStatus, setCalStatus] = useState("not_determined");
    const [requesting, setRequesting] = useState<string | null>(null);
    const [askedScreen, setAskedScreen] = useState(false);

    const refresh = useCallback(async () => {
        try {
            const [p, m, c] = await Promise.all([
                invoke<PermissionStatus>("check_permissions"),
                invoke<string>("get_microphone_auth_status"),
                invoke<string>("get_calendar_access_status").catch(() => "unknown"),
            ]);
            setPerms(p);
            setMicStatus(m);
            setCalStatus(c);
        } catch (err) {
            console.warn("Permission check unavailable:", err);
        }
    }, []);

    // Poll while the user answers the macOS prompts so the dots turn green
    useEffect(() => {
        refresh();
        const t = setInterval(refresh, 1500);
        return () => clearInterval(t);
    }, [refresh]);

    const grant = async (kind: "microphone" | "screen_recording" | "calendar") => {
        setRequesting(kind);
        try {
            if (kind === "calendar") await invoke("request_calendar_access");
            else await invoke("request_permission", { permissionType: kind });
            if (kind === "screen_recording") setAskedScreen(true);
        } catch (err) {
            console.warn("Permission request failed:", err);
        } finally {
            setRequesting(null);
            refresh();
        }
    };
    const openPane = (pane: string) => invoke("open_system_settings", { pane }).catch(console.warn);

    const micDenied = micStatus === "denied" || micStatus === "restricted";
    const calDenied = calStatus === "denied" || calStatus === "restricted";
    const calOk = calStatus === "authorized" || !!perms?.calendar;

    return (
        <div className="setup-step">
            <div className="step-icon">
                <GearIcon size={22} />
            </div>
            <h2>macOS permissions</h2>
            <p className="step-description">
                macOS asks you once for each. The dots turn green as you allow them. You can change any of these later
                in System Settings → Privacy &amp; Security.
            </p>

            <div className="perm-list">
                <PermRow
                    ok={!!perms?.microphone}
                    name="Microphone"
                    hint={
                        perms?.microphone
                            ? "Allowed — your voice will be transcribed"
                            : micDenied
                              ? "Turned off earlier — allow it in System Settings, then come back"
                              : "Needed to transcribe what you say"
                    }
                >
                    {micDenied ? (
                        <button className="setup-btn secondary perm-btn" onClick={() => openPane("microphone")}>
                            Open Settings
                        </button>
                    ) : (
                        <button className="setup-btn primary perm-btn" onClick={() => grant("microphone")} disabled={requesting === "microphone"}>
                            {requesting === "microphone" ? "Asking…" : "Allow"}
                        </button>
                    )}
                </PermRow>

                <PermRow
                    ok={!!perms?.screen_recording}
                    name="Screen & System Audio Recording"
                    hint={
                        perms?.screen_recording
                            ? "Allowed — call audio and screenshots are captured"
                            : askedScreen
                              ? "Turn on noFriction Meetings in System Settings. macOS may ask you to quit and reopen the app."
                              : "Needed for the other side of Zoom/Meet/Teams calls and for screenshots"
                    }
                >
                    <div style={{ display: "flex", gap: 8 }}>
                        <button
                            className="setup-btn primary perm-btn"
                            onClick={() => grant("screen_recording")}
                            disabled={requesting === "screen_recording"}
                        >
                            {requesting === "screen_recording" ? "Asking…" : "Allow"}
                        </button>
                        <button className="setup-btn secondary perm-btn" onClick={() => openPane("screen_recording")}>
                            Settings
                        </button>
                    </div>
                </PermRow>

                <PermRow
                    ok={calOk}
                    optional
                    name="Calendar (optional)"
                    hint={
                        calOk
                            ? "Allowed — recordings get the meeting's title and attendees"
                            : calDenied
                              ? "Turned off — allow it in System Settings to name recordings automatically"
                              : "Names recordings after your calendar events and lists who attended"
                    }
                >
                    {calDenied ? (
                        <button className="setup-btn secondary perm-btn" onClick={() => openPane("calendar")}>
                            Open Settings
                        </button>
                    ) : (
                        <button className="setup-btn primary perm-btn" onClick={() => grant("calendar")} disabled={requesting === "calendar"}>
                            {requesting === "calendar" ? "Asking…" : "Allow"}
                        </button>
                    )}
                </PermRow>
            </div>

            <p className="setup-footnote">
                Notifications are requested when you start your first recording, so noFriction can tell you when a
                meeting seems to have ended.
            </p>

            {perms && !perms.microphone && (
                <div className="warning-box">
                    <WarningIcon size={16} />
                    <span>Without microphone access you'd get screenshots but no transcript.</span>
                </div>
            )}
        </div>
    );
}

function PermRow({
    ok,
    name,
    hint,
    optional = false,
    children,
}: {
    ok: boolean;
    name: string;
    hint: string;
    optional?: boolean;
    children: ReactNode;
}) {
    return (
        <div className="perm-row">
            <span className={`perm-dot ${ok ? "ok" : optional ? "" : "pending"}`} />
            <div className="perm-info">
                <span className="perm-name">{name}</span>
                <span className="perm-hint">{hint}</span>
            </div>
            {!ok && children}
        </div>
    );
}

// ---------------------------------------------------------------------------

interface WhisperModel {
    name: string;
    size_mb: number;
    description: string;
    installed: boolean;
    active: boolean;
}

interface LocalSttStatus {
    ready: boolean;
    resolved_model: string | null;
    preferred_model: string;
    models: WhisperModel[];
}

interface DownloadProgress {
    model: string;
    downloaded_bytes: number;
    total_bytes: number | null;
    done: boolean;
    error: string | null;
}

function TranscriptionStep() {
    const [status, setStatus] = useState<LocalSttStatus | null>(null);
    const [provider, setProvider] = useState<string>("local");
    const [progress, setProgress] = useState<DownloadProgress | null>(null);
    const [downloading, setDownloading] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    const refresh = useCallback(async () => {
        try {
            setStatus(await invoke<LocalSttStatus>("get_local_stt_status"));
            const s = await invoke<{ transcription_provider?: string }>("get_settings");
            setProvider(s.transcription_provider || "local");
        } catch (e) {
            setError(String(e));
        }
    }, []);

    useEffect(() => {
        refresh();
        let off: (() => void) | undefined;
        let disposed = false;
        listen<DownloadProgress>("whisper_download_progress", (e) => {
            setProgress(e.payload);
            if (e.payload.done) {
                if (e.payload.error) setError(e.payload.error);
                refresh();
            }
        }).then((u) => (disposed ? u() : (off = u)));
        return () => {
            disposed = true;
            off?.();
        };
    }, [refresh]);

    const install = async (model: string) => {
        setError(null);
        setDownloading(model);
        setProgress({ model, downloaded_bytes: 0, total_bytes: null, done: false, error: null });
        try {
            // On-device Whisper is the transcription engine from now on
            await invoke("set_active_provider", { provider: "local" });
            await invoke("set_local_whisper_model", { model });
            setProvider("local");
            await invoke("download_whisper_model", { model });
        } catch (e) {
            setError(`Download failed: ${String(e)}. Check your internet connection and try again.`);
        } finally {
            setDownloading(null);
            refresh();
        }
    };

    const recommended = status?.models.find((m) => m.name === RECOMMENDED_MODEL);
    const light = status?.models.find((m) => m.name === LIGHT_MODEL);
    const installed = status?.models.filter((m) => m.installed) ?? [];
    const pct =
        progress && progress.total_bytes ? Math.min(100, Math.round((progress.downloaded_bytes / progress.total_bytes) * 100)) : null;
    const mb = (b: number) => `${Math.round(b / 1_048_576)} MB`;

    return (
        <div className="setup-step">
            <div className="step-icon">
                <MicIcon size={22} />
            </div>
            <h2>On-device transcription</h2>
            <p className="step-description">
                Speech is turned into text by Whisper running on this Mac: no account, no API key, and your audio
                never leaves the device. It needs a one-time model download.
            </p>

            {status?.ready ? (
                <div className="setup-ready">
                    <CheckIcon size={16} />
                    <span>
                        Ready — using {status.resolved_model?.replace(/^ggml-|\.bin$/g, "") ?? "an installed model"}
                        {provider !== "local" ? ` (current engine: ${provider}; switch in Settings → Transcription)` : ""}
                    </span>
                </div>
            ) : (
                <div className="mode-choice">
                    {recommended && (
                        <div className="mode-card selected">
                            <div style={{ flex: 1 }}>
                                <span className="mode-title">Recommended model · {recommended.size_mb} MB</span>
                                <span className="mode-hint">Best accuracy at real-time speed on Apple Silicon.</span>
                            </div>
                            <button className="setup-btn primary perm-btn" disabled={!!downloading} onClick={() => install(recommended.name)}>
                                {downloading === recommended.name ? "Downloading…" : "Download"}
                            </button>
                        </div>
                    )}
                    {light && (
                        <div className="mode-card">
                            <div style={{ flex: 1 }}>
                                <span className="mode-title">Smaller model · {light.size_mb} MB</span>
                                <span className="mode-hint">For older or Intel Macs, or a slow connection. Less accurate.</span>
                            </div>
                            <button className="setup-btn secondary perm-btn" disabled={!!downloading} onClick={() => install(light.name)}>
                                {downloading === light.name ? "Downloading…" : "Download"}
                            </button>
                        </div>
                    )}
                </div>
            )}

            {progress && !progress.done && (
                <div className="setup-download" aria-live="polite">
                    <div className="setup-download-bar">
                        <div style={{ width: `${pct ?? 5}%` }} />
                    </div>
                    <span>
                        {mb(progress.downloaded_bytes)}
                        {progress.total_bytes ? ` of ${mb(progress.total_bytes)} (${pct}%)` : ""} — you can continue
                        while it downloads.
                    </span>
                </div>
            )}

            {error && <div className="setup-error">{error}</div>}

            <p className="setup-footnote">
                {installed.length > 0 && !status?.ready ? `Installed: ${installed.map((m) => m.name).join(", ")}. ` : ""}
                Prefer a cloud service (Deepgram, Gemini, Gladia, Google)? Add your key in Settings → Transcription; audio
                is then sent to that service.
            </p>
        </div>
    );
}

// ---------------------------------------------------------------------------

function AiStep({ appleAvailable }: { appleAvailable: boolean }) {
    const [providers, setProviders] = useState<AiProviderInfo[]>([]);
    const [status, setStatus] = useState<AiStatus | null>(null);
    const [key, setKey] = useState("");
    const [busy, setBusy] = useState(false);
    const [feedback, setFeedback] = useState<{ ok: boolean; text: string } | null>(null);
    const [consentFor, setConsentFor] = useState<{ id: string; name: string } | null>(null);
    const inputRef = useRef<HTMLInputElement>(null);

    const refresh = useCallback(async () => {
        try {
            const [p, s] = await Promise.all([ai.listProviders(), ai.status()]);
            setProviders(p);
            setStatus(s);
            notifyAiStatusChanged();
        } catch (e) {
            console.warn("AI status unavailable:", e);
        }
    }, []);

    useEffect(() => {
        refresh();
    }, [refresh]);

    const openai = providers.find((p) => p.id === "openai");
    const apple = providers.find((p) => p.id === "apple");
    const text = status?.text;

    const connect = async () => {
        if (!key.trim()) return;
        setBusy(true);
        setFeedback(null);
        try {
            // Auto-detects the provider from the key (OpenAI, Anthropic, Gemini, …)
            const r = await ai.saveKey(key.trim(), null);
            setKey("");
            setFeedback({ ok: true, text: `Connected to ${r.name}${r.model ? ` · using ${r.model}` : ""}` });
            if (r.needs_consent) setConsentFor({ id: r.provider, name: r.name });
            await refresh();
        } catch (e) {
            const msg = friendlyAiError(e);
            setFeedback({
                ok: false,
                text: /UNKNOWN_PROVIDER|Unknown provider/i.test(String(e))
                    ? "We couldn't tell which service this key is for. Add it in Settings → AI Engine, where you can pick the provider."
                    : msg,
            });
        } finally {
            setBusy(false);
        }
    };

    const allow = async () => {
        if (!consentFor) return;
        try {
            await ai.grantConsent(consentFor.id);
            setConsentFor(null);
            await refresh();
        } catch (e) {
            setFeedback({ ok: false, text: friendlyAiError(e) });
        }
    };

    const useApple = async () => {
        setFeedback(null);
        try {
            await ai.setActive("apple", apple?.model ?? null, "text");
            setFeedback({ ok: true, text: "Using Apple's on-device model. Meeting content never leaves this Mac." });
            await refresh();
        } catch (e) {
            setFeedback({ ok: false, text: friendlyAiError(e) });
        }
    };

    const paste = async () => {
        try {
            const t = await navigator.clipboard.readText();
            if (t) setKey(t.trim());
            else inputRef.current?.focus();
        } catch {
            inputRef.current?.focus();
        }
    };

    return (
        <div className="setup-step">
            <div className="step-icon">
                <SparkleIcon size={22} />
            </div>
            <h2>AI notes, summaries and chat</h2>
            <p className="step-description">
                Bring your own AI key — OpenAI is the default, and Anthropic, Gemini, Groq, Mistral and others work
                too. noFriction talks to the provider directly from this Mac with your key; we never see it. Recording
                and transcription work without AI.
            </p>

            {text && (
                <div className="setup-ready">
                    <CheckIcon size={16} />
                    <span>
                        AI is set up: {text.name} · {text.model}
                        {!text.local && !text.consent ? " (asks before sending anything)" : ""}
                    </span>
                </div>
            )}

            <div className="api-key-section">
                <label htmlFor="ai-key">{text ? "Use a different key" : "Paste your API key"}</label>
                <div style={{ display: "flex", gap: 8 }}>
                    <input
                        id="ai-key"
                        ref={inputRef}
                        type="password"
                        className="setup-input"
                        placeholder="sk-… (OpenAI) · sk-ant-… · AIza… · gsk_…"
                        value={key}
                        onChange={(e) => setKey(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && connect()}
                        autoComplete="off"
                        spellCheck={false}
                    />
                    <button className="setup-btn secondary perm-btn" onClick={paste}>
                        Paste
                    </button>
                    <button className="setup-btn primary perm-btn" onClick={connect} disabled={busy || !key.trim()}>
                        {busy ? "Checking…" : "Connect"}
                    </button>
                </div>
                {openai?.key_url && (
                    <button className="get-key-link as-button" onClick={() => openUrl(openai.key_url).catch(() => undefined)}>
                        Get an OpenAI API key →
                    </button>
                )}
                <p className="key-hint">Saved in your macOS Keychain. Usage is billed by your provider.</p>
            </div>

            {consentFor && (
                <div className="setup-consent" role="dialog" aria-label={`Send meeting content to ${consentFor.name}?`}>
                    <strong>Send meeting content to {consentFor.name}?</strong>
                    <p>
                        To write notes, summaries and emails, noFriction sends the transcript, the meeting title,
                        attendee names and (for screen features) screenshots to {consentFor.name} using your API key.{" "}
                        {consentFor.name}'s privacy policy and terms apply. Nothing is sent to noFriction; we have no
                        servers.
                    </p>
                    <div style={{ display: "flex", gap: 8 }}>
                        <button className="setup-btn secondary perm-btn" onClick={() => setConsentFor(null)}>
                            Not now
                        </button>
                        <button className="setup-btn primary perm-btn" onClick={allow}>
                            Allow
                        </button>
                    </div>
                </div>
            )}

            {appleAvailable && text?.provider !== "apple" && (
                <div className="mode-card">
                    <div style={{ flex: 1 }}>
                        <span className="mode-title">Use Apple on-device (no key)</span>
                        <span className="mode-hint">
                            Apple Intelligence runs on this Mac. Free and private; shorter, simpler notes than cloud models.
                        </span>
                    </div>
                    <button className="setup-btn secondary perm-btn" onClick={useApple}>
                        Use Apple
                    </button>
                </div>
            )}

            {feedback && <p className={feedback.ok ? "setup-ok" : "setup-error"}>{feedback.text}</p>}

            <p className="setup-footnote">
                Running Ollama or LM Studio? Connect it in Settings → AI Engine → Local &amp; custom servers.
            </p>
        </div>
    );
}

// ---------------------------------------------------------------------------

function ProStep() {
    return (
        <div className="setup-step">
            <div className="step-icon">
                <SparkleIcon size={22} />
            </div>
            <h2>noFriction Pro</h2>
            <p className="step-description">{FREE_FEATURES}</p>
            <div className="setup-summary">
                {PRO_FEATURES.map((f) => (
                    <div className="summary-item" key={f}>
                        <span className="summary-label">{f}</span>
                        <span className="summary-value">Pro</span>
                    </div>
                ))}
            </div>
            <p className="setup-footnote">
                Nothing to buy now. When you first use an AI feature you'll see the plans, or open Settings →
                Subscription any time. Pro features use your own AI key (or Apple's on-device model).
            </p>
        </div>
    );
}

// ---------------------------------------------------------------------------

function DoneStep() {
    return (
        <div className="setup-step">
            <div className="step-icon">
                <SparkleIcon size={22} />
            </div>
            <h2>You're all set</h2>
            <p className="step-description">
                Click START CAPTURE when your meeting begins. Recording stops by itself when the meeting ends (you get a
                30-second heads-up), and the recording lands in REWIND with its transcript, screenshots and notes.
            </p>
            <div className="quick-start">
                <h3>Shortcuts</h3>
                <ul>
                    <li>
                        <kbd>⌘N</kbd> start recording · <kbd>⌘.</kbd> stop
                    </li>
                    <li>
                        <kbd>⌘K</kbd> command palette · <kbd>⌘,</kbd> settings
                    </li>
                    <li>
                        <kbd>⌘2</kbd> your recordings · <kbd>⇧⌘I</kbd> chat with your meetings
                    </li>
                </ul>
            </div>
            <p className="setup-footnote">Run this assistant again from Settings → General.</p>
        </div>
    );
}

// ---------------------------------------------------------------------------

/**
 * Whether the setup assistant should show. `null` while unknown. Returns a
 * setter so the app can dismiss it without reloading.
 */
export function useSetupRequired(): [boolean | null, (required: boolean) => void] {
    const [isRequired, setIsRequired] = useState<boolean | null>(null);

    useEffect(() => {
        let done = false;
        try {
            done = localStorage.getItem(SETUP_COMPLETE_KEY) === "true";
        } catch {
            done = false;
        }
        setIsRequired(!done);
    }, []);

    return [isRequired, setIsRequired];
}

export default SetupWizard;
