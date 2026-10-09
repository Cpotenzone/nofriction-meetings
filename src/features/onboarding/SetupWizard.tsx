// First run: one screen. Microphone, Screen & System Audio and Calendar
// with one Allow each, while the speech model downloads in the background
// (a progress line; "Smaller model" for older Macs). Continue lands on the
// Record screen. AI and noFriction Pro come up the first time they're
// needed (Make notes, Chat), never here.

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SETUP_COMPLETE_KEY } from "../../lib/navigation";
import { CheckIcon, WarningIcon } from "../../components/icons";
import "./SetupWizard.css";

interface SetupWizardProps {
    onComplete: () => void;
}

const RECOMMENDED_MODEL = "large-v3-turbo-q5_0";
const LIGHT_MODEL = "base.en";

export function SetupWizard({ onComplete }: SetupWizardProps) {
    const finish = () => {
        try {
            localStorage.setItem(SETUP_COMPLETE_KEY, "true");
        } catch {
            /* storage unavailable: the screen shows again next launch */
        }
        onComplete();
    };

    return (
        <div className="setup-wizard">
            <div className="setup-header">
                <h1>Before you record</h1>
                <p className="setup-subtitle">
                    Recording and transcription happen on this Mac. Nothing leaves it unless you want it to.
                </p>
            </div>

            <div className="setup-content">
                <PermissionsStep />
                <ModelDownload />
            </div>

            <div className="setup-actions">
                <div className="spacer" />
                <button className="setup-btn primary" onClick={finish}>
                    Continue
                </button>
            </div>
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
            <div className="perm-list">
                <PermRow
                    ok={!!perms?.microphone}
                    name="Microphone"
                    hint={
                        perms?.microphone
                            ? "Allowed"
                            : micDenied
                              ? "Turned off earlier. Allow it in System Settings, then come back."
                              : "To transcribe what you say"
                    }
                >
                    {micDenied ? (
                        <button className="setup-btn secondary perm-btn" onClick={() => openPane("microphone")}>
                            Open System Settings
                        </button>
                    ) : (
                        <button className="setup-btn primary perm-btn" onClick={() => grant("microphone")} disabled={requesting === "microphone"}>
                            {requesting === "microphone" ? "Asking…" : "Allow"}
                        </button>
                    )}
                </PermRow>

                <PermRow
                    ok={!!perms?.screen_recording}
                    name="Screen & System Audio"
                    hint={
                        perms?.screen_recording
                            ? "Allowed"
                            : askedScreen
                              ? "Turn on noFriction in System Settings. macOS may ask you to quit and reopen the app."
                              : "For the other people on a call, and the screens you capture"
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
                        {askedScreen && (
                            <button className="setup-btn secondary perm-btn" onClick={() => openPane("screen_recording")}>
                                System Settings
                            </button>
                        )}
                    </div>
                </PermRow>

                <PermRow
                    ok={calOk}
                    optional
                    name="Calendar"
                    hint={
                        calOk
                            ? "Allowed"
                            : calDenied
                              ? "Turned off. Allow it in System Settings to name recordings after your events."
                              : "Names recordings after your events and lists who was there. Optional."
                    }
                >
                    {calDenied ? (
                        <button className="setup-btn secondary perm-btn" onClick={() => openPane("calendar")}>
                            Open System Settings
                        </button>
                    ) : (
                        <button className="setup-btn primary perm-btn" onClick={() => grant("calendar")} disabled={requesting === "calendar"}>
                            {requesting === "calendar" ? "Asking…" : "Allow"}
                        </button>
                    )}
                </PermRow>
            </div>

            {perms && !perms.microphone && micDenied && (
                <div className="warning-box">
                    <WarningIcon size={16} />
                    <span>Without the microphone you'd get screens but no transcript.</span>
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

/** The speech model downloads on its own; one line says how it's going. */
function ModelDownload() {
    const [status, setStatus] = useState<LocalSttStatus | null>(null);
    const [progress, setProgress] = useState<DownloadProgress | null>(null);
    const [downloading, setDownloading] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [started, setStarted] = useState(false);

    const refresh = useCallback(async () => {
        try {
            setStatus(await invoke<LocalSttStatus>("get_local_stt_status"));
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

    const install = useCallback(async (model: string) => {
        setError(null);
        setDownloading(model);
        setProgress({ model, downloaded_bytes: 0, total_bytes: null, done: false, error: null });
        try {
            await invoke("set_active_provider", { provider: "local" });
            await invoke("set_local_whisper_model", { model });
            await invoke("download_whisper_model", { model });
        } catch (e) {
            setError(`The download failed: ${String(e)}. Check your internet connection; you can try again in Settings → Transcription.`);
        } finally {
            setDownloading(null);
            refresh();
        }
    }, [refresh]);

    // Start the recommended model on its own, once we know none is installed
    useEffect(() => {
        if (started || !status || status.ready) return;
        const recommended = status.models.find((m) => m.name === RECOMMENDED_MODEL);
        if (!recommended || recommended.installed) return;
        setStarted(true);
        void install(recommended.name);
    }, [status, started, install]);

    const light = status?.models.find((m) => m.name === LIGHT_MODEL);
    const pct =
        progress && progress.total_bytes ? Math.min(100, Math.round((progress.downloaded_bytes / progress.total_bytes) * 100)) : null;
    const mb = (b: number) => `${Math.round(b / 1_048_576)} MB`;

    if (!status) return null;

    return (
        <div className="setup-model" aria-live="polite">
            {status.ready ? (
                <p className="setup-model__line">
                    <CheckIcon size={14} /> Speech model ready. Transcription runs on this Mac.
                </p>
            ) : progress && !progress.done ? (
                <>
                    <div className="setup-download-bar">
                        <div style={{ width: `${pct ?? 5}%` }} />
                    </div>
                    <p className="setup-model__line">
                        Downloading the speech model{progress.total_bytes ? `: ${mb(progress.downloaded_bytes)} of ${mb(progress.total_bytes)}` : "…"}.
                        Transcription starts when it finishes. You can record now.
                        {light && !light.installed && downloading !== light.name && (
                            <>
                                {" "}
                                <button type="button" className="setup-link" onClick={() => install(light.name)}>
                                    Smaller model
                                </button>{" "}
                                for an older or Intel Mac.
                            </>
                        )}
                    </p>
                </>
            ) : (
                <p className="setup-model__line">
                    {error ?? "The speech model hasn't downloaded yet. Settings → Transcription offers it again."}
                </p>
            )}
        </div>
    );
}

// ---------------------------------------------------------------------------

/**
 * Whether the first-run screen should show. `null` while unknown. Returns a
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
