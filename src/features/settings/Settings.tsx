// Settings: Recording, Transcription, AI, Subscription (Mac App Store
// build), About. Decisions the app can make for the user are not here:
// the silence that ends a recording is 3 minutes, what is captured follows
// the permissions, notes are made automatically once AI is set up.

import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import * as tauri from "../../lib/tauri";
import type { AudioDevice } from "../../lib/tauri";
import { AIProviderSettings } from "./AIProviderSettings";
import { TranscriptionSettings } from "./TranscriptionSettings";
import { SubscriptionSettings } from "./SubscriptionSettings";
import { useCapabilities, PRIVACY_URL, SUPPORT_EMAIL, SUPPORT_URL, TERMS_URL } from "../../lib/build";
import { openHelp, type SettingsCategory } from "../../lib/navigation";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useAppVersion } from "../../hooks/useAppVersion";
import { CheckIcon } from "../../components/icons";

interface SettingsProps {
    initialCategory?: SettingsCategory;
}

const openLink = (url: string) => openUrl(url).catch((e) => console.error("Could not open link:", e));

/** Minutes of silence after which a recording is considered over (no setting). */
export const SILENCE_MINUTES = 3;

export function Settings({ initialCategory = "recording" }: SettingsProps) {
    const version = useAppVersion();
    const caps = useCapabilities();
    const [activeCategory, setActiveCategory] = useState<SettingsCategory>(initialCategory);
    const [toast, setToast] = useState<{ text: string; error: boolean } | null>(null);

    const showToast = useCallback((text: string, error = false) => {
        setToast({ text, error });
        setTimeout(() => setToast(null), 2200);
    }, []);

    const categories: { id: SettingsCategory; label: string }[] = [
        { id: "recording", label: "Recording" },
        { id: "transcription", label: "Transcription" },
        { id: "ai", label: "AI" },
        ...(caps?.storekit ? [{ id: "subscription" as const, label: "Subscription" }] : []),
        { id: "about", label: "About" },
    ];

    const shown = categories.some((c) => c.id === activeCategory) ? activeCategory : "recording";

    return (
        <div className="settings">
            <nav className="settings__nav" aria-label="Settings sections">
                <h2>Settings</h2>
                {categories.map((cat) => (
                    <button
                        key={cat.id}
                        type="button"
                        className={`settings__nav-item ${shown === cat.id ? "is-on" : ""}`}
                        onClick={() => setActiveCategory(cat.id)}
                    >
                        {cat.label}
                    </button>
                ))}
                <div className="settings__version">
                    v{version}
                    {caps?.build ? ` (${caps.build})` : ""}
                </div>
            </nav>
            <div className="settings__body">
                <h1>{categories.find((c) => c.id === shown)?.label}</h1>
                {shown === "recording" && <RecordingSettings showToast={showToast} />}
                {shown === "transcription" && <TranscriptionSettings />}
                {shown === "ai" && <AIProviderSettings />}
                {shown === "subscription" && <SubscriptionSettings />}
                {shown === "about" && <About />}
            </div>
            {toast && <div className={`settings-toast ${toast.error ? "is-error" : ""}`} role="status">{toast.text}</div>}
        </div>
    );
}

// ── Recording ────────────────────────────────────────────────────────────

