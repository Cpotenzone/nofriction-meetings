// noFriction Meetings - Full Settings Component
// Streamlined settings: General, Transcription, Obsidian, AI Engine, Data

import { useState, useEffect, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog"; // Import dialog plugin
import { invoke } from "@tauri-apps/api/core";
import * as tauri from "../../lib/tauri";
import type { AudioDevice } from "../../lib/tauri";
import { AIProviderSettings } from "./AIProviderSettings";
import { PermissionsStatus } from "./PermissionsStatus";
import { TranscriptionSettings } from "./TranscriptionSettings";
import { SubscriptionSettings } from "./SubscriptionSettings";
import { useCapabilities, PRIVACY_URL, SUPPORT_EMAIL, SUPPORT_URL, TERMS_URL } from "../../lib/build";
import { rerunSetupAssistant, type SettingsCategory } from "../../lib/navigation";
import { openUrl } from "@tauri-apps/plugin-opener";

import { useAppVersion } from '../../hooks/useAppVersion';

interface FullSettingsProps {
    onSave?: () => void;
    /** Category to open on (e.g. "ai" from an "Open AI Engine" button) */
    initialCategory?: SettingsCategory;
}

const openLink = (url: string) => openUrl(url).catch((e) => console.error("Could not open link:", e));

export function FullSettings({ onSave: _onSave, initialCategory = "general" }: FullSettingsProps) {
    const version = useAppVersion();
    const caps = useCapabilities();
    // Devices
    const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
    const [selectedMic, setSelectedMic] = useState<string>("");
    const [vaultPath, setVaultPath] = useState<string>("");
    const [vaultStatus, setVaultStatus] = useState<any>(null);
    const [autoExport, setAutoExport] = useState(false);
    // Meeting-end detection (auto-stop)
    const [autoStop, setAutoStop] = useState(true);
    const [silenceMinutes, setSilenceMinutes] = useState(3);
    // What gets captured during a recording
    const [captureSystemAudio, setCaptureSystemAudio] = useState(true);
    const [captureScreen, setCaptureScreen] = useState(true);

    // Loading & feedback
    const [isLoadingDevices, setIsLoadingDevices] = useState(true);
    const [saveToast, setSaveToast] = useState<string | null>(null);

    // Sidebar State
    const [activeCategory, setActiveCategory] = useState<SettingsCategory>(initialCategory);

    // Auto-dismiss save toast
    const showToast = useCallback((msg: string) => {
        setSaveToast(msg);
        setTimeout(() => setSaveToast(null), 2000);
    }, []);

    // Load current settings
    useEffect(() => {
        loadSettings();
    }, []);

    const loadSettings = async () => {
        setIsLoadingDevices(true);
        try {
            const [devices, savedSettings] = await Promise.all([
                tauri.getAudioDevices(),
                tauri.getSavedSettings(),
            ]);

            setAudioDevices(devices);

            // Set saved mic selection
            if (savedSettings.microphone) {
                setSelectedMic(savedSettings.microphone);
            } else if (devices.length > 0) {
                const defaultDevice = devices.find(d => d.is_default) || devices[0];
                setSelectedMic(defaultDevice.id);
            }

            // Load vault status
            const status = await tauri.getVaultStatus();
            setVaultStatus(status);
            if (status.path) {
                setVaultPath(status.path);
            }

            // Load auto-stop settings
            try {
                const s = await tauri.getAutoStopSettings();
                setAutoStop(s.enabled);
                setSilenceMinutes(s.silenceMinutes);
            } catch { /* defaults */ }

            // Capture sources (honored at the next recording start)
            try {
                const c = await invoke<{ capture_system_audio: boolean; capture_screen: boolean }>("get_capture_settings");
                setCaptureSystemAudio(c.capture_system_audio);
                setCaptureScreen(c.capture_screen);
            } catch { /* defaults */ }

            // Load auto-export setting
            try {
                const autoExpVal = await invoke<string | null>("get_setting", { key: "obsidian_auto_export" });
                setAutoExport(autoExpVal === "true");
            } catch { /* defaults to false */ }
        } catch (err) {
            console.error("Failed to load settings:", err);
        } finally {
            setIsLoadingDevices(false);
        }
    };

    const handleSelectMic = async (deviceId: string) => {
        setSelectedMic(deviceId);
        try {
            await tauri.setAudioDevice(deviceId);
            showToast("✅ Microphone saved");
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            console.error("❌ Failed to save microphone:", errorMsg);
            showToast(`❌ Failed to save: ${errorMsg}`);
        }
    };

    const handleSaveVaultPath = async () => {
        try {
            await tauri.setVaultPath(vaultPath);
            const status = await tauri.getVaultStatus();
            setVaultStatus(status);
            showToast("✅ Vault path saved");
        } catch (err) {
            const errorMsg = err instanceof Error ? err.message : String(err);
            showToast(`❌ Failed to save: ${errorMsg}`);
        }
    };

    const handleToggleAutoExport = async () => {
        const newVal = !autoExport;
        setAutoExport(newVal);
        try {
            await invoke("set_setting", { key: "obsidian_auto_export", value: String(newVal) });
            showToast(newVal ? "✅ Auto-export enabled" : "✅ Auto-export disabled");
        } catch (err) {
            setAutoExport(!newVal);
            const errorMsg = err instanceof Error ? err.message : String(err);
            showToast(`❌ Failed: ${errorMsg}`);
        }
    };

    const saveAutoStop = async (enabled: boolean, minutes: number) => {
        const prev = { enabled: autoStop, minutes: silenceMinutes };
        setAutoStop(enabled);
        setSilenceMinutes(minutes);
        try {
            const s = await tauri.setAutoStopSettings(enabled, minutes);
            setAutoStop(s.enabled);
            setSilenceMinutes(s.silenceMinutes);
            showToast("✅ Auto-stop saved");
        } catch (err) {
            setAutoStop(prev.enabled);
            setSilenceMinutes(prev.minutes);
            const errorMsg = err instanceof Error ? err.message : String(err);
            showToast(`❌ Failed: ${errorMsg}`);
        }
    };

    const saveCaptureSource = async (kind: "system_audio" | "screen", enabled: boolean) => {
        const set = kind === "system_audio" ? setCaptureSystemAudio : setCaptureScreen;
        set(enabled);
        try {
            await invoke(kind === "system_audio" ? "set_capture_system_audio" : "set_capture_screen", { enabled });
            showToast("✅ Saved — applies to your next recording");
        } catch (err) {
            set(!enabled);
            const errorMsg = err instanceof Error ? err.message : String(err);
            showToast(`❌ Failed: ${errorMsg}`);
        }
    };

    // Tray checkbox changes the same setting
    useEffect(() => {
        let off: (() => void) | undefined;
        let disposed = false;
        import("@tauri-apps/api/event").then(({ listen }) =>
            listen<tauri.AutoStopSettings>("auto-stop-settings-changed", (e) => {
                setAutoStop(e.payload.enabled);
                setSilenceMinutes(e.payload.silenceMinutes);
            }).then((u) => { if (disposed) u(); else off = u; })
        );
        return () => { disposed = true; off?.(); };
    }, []);

    const categories = [
        { id: "general", label: "General", icon: "⚙️" },
        { id: "transcription", label: "Transcription", icon: "🎙️" },
        { id: "obsidian", label: "Obsidian", icon: "📚" },
        { id: "ai", label: "AI Engine", icon: "✨" },
        // Mac App Store build: StoreKit subscription (noFriction Pro)
        ...(caps?.storekit ? [{ id: "subscription", label: "Subscription", icon: "⭐" }] : []),
        { id: "data", label: "Data", icon: "💾" },
        { id: "about", label: "About", icon: "ℹ️" },
    ];

    const renderSidebar = () => (
        <div className="settings-sidebar glass-panel">
            <div className="sidebar-header">
                <h2>Settings</h2>
            </div>
            <nav className="sidebar-nav">
                {categories.map(cat => (
                    <button
                        key={cat.id}
                        className={`sidebar-item ${activeCategory === cat.id ? "active" : ""}`}
                        onClick={() => setActiveCategory(cat.id as SettingsCategory)}
                    >
                        <span className="sidebar-icon">{cat.icon}</span>
                        <span className="sidebar-label">{cat.label}</span>
                    </button>
                ))}
            </nav>
            <div className="sidebar-footer">
                <div className="app-version">v{version}{caps?.build ? ` (${caps.build})` : ""}</div>
            </div>
        </div>
    );

    const handleSelectVault = async () => {
        try {
            const selected = await open({
                directory: true,
                multiple: false,
                title: "Select Obsidian Vault Folder",
            });
            if (selected && typeof selected === "string") {
                setVaultPath(selected);
                showToast("✅ Folder selected. Click Save to apply.");
            }
        } catch (err) {
            console.error("Failed to open dialog:", err);
            showToast("❌ Permission Error: Could not open folder picker.");
        }
    };

    const renderContent = () => {
        switch (activeCategory) {
            case "general":
                return (
                    <div className="settings-content-panel fade-in">
                        <section className="settings-section">
                            <h3>Microphone</h3>
                            <p className="section-desc">Select the microphone for voice capture.</p>
                            {isLoadingDevices ? (
                                <div className="loading-spinner" style={{ margin: "20px auto" }} />
                            ) : (
                                <div className="device-list">
                                    {audioDevices.filter(d => d.is_input).map((device) => (
                                        <div
                                            key={device.id}
                                            className={`device-item ${selectedMic === device.id ? "selected" : ""}`}
                                            onClick={() => handleSelectMic(device.id)}
                                        >
                                            <span className="device-icon">🎤</span>
                                            <div className="device-info">
                                                <span className="device-name">{device.name}</span>
                                                {device.is_default && <span className="device-tag">Default</span>}
                                            </div>
                                            {selectedMic === device.id && <span className="check-icon">✓</span>}
                                        </div>
                                    ))}
                                    {audioDevices.filter(d => d.is_input).length === 0 && (
                                        <p style={{ color: "var(--text-tertiary)" }}>No microphones detected</p>
                                    )}
                                </div>
                            )}
                        </section>

                        <section className="settings-section">
                            <h3>Recording</h3>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Stop automatically when the meeting ends</span>
                                    <span className="label-sub">
                                        When the call app releases the microphone, the meeting window closes,
                                        the calendar event is over, or no one speaks for a while. You get a
                                        30-second countdown with a Keep recording option first.
                                    </span>
                                </div>
                                <div
                                    className={`toggle-switch ${autoStop ? 'active' : ''}`}
                                    onClick={() => saveAutoStop(!autoStop, silenceMinutes)}
                                    style={{ cursor: 'pointer' }}
                                    role="switch"
                                    aria-checked={autoStop}
                                >
                                    <div className="toggle-knob"></div>
                                </div>
                            </div>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Silence before stopping</span>
                                    <span className="label-sub">Minutes with no speech before the recording is considered over.</span>
                                </div>
                                <input
                                    type="number"
                                    min={1}
                                    max={60}
                                    value={silenceMinutes}
                                    disabled={!autoStop}
                                    onChange={(e) => setSilenceMinutes(Number(e.target.value) || 1)}
                                    onBlur={() => saveAutoStop(autoStop, Math.min(60, Math.max(1, silenceMinutes)))}
                                    style={{ width: 72 }}
                                    aria-label="Silence minutes before auto-stop"
                                />
                            </div>
                        </section>

                        <section className="settings-section">
                            <h3>What to Capture</h3>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">System audio</span>
                                    <span className="label-sub">The other side of Zoom, Meet and Teams calls. Needs Screen &amp; System Audio Recording permission.</span>
                                </div>
                                <div
                                    className={`toggle-switch ${captureSystemAudio ? 'active' : ''}`}
                                    onClick={() => saveCaptureSource("system_audio", !captureSystemAudio)}
                                    style={{ cursor: 'pointer' }}
                                    role="switch"
                                    aria-checked={captureSystemAudio}
                                    aria-label="Capture system audio"
                                >
                                    <div className="toggle-knob"></div>
                                </div>
                            </div>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Screenshots</span>
                                    <span className="label-sub">About one per second while recording; unchanged screens are skipped.</span>
                                </div>
                                <div
                                    className={`toggle-switch ${captureScreen ? 'active' : ''}`}
                                    onClick={() => saveCaptureSource("screen", !captureScreen)}
                                    style={{ cursor: 'pointer' }}
                                    role="switch"
                                    aria-checked={captureScreen}
                                    aria-label="Capture screenshots"
                                >
                                    <div className="toggle-knob"></div>
                                </div>
                            </div>
                        </section>

                        <section className="settings-section">
                            <h3>Setup Assistant</h3>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Run setup assistant again</span>
                                    <span className="label-sub">Permissions, the on-device transcription model and your AI key. Nothing is reset.</span>
                                </div>
                                <button className="btn-secondary" onClick={rerunSetupAssistant}>Run Setup Assistant</button>
                            </div>
                        </section>
                    </div>
                );
            case "transcription":
                return <TranscriptionSettings />;
            case "obsidian":
                return (
                    <div className="settings-content-panel fade-in">
                        <section className="settings-section">
                            <h3>Obsidian Integration</h3>
                            <p className="section-desc">Connect noFriction to your Obsidian vault for meeting knowledge management.</p>

                            <div className="input-group">
                                <label>Vault Root Path</label>
                                <div className="input-with-button">
                                    <input
                                        type="text"
                                        value={vaultPath}
                                        onChange={(e) => setVaultPath(e.target.value)}
                                        placeholder="/Users/name/Documents/MyVault"
                                        readOnly // Make read-only to encourage using the picker
                                        style={{ cursor: "pointer" }}
                                        onClick={handleSelectVault}
                                    />
                                    <button className="btn-secondary" onClick={handleSelectVault} style={{ marginRight: "8px" }}>Select Folder</button>
                                    <button className="btn-primary" onClick={handleSaveVaultPath}>Save</button>
                                </div>
                                <p className="input-help">The absolute path to your Obsidian vault folder.</p>
                            </div>

                            {vaultStatus && vaultStatus.configured && (
                                <div className={`status-card ${vaultStatus.valid ? 'success' : 'error'}`}>
                                    <div className="status-header">
                                        <span className="status-icon">{vaultStatus.valid ? '✅' : '❌'}</span>
                                        <span className="status-text">{vaultStatus.valid ? 'Vault Connected' : 'Invalid Path'}</span>
                                    </div>
                                    {vaultStatus.valid && (
                                        <div className="status-details">
                                            <div className="detail-item">
                                                <span className="detail-label">Topics:</span>
                                                <span className="detail-value">{vaultStatus.topicCount}</span>
                                            </div>
                                            <div className="detail-item">
                                                <span className="detail-label">Total Files:</span>
                                                <span className="detail-value">{vaultStatus.totalFiles}</span>
                                            </div>
                                        </div>
                                    )}
                                </div>
                            )}
                        </section>

                        <section className="settings-section">
                            <h3>Auto-Export</h3>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Auto-Export Meetings</span>
                                    <span className="label-sub">Automatically save meetings to vault when capture stops.</span>
                                </div>
                                <div
                                    className={`toggle-switch ${autoExport ? 'active' : ''}`}
                                    onClick={handleToggleAutoExport}
                                    style={{ cursor: 'pointer' }}
                                >
                                    <div className="toggle-knob"></div>
                                </div>
                            </div>
                        </section>
                    </div>
                );
            case "subscription":
                return <SubscriptionSettings />;
            case "ai":
                return (
                    <div className="settings-content-panel fade-in">
                        <AIProviderSettings />
                    </div>
                );
            case "data":
                return (
                    <div className="settings-content-panel fade-in">
                        <PermissionsStatus />
                        {/* Video storage cleanup: ffmpeg recordings don't exist in the App Store build */}
                        {caps?.video_recording && <section className="settings-section">
                            <h3>Storage Management</h3>
                            <div className="storage-card">
                                <div className="storage-icon">💾</div>
                                <div className="storage-details">
                                    <span className="storage-title">Local Recordings</span>
                                    <span className="storage-subtitle">Manage disk usage and cleanup</span>
                                </div>
                                <button className="btn-danger-outline" onClick={async () => {
                                    const { invoke } = await import("@tauri-apps/api/core");
                                    await invoke("apply_retention");
                                    alert("Cleanup complete");
                                }}>Cleanup Now</button>
                            </div>
                        </section>}
                        <section className="settings-section">
                            <h3>Export & Reset</h3>
                            <div className="button-group">
                                <button className="btn-secondary" onClick={async () => {
                                    try {
                                        const { invoke } = await import("@tauri-apps/api/core");
                                        const data = await invoke<string>("export_data");
                                        const blob = new Blob([data], { type: "application/json" });
                                        const url = URL.createObjectURL(blob);
                                        const a = document.createElement("a");
                                        a.href = url;
                                        a.download = `nofriction-export-${new Date().toISOString().split("T")[0]}.json`;
                                        a.click();
                                        URL.revokeObjectURL(url);
                                    } catch (err) {
                                        console.error("Failed to export data:", err);
                                        alert("Failed to export data");
                                    }
                                }}>Export Data to JSON</button>
                                <button className="btn-danger" onClick={async () => {
                                    if (confirm("Clear all cache? This cannot be undone.")) {
                                        const { invoke } = await import("@tauri-apps/api/core");
                                        await invoke("clear_cache");
                                        alert("Cache cleared successfully");
                                    }
                                }}>Clear Cache</button>
                            </div>
                        </section>
                    </div>
                );
            case "about":
                return (
                    <div className="settings-content-panel fade-in">
                        <section className="settings-section">
                            <h3>noFriction Meetings</h3>
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
                            <p className="section-desc">
                                Recordings, transcripts and notes are stored on this Mac. Local and Apple on-device models
                                support offline AI after setup. Optional cloud AI sends meeting content directly to your
                                configured endpoint with your own optional key. Transcription remains on this Mac.
                                noFriction offers no hosted models and receives none of this content.
                            </p>
                        </section>
                        <section className="settings-section">
                            <h3>Legal &amp; Support</h3>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Privacy Policy</span>
                                    <span className="label-sub">{PRIVACY_URL}</span>
                                </div>
                                <button className="btn-secondary" onClick={() => openLink(PRIVACY_URL)}>Open</button>
                            </div>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Terms of Use</span>
                                    <span className="label-sub">Apple's standard End User License Agreement</span>
                                </div>
                                <button className="btn-secondary" onClick={() => openLink(TERMS_URL)}>Open</button>
                            </div>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Support</span>
                                    <span className="label-sub">{SUPPORT_EMAIL} · {SUPPORT_URL}</span>
                                </div>
                                <div style={{ display: "flex", gap: 8 }}>
                                    <button className="btn-secondary" onClick={() => openLink(SUPPORT_URL)}>Support Site</button>
                                    <button className="btn-secondary" onClick={() => openLink(`mailto:${SUPPORT_EMAIL}?subject=${encodeURIComponent(`noFriction Meetings ${version}${caps?.build ? ` (${caps.build})` : ""}`)}`)}>Email Support</button>
                                </div>
                            </div>
                        </section>
                    </div>
                );
            default:
                return null;
        }
    };

    return (
        <div className="full-settings-layout">
            {renderSidebar()}
            <div className="settings-main-content">
                <div className="content-header">
                    <h1>{categories.find(c => c.id === activeCategory)?.label}</h1>
                </div>
                <div className="content-scrollable">
                    {renderContent()}
                </div>
            </div>
            {/* Save confirmation toast */}
            {saveToast && (
                <div style={{
                    position: "fixed",
                    bottom: "24px",
                    right: "24px",
                    background: saveToast.startsWith("✅") ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)",
                    border: `1px solid ${saveToast.startsWith("✅") ? "rgba(34,197,94,0.4)" : "rgba(239,68,68,0.4)"}`,
                    color: saveToast.startsWith("✅") ? "#22c55e" : "#ef4444",
                    padding: "10px 20px",
                    borderRadius: "8px",
                    fontSize: "0.85rem",
                    fontWeight: 600,
                    backdropFilter: "blur(12px)",
                    zIndex: 9999,
                    animation: "fadeIn 0.2s ease-out",
                }}>
                    {saveToast}
                </div>
            )}
        </div>
    );
}
