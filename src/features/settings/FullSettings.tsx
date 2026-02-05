
// noFriction Meetings - Full Settings Component
// Comprehensive settings with device selection, API config, and preferences

import { useState, useEffect } from "react";
import * as tauri from "../../lib/tauri";
import type { AudioDevice, MonitorInfo } from "../../lib/tauri";
import { AISettings } from "./AISettings";
import { KnowledgeBaseSettings } from "./KnowledgeBaseSettings";
import { PermissionsStatus } from "./PermissionsStatus";
import { ActivityThemesSettings } from "./ActivityThemesSettings";
import PromptBrowser from "../../components/PromptBrowser";
import { TranscriptionSettings } from "./TranscriptionSettings";
import { IngestSettings } from "./IngestSettings";

import { useAppVersion } from '../../hooks/useAppVersion';

interface FullSettingsProps {
    onSave?: () => void;
}

export function FullSettings({ onSave: _onSave }: FullSettingsProps) {
    const version = useAppVersion();
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

    // Sidebar State
    const [activeCategory, setActiveCategory] = useState<"general" | "transcription" | "intelligence" | "capture" | "data">("general");

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

    const categories = [
        { id: "general", label: "General", icon: "⚙️" },
        { id: "transcription", label: "Transcription", icon: "🎙️" },
        { id: "capture", label: "Capture", icon: "🎥" },
        { id: "intelligence", label: "Intelligence", icon: "🧠" },
        { id: "data", label: "Data", icon: "💾" },
    ];

    const renderSidbar = () => (
        <div className="settings-sidebar glass-panel">
            <div className="sidebar-header">
                <h2>Settings</h2>
            </div>
            <nav className="sidebar-nav">
                {categories.map(cat => (
                    <button
                        key={cat.id}
                        className={`sidebar-item ${activeCategory === cat.id ? "active" : ""}`}
                        onClick={() => setActiveCategory(cat.id as any)}
                    >
                        <span className="sidebar-icon">{cat.icon}</span>
                        <span className="sidebar-label">{cat.label}</span>
                    </button>
                ))}
            </nav>
            <div className="sidebar-footer">
                <div className="app-version">v{version}</div>
            </div>
        </div>
    );

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
                            <h3>System Audio</h3>
                            <div className="settings-row">
                                <div className="settings-label">
                                    <span className="label-main">Capture System Audio</span>
                                    <span className="label-sub">Zoom, YouTube, etc.</span>
                                </div>
                                <div className="toggle-switch active">
                                    <div className="toggle-knob"></div>
                                </div>
                            </div>
                        </section>
                    </div>
                );
            case "transcription":
                return <TranscriptionSettings />;
            case "capture":
                return (
                    <div className="settings-content-panel fade-in">
                        <section className="settings-section">
                            <h3>Screen Capture</h3>
                            <p className="section-desc">Select which monitor to record.</p>
                            {isLoadingDevices ? (
                                <div className="loading-spinner" style={{ margin: "20px auto" }} />
                            ) : (
                                <div className="monitor-preview-grid">
                                    {monitors.map((monitor) => (
                                        <div
                                            key={monitor.id}
                                            className={`monitor-card ${selectedMonitor === monitor.id ? "selected" : ""}`}
                                            onClick={() => handleSelectMonitor(monitor.id)}
                                        >
                                            <div className="monitor-screen">
                                                <span className="monitor-res">{monitor.width}x{monitor.height}</span>
                                            </div>
                                            <div className="monitor-name">{monitor.name}</div>
                                            {monitor.is_primary && <span className="monitor-tag">Primary</span>}
                                        </div>
                                    ))}
                                </div>
                            )}
                        </section>
                        <section className="settings-section">
                            <h3>Recording Mode</h3>
                            <div className="segmented-control">
                                <button className="segment active">Video Mode</button>
                                <button className="segment">Frame Mode</button>
                            </div>
                            <ScreenshotFrequencySlider />
                        </section>
                    </div>
                );
            case "intelligence":
                return (
                    <div className="settings-content-panel fade-in">
                        <section className="settings-section">
                            <h3>Activity Themes</h3>
                            <ActivityThemesSettings />
                        </section>
                        {/* AISettings included here to fix unused variable error */}
                        <div className="settings-section-divider"></div>
                        <AISettings />
                        <div className="settings-section-divider"></div>
                        <IngestSettings />
                        <section className="settings-section">
                            <h3>Deepgram API</h3>
                            <div className="settings-input-group">
                                <input
                                    type="password"
                                    placeholder="Enter API Key"
                                    value={apiKey}
                                    onChange={(e) => { setApiKey(e.target.value); setApiKeyValid(null); }}
                                    className="modern-input"
                                />
                                <button className="btn-primary" onClick={handleSaveApiKey} disabled={isSavingApiKey}>
                                    {isSavingApiKey ? "Saving..." : apiKeyValid ? "Saved" : "Save"}
                                </button>
                            </div>
                        </section>
                        <section className="settings-section">
                            <h3>Knowledge Base</h3>
                            <KnowledgeBaseSettings />
                        </section>
                        <section className="settings-section">
                            <h3>Prompt Library</h3>
                            <PromptBrowser />
                        </section>
                    </div>
                );
            case "data":
                return (
                    <div className="settings-content-panel fade-in">
                        <PermissionsStatus />
                        <section className="settings-section">
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
                        </section>
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
            default:
                return null;
        }
    };

    return (
        <div className="full-settings-layout">
            {renderSidbar()}
            <div className="settings-main-content">
                <div className="content-header">
                    <h1>{categories.find(c => c.id === activeCategory)?.label}</h1>
                </div>
                <div className="content-scrollable">
                    {renderContent()}
                </div>
            </div>
        </div>
    );
}