function RecordingSettings({ showToast }: { showToast: (text: string, error?: boolean) => void }) {
    const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
    const [selectedMic, setSelectedMic] = useState<string>("");
    const [isLoadingDevices, setIsLoadingDevices] = useState(true);
    // Stop when it's over (meeting-end detection)
    const [autoStop, setAutoStop] = useState(true);
    // Export to Obsidian: a folder and an auto-export switch
    const [vaultPath, setVaultPath] = useState<string>("");
    const [autoExport, setAutoExport] = useState(false);

    useEffect(() => {
        (async () => {
            setIsLoadingDevices(true);
            try {
                const [devices, savedSettings] = await Promise.all([tauri.getAudioDevices(), tauri.getSavedSettings()]);
                setAudioDevices(devices);
                if (savedSettings.microphone) setSelectedMic(savedSettings.microphone);
                else if (devices.length > 0) setSelectedMic((devices.find((d) => d.is_default) || devices[0]).id);
            } catch (err) {
                console.error("Failed to load settings:", err);
            } finally {
                setIsLoadingDevices(false);
            }
            try {
                const status = await tauri.getVaultStatus();
                if (status.path) setVaultPath(status.path);
            } catch { /* no folder yet */ }
            try {
                const s = await tauri.getAutoStopSettings();
                setAutoStop(s.enabled);
            } catch { /* default on */ }
            try {
                const v = await invoke<string | null>("get_setting", { key: "obsidian_auto_export" });
                setAutoExport(v === "true");
            } catch { /* default off */ }
        })();
    }, []);

    // The tray's Pause/Stop don't change this, but another window might
    useEffect(() => {
        let off: (() => void) | undefined;
        let disposed = false;
        import("@tauri-apps/api/event").then(({ listen }) =>
            listen<tauri.AutoStopSettings>("auto-stop-settings-changed", (e) => setAutoStop(e.payload.enabled)).then((u) => {
                if (disposed) u();
                else off = u;
            }),
        );
        return () => {
            disposed = true;
            off?.();
        };
    }, []);

    const handleSelectMic = async (deviceId: string) => {
        setSelectedMic(deviceId);
        try {
            await tauri.setAudioDevice(deviceId);
            showToast("Microphone saved");
        } catch (err) {
            showToast(`Couldn't save the microphone: ${err instanceof Error ? err.message : String(err)}`, true);
        }
    };

    const saveAutoStop = async (enabled: boolean) => {
        const prev = autoStop;
        setAutoStop(enabled);
        try {
            const s = await tauri.setAutoStopSettings(enabled, SILENCE_MINUTES);
            setAutoStop(s.enabled);
        } catch (err) {
            setAutoStop(prev);
            showToast(`Couldn't save: ${err instanceof Error ? err.message : String(err)}`, true);
        }
    };

    const chooseVault = async () => {
        try {
            const selected = await open({ directory: true, multiple: false, title: "Choose your Obsidian vault folder" });
            if (selected && typeof selected === "string") {
                await tauri.setVaultPath(selected);
                setVaultPath(selected);
                showToast("Folder saved");
            }
        } catch (err) {
            showToast(`Couldn't choose the folder: ${err instanceof Error ? err.message : String(err)}`, true);
        }
    };

    const toggleAutoExport = async () => {
        const next = !autoExport;
        setAutoExport(next);
        try {
            await invoke("set_setting", { key: "obsidian_auto_export", value: String(next) });
        } catch (err) {
            setAutoExport(!next);
            showToast(`Couldn't save: ${err instanceof Error ? err.message : String(err)}`, true);
        }
    };

    const exportJson = async () => {
        try {
            const path = await save({
                title: "Export everything as JSON",
                defaultPath: `nofriction-export-${new Date().toISOString().split("T")[0]}.json`,
                filters: [{ name: "JSON", extensions: ["json"] }],
            });
            if (!path) return;
            await invoke("export_data", { path });
            showToast("Exported");
        } catch (err) {
            showToast(`Couldn't export: ${err instanceof Error ? err.message : String(err)}`, true);
        }
    };

    return (
        <>
            <section className="settings-section">
                <h3>Microphone</h3>
                <p className="section-desc">The other people on a call are recorded through Screen & System Audio Recording, when it's allowed.</p>
                {isLoadingDevices ? (
                    <div className="loading-spinner" style={{ margin: "12px auto" }} />
                ) : (
                    <div className="device-list">
                        {audioDevices.filter((d) => d.is_input).map((device) => (
                            <div
                                key={device.id}
                                className={`device-item ${selectedMic === device.id ? "selected" : ""}`}
                                onClick={() => handleSelectMic(device.id)}
                                role="radio"
                                aria-checked={selectedMic === device.id}
                            >
                                <span className="device-name">{device.name}</span>
                                {device.is_default && <span className="device-tag">Default</span>}
                                {selectedMic === device.id && <span className="check-icon"><CheckIcon size={14} /></span>}
                            </div>
                        ))}
                        {audioDevices.filter((d) => d.is_input).length === 0 && (
                            <p className="section-desc">No microphone found.</p>
                        )}
                    </div>
                )}
            </section>

            <section className="settings-section">
                <h3>Stopping</h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Stop when it's over</span>
                        <span className="label-sub">
                            When the call ends, the calendar event is over, or no one has spoken for {SILENCE_MINUTES} minutes.
                            You get 30 seconds to keep recording.
                        </span>
                    </div>
                    <div
                        className={`toggle-switch ${autoStop ? "active" : ""}`}
                        onClick={() => saveAutoStop(!autoStop)}
                        role="switch"
                        aria-checked={autoStop}
                        tabIndex={0}
                        onKeyDown={(e) => (e.key === " " || e.key === "Enter") && saveAutoStop(!autoStop)}
                    >
                        <div className="toggle-knob" />
                    </div>
                </div>
            </section>

            <section className="settings-section">
                <h3>Export</h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Export to Obsidian</span>
                        <span className="label-sub">
                            {vaultPath ? `Each recording is saved as Markdown in ${vaultPath}` : "Choose your vault folder to save recordings as Markdown."}
                        </span>
                    </div>
                    <div className="settings-path">
                        <button className="btn-secondary" onClick={chooseVault}>{vaultPath ? "Change folder" : "Choose folder"}</button>
                        {vaultPath && (
                            <div
                                className={`toggle-switch ${autoExport ? "active" : ""}`}
                                onClick={toggleAutoExport}
                                role="switch"
                                aria-checked={autoExport}
                                aria-label="Export each recording when it stops"
                                title="Export each recording when it stops"
                                tabIndex={0}
                                onKeyDown={(e) => (e.key === " " || e.key === "Enter") && toggleAutoExport()}
                            >
                                <div className="toggle-knob" />
                            </div>
                        )}
                    </div>
                </div>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Export everything as JSON</span>
                        <span className="label-sub">Recordings, transcripts and notes in one file.</span>
                    </div>
                    <button className="btn-secondary" onClick={exportJson}>Export…</button>
                </div>
            </section>
        </>
    );
}

