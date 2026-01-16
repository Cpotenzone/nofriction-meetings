// noFriction Meetings - Full Settings Component
// Comprehensive settings with device selection, API config, and preferences

import { useState, useEffect } from "react";
import * as tauri from "../lib/tauri";
import type { AudioDevice, MonitorInfo } from "../lib/tauri";
import { AISettings } from "./AISettings";
import { KnowledgeBaseSettings } from "./KnowledgeBaseSettings";
import { PermissionsStatus } from "./PermissionsStatus";

interface FullSettingsProps {
    onSave?: () => void;
}

export function FullSettings({ onSave: _onSave }: FullSettingsProps) {
    // API Settings
    const [apiKey, setApiKey] = useState("");
    const [apiKeyValid, setApiKeyValid] = useState<boolean | null>(null);
    const [savedApiKey, setSavedApiKey] = useState("");

    // Devices
    const [audioDevices, setAudioDevices] = useState<AudioDevice[]>([]);
    const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
    const [selectedMic, setSelectedMic] = useState<string>("");
    const [selectedMonitor, setSelectedMonitor] = useState<number | null>(null);

    // Loading states
    const [isLoadingDevices, setIsLoadingDevices] = useState(true);
    const [isSavingApiKey, setIsSavingApiKey] = useState(false);

    // Load current settings
    useEffect(() => {
        loadSettings();
    }, []);

    const loadSettings = async () => {
        setIsLoadingDevices(true);
        try {
            // Load API key
            const key = await tauri.getApiKey();
            if (key) {
                setSavedApiKey(key);
                setApiKey(maskApiKey(key));
                setApiKeyValid(true);
            }

            // Load devices
            const [devices, mons, savedSettings] = await Promise.all([
                tauri.getAudioDevices(),
                tauri.getMonitors(),
                tauri.getSavedSettings(),
            ]);

            setAudioDevices(devices);
            setMonitors(mons);

            // Set saved selections
            if (savedSettings.microphone) {
                setSelectedMic(savedSettings.microphone);
            } else if (devices.length > 0) {
                const defaultDevice = devices.find(d => d.is_default) || devices[0];
                setSelectedMic(defaultDevice.id);
            }

            if (savedSettings.monitor_id) {
                setSelectedMonitor(savedSettings.monitor_id);
            } else if (mons.length > 0) {
                const primaryMon = mons.find(m => m.is_primary) || mons[0];
                setSelectedMonitor(primaryMon.id);
            }
        } catch (err) {
            console.error("Failed to load settings:", err);
        } finally {
            setIsLoadingDevices(false);
        }
    };

    const maskApiKey = (key: string) => {
        if (key.length <= 8) return key;
        return key.slice(0, 4) + "..." + key.slice(-4);
    };

    const handleSaveApiKey = async () => {
        if (!apiKey || apiKey === maskApiKey(savedApiKey)) {
            return;
        }

        setIsSavingApiKey(true);
        try {
            await tauri.setDeepgramApiKey(apiKey);
            setSavedApiKey(apiKey);
            setApiKey(maskApiKey(apiKey));
            setApiKeyValid(true);
        } catch (err) {
            console.error("Failed to save API key:", err);
            setApiKeyValid(false);
        } finally {
            setIsSavingApiKey(false);
        }
    };

    const handleSelectMic = async (deviceId: string) => {
        setSelectedMic(deviceId);
        try {
            await tauri.setAudioDevice(deviceId);
        } catch (err) {
            console.error("Failed to set microphone:", err);
        }
    };

    const handleSelectMonitor = async (monitorId: number) => {
        setSelectedMonitor(monitorId);
        try {
            await tauri.setMonitor(monitorId);
        } catch (err) {
            console.error("Failed to set monitor:", err);
        }
    };

    return (
        <div className="full-settings scrollable">
            {/* Deepgram API Section */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🎙️</span>
                    Deepgram API
                </h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">API Key</span>
                        <span className="label-sub">Required for live transcription. Get one at deepgram.com</span>
                    </div>
                    <div className="settings-control">
                        <input
                            type="text"
                            className="settings-input"
                            placeholder="Enter your Deepgram API key"
                            value={apiKey}
                            onChange={(e) => {
                                setApiKey(e.target.value);
                                setApiKeyValid(null);
                            }}
                        />
                        <button
                            className={`settings-button ${apiKeyValid ? "success" : ""}`}
                            onClick={handleSaveApiKey}
                            disabled={isSavingApiKey || !apiKey}
                        >
                            {isSavingApiKey ? "Saving..." : apiKeyValid ? "✓ Saved" : "Save"}
                        </button>
                    </div>
                </div>
            </section>

            {/* Microphone Section */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🎤</span>
                    Microphone
                </h3>
                <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "var(--spacing-md)" }}>
                    Select the microphone for voice capture. This will be used for live transcription.
                </p>

                {isLoadingDevices ? (
                    <div className="loading-spinner" style={{ margin: "var(--spacing-lg) auto" }}></div>
                ) : (
                    <div className="device-list">
                        {audioDevices.filter(d => d.is_input).map((device) => (
                            <div
                                key={device.id}
                                className={`device-item ${selectedMic === device.id ? "selected" : ""}`}
                                onClick={() => handleSelectMic(device.id)}
                            >
                                <span className="device-icon">🎤</span>
                                <span className="device-name">{device.name}</span>
                                {device.is_default && (
                                    <span className="device-default">Default</span>
                                )}
                            </div>
                        ))}
                        {audioDevices.filter(d => d.is_input).length === 0 && (
                            <p style={{ color: "var(--text-tertiary)", textAlign: "center", padding: "var(--spacing-md)" }}>
                                No microphones detected
                            </p>
                        )}
                    </div>
                )}
            </section>

            {/* Monitor Section */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🖥️</span>
                    Screen Capture
                </h3>
                <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "var(--spacing-md)" }}>
                    Select which monitor to capture for screenshots and rewind.
                </p>

                {isLoadingDevices ? (
                    <div className="loading-spinner" style={{ margin: "var(--spacing-lg) auto" }}></div>
                ) : (
                    <div className="monitor-preview">
                        {monitors.map((monitor) => (
                            <div
                                key={monitor.id}
                                className={`monitor-card ${selectedMonitor === monitor.id ? "selected" : ""}`}
                                onClick={() => handleSelectMonitor(monitor.id)}
                            >
                                <div className="monitor-icon">🖥️</div>
                                <div className="monitor-name">{monitor.name}</div>
                                <div className="monitor-resolution">
                                    {monitor.width} × {monitor.height}
                                    {monitor.is_primary && " (Primary)"}
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </section>

            {/* System Audio Section */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🔊</span>
                    System Audio
                </h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Capture System Audio</span>
                        <span className="label-sub">
                            Automatically captures audio from your computer (Zoom, YouTube, etc.)
                        </span>
                    </div>
                    <div className="settings-control">
                        <span style={{
                            padding: "4px 12px",
                            background: "var(--success)",
                            borderRadius: "20px",
                            fontSize: "0.75rem",
                            fontWeight: "600"
                        }}>
                            ✓ Enabled
                        </span>
                    </div>
                </div>
                <p style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", marginTop: "var(--spacing-sm)" }}>
                    System audio capture uses ScreenCaptureKit. Make sure the app has Screen Recording permission.
                </p>
            </section>

            {/* Video Recording & Storage Section */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🎬</span>
                    Video Recording
                </h3>
                <p style={{ fontSize: "0.875rem", color: "var(--text-secondary)", marginBottom: "var(--spacing-md)" }}>
                    Continuous video recording captures your screen efficiently. Frames are extracted on-demand.
                </p>

                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Capture Mode</span>
                        <span className="label-sub">
                            Video recording is more efficient than frame capture
                        </span>
                    </div>
                    <div className="settings-control">
                        <span style={{
                            padding: "4px 12px",
                            background: "var(--accent-primary)",
                            borderRadius: "20px",
                            fontSize: "0.75rem",
                            fontWeight: "600",
                            color: "white"
                        }}>
                            🎬 Video (Recommended)
                        </span>
                    </div>
                </div>

                <div className="settings-row" style={{ marginTop: "var(--spacing-md)" }}>
                    <div className="settings-label">
                        <span className="label-main">Storage Usage</span>
                        <span className="label-sub">
                            Video files are stored locally and can be managed here
                        </span>
                    </div>
                    <div className="settings-control">
                        <button
                            className="settings-button"
                            onClick={async () => {
                                try {
                                    const { invoke } = await import("@tauri-apps/api/core");
                                    const [deleted, freed] = await invoke<[number, number]>("apply_retention");
                                    alert(`Cleaned up ${deleted} old recordings, freed ${(freed / 1024 / 1024).toFixed(1)} MB`);
                                } catch (err) {
                                    console.error("Cleanup failed:", err);
                                }
                            }}
                        >
                            🗑️ Cleanup Old Recordings
                        </button>
                    </div>
                </div>

                <p style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", marginTop: "var(--spacing-sm)" }}>
                    Videos are retained for 7 days by default. Pin moments are preserved longer.
                </p>
            </section>

            {/* Permissions Status */}
            <PermissionsStatus />

            {/* AI Settings */}
            <AISettings />

            {/* Knowledge Base Settings (Supabase, Pinecone, VLM) */}
            <KnowledgeBaseSettings />

            {/* About Section */}
            <section className="settings-section">
                <h3>
                    <span className="icon">ℹ️</span>
                    About
                </h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">noFriction Meetings</span>
                        <span className="label-sub">Version 1.0.0</span>
                    </div>
                </div>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Data Location</span>
                        <span className="label-sub" style={{ wordBreak: "break-all" }}>
                            ~/Library/Application Support/com.nofriction.meetings/
                        </span>
                    </div>
                </div>
            </section>

            {/* Data Management Section */}
            <section className="settings-section">
                <h3>
                    <span className="icon">🗃️</span>
                    Data Management
                </h3>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Clear Cache</span>
                        <span className="label-sub">Remove temporary analysis data and pending frames</span>
                    </div>
                    <div className="settings-control">
                        <button
                            className="btn btn-secondary"
                            onClick={async () => {
                                if (confirm("Clear all cached data? This cannot be undone.")) {
                                    try {
                                        const { invoke } = await import("@tauri-apps/api/core");
                                        await invoke("clear_cache");
                                        alert("Cache cleared successfully");
                                    } catch (err) {
                                        console.error("Failed to clear cache:", err);
                                        alert("Failed to clear cache");
                                    }
                                }
                            }}
                        >
                            Clear Cache
                        </button>
                    </div>
                </div>
                <div className="settings-row">
                    <div className="settings-label">
                        <span className="label-main">Export Data</span>
                        <span className="label-sub">Download all meetings and transcripts as JSON</span>
                    </div>
                    <div className="settings-control">
                        <button
                            className="btn btn-secondary"
                            onClick={async () => {
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
                            }}
                        >
                            Export Data
                        </button>
                    </div>
                </div>
            </section>
        </div>
    );
}
