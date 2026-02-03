import { useEffect, useState } from 'react';
import {
    getCaptureMode,
    startAmbientCapture,
    startMeetingCapture,
    pauseCapture,
    getAlwaysOnSettings,
    setAlwaysOnEnabled,
    type CaptureMode,
    type AlwaysOnSettings
} from '../lib/tauri';

interface ModeOption {
    id: CaptureMode;
    label: string;
    icon: string;
    description: string;
    color: string;
}

const CAPTURE_MODES: ModeOption[] = [
    {
        id: 'Ambient',
        label: 'Ambient',
        icon: '🌙',
        description: 'Silent background capture (30s intervals, no audio)',
        color: '#3B82F6'
    },
    {
        id: 'Meeting',
        label: 'Meeting',
        icon: '🎙️',
        description: 'Full capture with audio (2s intervals)',
        color: '#10B981'
    },
    {
        id: 'Paused',
        label: 'Paused',
        icon: '⏸️',
        description: 'No capture active',
        color: '#64748B'
    }
];

export function AlwaysOnSettings() {
    const [currentMode, setCurrentMode] = useState<CaptureMode>('Paused');
    const [settings, setSettings] = useState<AlwaysOnSettings | null>(null);
    const [enabled, setEnabled] = useState(false);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        loadState();
        // Poll for mode changes every 5 seconds
        const interval = setInterval(loadCurrentMode, 5000);
        return () => clearInterval(interval);
    }, []);

    const loadState = async () => {
        try {
            setLoading(true);
            const [mode, settingsData] = await Promise.all([
                getCaptureMode(),
                getAlwaysOnSettings()
            ]);
            setCurrentMode(mode);
            setSettings(settingsData);
            setEnabled(settingsData.enabled);
        } catch (err) {
            console.error('Failed to load Always-On state:', err);
            setError('Failed to load settings');
        } finally {
            setLoading(false);
        }
    };

    const loadCurrentMode = async () => {
        try {
            const mode = await getCaptureMode();
            setCurrentMode(mode);
        } catch (err) {
            console.error('Failed to get capture mode:', err);
        }
    };

    const handleModeChange = async (newMode: CaptureMode) => {
        try {
            setError(null);
            switch (newMode) {
                case 'Ambient':
                    await startAmbientCapture();
                    break;
                case 'Meeting':
                    await startMeetingCapture();
                    break;
                case 'Paused':
                    await pauseCapture();
                    break;
            }
            setCurrentMode(newMode);
        } catch (err) {
            console.error('Failed to change capture mode:', err);
            setError(`Failed to switch to ${newMode} mode`);
        }
    };

    const handleEnableToggle = async () => {
        try {
            setError(null);
            const newState = !enabled;
            await setAlwaysOnEnabled(newState);
            setEnabled(newState);

            if (newState) {
                // When enabling, start in ambient mode
                await startAmbientCapture();
                setCurrentMode('Ambient');
            } else {
                // When disabling, pause capture
                await pauseCapture();
                setCurrentMode('Paused');
            }
        } catch (err) {
            console.error('Failed to toggle Always-On:', err);
            setError('Failed to toggle Always-On mode');
        }
    };

    const currentModeConfig = CAPTURE_MODES.find(m => m.id === currentMode) || CAPTURE_MODES[2];

    if (loading) {
        return (
            <div className="always-on-loading">
                <div className="spinner" />
                <span>Loading Always-On settings...</span>
            </div>
        );
    }

    return (
        <div className="always-on-container">
            {error && (
                <div className="always-on-error">
                    ⚠️ {error}
                </div>
            )}

            {/* Hero: Current Status */}
            <div
                className="always-on-hero"
                style={{ borderColor: currentModeConfig.color }}
            >
                <div className="always-on-hero-header">
                    <div className="always-on-hero-left">
                        <span className="always-on-hero-icon">{currentModeConfig.icon}</span>
                        <div>
                            <div className="always-on-hero-title">
                                {currentModeConfig.label} Mode
                            </div>
                            <div className="always-on-hero-description">
                                {currentModeConfig.description}
                            </div>
                        </div>
                    </div>
                    <div className="always-on-status-indicator" style={{ background: currentModeConfig.color }}>
                        {currentMode === 'Paused' ? 'OFF' : 'ON'}
                    </div>
                </div>
            </div>

            {/* Master Toggle */}
            <div className="always-on-toggle-section">
                <div className="always-on-toggle-info">
                    <span className="always-on-toggle-label">Always-On Recording</span>
                    <span className="always-on-toggle-sublabel">
                        Continuous background capture with smart meeting detection
                    </span>
                </div>
                <label className="always-on-switch">
                    <input
                        type="checkbox"
                        checked={enabled}
                        onChange={handleEnableToggle}
                    />
                    <span className="always-on-slider"></span>
                </label>
            </div>

            {/* Mode Selector */}
            <div className="always-on-modes">
                <div className="always-on-modes-header">Capture Mode</div>
                <div className="always-on-modes-grid">
                    {CAPTURE_MODES.map((mode) => (
                        <button
                            key={mode.id}
                            className={`always-on-mode-card ${currentMode === mode.id ? 'active' : ''}`}
                            style={{
                                borderColor: currentMode === mode.id ? mode.color : 'transparent',
                                background: currentMode === mode.id
                                    ? `${mode.color}15`
                                    : 'rgba(255, 255, 255, 0.05)'
                            }}
                            onClick={() => handleModeChange(mode.id)}
                        >
                            <span className="mode-icon">{mode.icon}</span>
                            <span className="mode-label">{mode.label}</span>
                        </button>
                    ))}
                </div>
            </div>

            {/* Detection Settings */}
            {settings && (
                <div className="always-on-detection">
                    <div className="always-on-detection-header">Auto-Detection</div>
                    <div className="always-on-detection-grid">
                        <div className="detection-item">
                            <span className="detection-icon">📅</span>
                            <div className="detection-info">
                                <span className="detection-label">Calendar Events</span>
                                <span className="detection-sublabel">
                                    Auto-switch to Meeting mode
                                </span>
                            </div>
                            <span className={`detection-badge ${settings.calendar_detection ? 'on' : 'off'}`}>
                                {settings.calendar_detection ? 'ON' : 'OFF'}
                            </span>
                        </div>
                        <div className="detection-item">
                            <span className="detection-icon">💻</span>
                            <div className="detection-info">
                                <span className="detection-label">Meeting Apps</span>
                                <span className="detection-sublabel">
                                    Zoom, Meet, Teams, Slack
                                </span>
                            </div>
                            <span className={`detection-badge ${settings.app_detection ? 'on' : 'off'}`}>
                                {settings.app_detection ? 'ON' : 'OFF'}
                            </span>
                        </div>
                    </div>
                </div>
            )}

            {/* Settings Summary */}
            {settings && (
                <div className="always-on-summary">
                    <div className="summary-item">
                        <span className="summary-label">Idle Timeout</span>
                        <span className="summary-value">{settings.idle_timeout_mins} min</span>
                    </div>
                    <div className="summary-item">
                        <span className="summary-label">Ambient Interval</span>
                        <span className="summary-value">{settings.ambient_interval_secs}s</span>
                    </div>
                    <div className="summary-item">
                        <span className="summary-label">Meeting Interval</span>
                        <span className="summary-value">{settings.meeting_interval_secs}s</span>
                    </div>
                    <div className="summary-item">
                        <span className="summary-label">Retention</span>
                        <span className="summary-value">{settings.retention_hours}h</span>
                    </div>
                </div>
            )}
        </div>
    );
}