// ── About ────────────────────────────────────────────────────────────────

function About() {
    const version = useAppVersion();
    const caps = useCapabilities();
    return (
        <>
            <section className="settings-section">
                <h3>noFriction</h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Version</span>
                        <span className="label-sub">
                            {version || caps?.version || "…"}
                            {caps?.build ? ` (build ${caps.build})` : ""}
                            {caps ? ` · ${caps.flavor === "mas" ? "Mac App Store" : "Direct download"}` : ""}
                        </span>
                    </div>
                </div>
                <p className="section-desc" style={{ marginTop: 12 }}>
                    Recordings, transcripts and notes stay on this Mac. Transcription runs here. AI runs on Apple's on-device model or
                    goes straight to the endpoint you entered, with your own key. noFriction runs no servers and receives none of it.
                </p>
            </section>
            <section className="settings-section">
                <h3>Help</h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">User guide</span>
                        <span className="label-sub">How to record, find, edit and make notes. Also in the Help menu.</span>
                    </div>
                    <button className="btn-secondary" onClick={openHelp}>Open Help</button>
                </div>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Support</span>
                        <span className="label-sub">{SUPPORT_EMAIL} · {SUPPORT_URL}</span>
                    </div>
                    <div className="about-links">
                        <button className="btn-secondary" onClick={() => openLink(SUPPORT_URL)}>Support site</button>
                        <button
                            className="btn-secondary"
                            onClick={() => openLink(`mailto:${SUPPORT_EMAIL}?subject=${encodeURIComponent(`noFriction ${version}${caps?.build ? ` (${caps.build})` : ""}`)}`)}
                        >
                            Email
                        </button>
                    </div>
                </div>
            </section>
            <section className="settings-section">
                <h3>Legal</h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Privacy policy</span>
                        <span className="label-sub">{PRIVACY_URL}</span>
                    </div>
                    <button className="btn-secondary" onClick={() => openLink(PRIVACY_URL)}>Open</button>
                </div>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Terms of use</span>
                        <span className="label-sub">Apple's standard End User License Agreement</span>
                    </div>
                    <button className="btn-secondary" onClick={() => openLink(TERMS_URL)}>Open</button>
                </div>
            </section>
        </>
    );
}