// Internal component for Screenshot Frequency slider with dynamic display
function ScreenshotFrequencySlider() {
    const [intervalMs, setIntervalMs] = useState(1000);

    // Load saved setting on mount
    useEffect(() => {
        const loadSetting = async () => {
            try {
                const { invoke } = await import("@tauri-apps/api/core");
                const settings = await invoke<{ frame_capture_interval_ms?: number }>("get_capture_settings");
                if (settings.frame_capture_interval_ms) {
                    setIntervalMs(settings.frame_capture_interval_ms);
                }
            } catch (err) {
                console.error("Failed to load frame interval:", err);
            }
        };
        loadSetting();
    }, []);

    const handleChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const newInterval = parseInt(e.target.value, 10);
        setIntervalMs(newInterval);
        try {
            await tauri.setFrameCaptureInterval(newInterval);
        } catch (err) {
            console.error("Failed to set frame interval:", err);
        }
    };

    // Format the interval for display
    const formatInterval = (ms: number): string => {
        if (ms <= 500) {
            return `${Math.round(1000 / ms)} per second`;
        } else if (ms === 1000) {
            return "1 per second";
        } else {
            return `1 every ${(ms / 1000).toFixed(1)}s`;
        }
    };

    return (
        <div className="settings-row" style={{ marginTop: "var(--spacing-md)" }}>
            <div className="settings-label">
                <span className="label-main">Screenshot Frequency</span>
                <span className="label-sub">
                    How often to capture screenshots during recording
                </span>
            </div>
            <div className="settings-control" style={{ display: "flex", flexDirection: "column", gap: "8px", alignItems: "flex-end" }}>
                <input
                    type="range"
                    min={100}
                    max={5000}
                    step={100}
                    value={intervalMs}
                    onChange={handleChange}
                    style={{ width: "150px" }}
                />
                <span style={{
                    fontSize: "0.75rem",
                    color: "var(--text-secondary)",
                    fontWeight: intervalMs === 1000 ? 400 : 600
                }}>
                    {formatInterval(intervalMs)}{intervalMs === 1000 ? " (default)" : ""}
                </span>
            </div>
        </div>
    );
}
