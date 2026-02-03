// noFriction Meetings - Ambient Timeline Component
// Displays ambient capture sessions with mode indicators and quick navigation

import { useEffect, useState, useCallback } from 'react';
import {
    getCaptureMode,
    startAmbientCapture,
    startMeetingCapture,
    pauseCapture,
    type CaptureMode
} from '../lib/tauri';

interface AmbientSession {
    id: string;
    startTime: string;
    endTime: string | null;
    mode: CaptureMode;
    frameCount: number;
    meetings: { id: string; title: string }[];
}

interface AmbientTimelineProps {
    onSessionClick?: (sessionId: string) => void;
    compact?: boolean;
}

export function AmbientTimeline({ onSessionClick, compact = false }: AmbientTimelineProps) {
    const [currentMode, setCurrentMode] = useState<CaptureMode>('Paused');
    const [sessions, setSessions] = useState<AmbientSession[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [isRecording, setIsRecording] = useState(false);

    // Load current mode and sessions
    const loadState = useCallback(async () => {
        try {
            const mode = await getCaptureMode();
            setCurrentMode(mode);
            setIsRecording(mode !== 'Paused');

            // Try to get recent sessions (mock for now - would connect to database)
            // In production, this would call get_ambient_sessions command
            setSessions([]);
        } catch (err) {
            console.error('Failed to load ambient state:', err);
            setError(String(err));
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        loadState();
        // Poll for mode changes
        const interval = setInterval(loadState, 5000);
        return () => clearInterval(interval);
    }, [loadState]);

    const handleModeToggle = async () => {
        try {
            setError(null);
            if (isRecording) {
                await pauseCapture();
                setIsRecording(false);
                setCurrentMode('Paused');
            } else {
                await startAmbientCapture();
                setIsRecording(true);
                setCurrentMode('Ambient');
            }
        } catch (err) {
            setError(`Failed to toggle recording: ${err}`);
        }
    };

    const handleEscalateToMeeting = async () => {
        try {
            setError(null);
            await startMeetingCapture();
            setCurrentMode('Meeting');
            setIsRecording(true);
        } catch (err) {
            setError(`Failed to start meeting mode: ${err}`);
        }
    };

    const getModeConfig = (mode: CaptureMode) => {
        switch (mode) {
            case 'Ambient':
                return { color: '#3B82F6', icon: '🌙', label: 'Ambient', pulse: true };
            case 'Meeting':
                return { color: '#EF4444', icon: '🎙️', label: 'Meeting', pulse: true };
            case 'Paused':
            default:
                return { color: '#64748B', icon: '⏸️', label: 'Paused', pulse: false };
        }
    };

    const modeConfig = getModeConfig(currentMode);

    if (compact) {
        return (
            <div className="ambient-compact">
                <div
                    className={`ambient-mode-badge ${modeConfig.pulse ? 'pulse' : ''}`}
                    style={{ background: modeConfig.color }}
                >
                    <span>{modeConfig.icon}</span>
                    <span>{modeConfig.label}</span>
                </div>
                <button
                    className="ambient-toggle-btn"
                    onClick={handleModeToggle}
                >
                    {isRecording ? '⏹️ Stop' : '▶️ Start'}
                </button>
            </div>
        );
    }

    return (
        <div className="ambient-timeline">
            {error && (
                <div className="ambient-error">⚠️ {error}</div>
            )}

            {/* Current Mode Hero */}
            <div className="ambient-hero" style={{ borderColor: modeConfig.color }}>
                <div className="ambient-hero-left">
                    <div
                        className={`ambient-mode-indicator ${modeConfig.pulse ? 'pulse' : ''}`}
                        style={{ background: modeConfig.color }}
                    >
                        <span className="ambient-mode-icon">{modeConfig.icon}</span>
                    </div>
                    <div className="ambient-mode-info">
                        <div className="ambient-mode-label">{modeConfig.label} Mode</div>
                        <div className="ambient-mode-status">
                            {isRecording ? 'Recording active' : 'Ready to record'}
                        </div>
                    </div>
                </div>
                <div className="ambient-hero-controls">
                    <button
                        className={`ambient-control-btn ${isRecording ? 'recording' : 'stopped'}`}
                        onClick={handleModeToggle}
                    >
                        {isRecording ? '⏹️ Stop' : '▶️ Start'}
                    </button>
                    {currentMode === 'Ambient' && (
                        <button
                            className="ambient-control-btn escalate"
                            onClick={handleEscalateToMeeting}
                        >
                            🎙️ Meeting Mode
                        </button>
                    )}
                </div>
            </div>

            {/* Sessions Timeline */}
            <div className="ambient-sessions">
                <div className="ambient-sessions-header">
                    Recent Sessions
                </div>

                {loading ? (
                    <div className="ambient-loading">Loading sessions...</div>
                ) : sessions.length === 0 ? (
                    <div className="ambient-empty">
                        <div className="ambient-empty-icon">📹</div>
                        <div className="ambient-empty-text">No sessions yet</div>
                        <div className="ambient-empty-hint">
                            Start recording to capture your work
                        </div>
                    </div>
                ) : (
                    <div className="ambient-sessions-list">
                        {sessions.map((session) => {
                            const sessionConfig = getModeConfig(session.mode);
                            return (
                                <div
                                    key={session.id}
                                    className="ambient-session-card"
                                    onClick={() => onSessionClick?.(session.id)}
                                >
                                    <div
                                        className="session-mode-dot"
                                        style={{ background: sessionConfig.color }}
                                    />
                                    <div className="session-info">
                                        <div className="session-time">
                                            {new Date(session.startTime).toLocaleTimeString()}
                                            {session.endTime && ` - ${new Date(session.endTime).toLocaleTimeString()}`}
                                        </div>
                                        <div className="session-details">
                                            {session.frameCount} frames
                                            {session.meetings.length > 0 && (
                                                <> · {session.meetings.length} meeting{session.meetings.length > 1 ? 's' : ''}</>
                                            )}
                                        </div>
                                    </div>
                                    <div className="session-mode-badge" style={{ color: sessionConfig.color }}>
                                        {sessionConfig.icon}
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                )}
            </div>
        </div>
    );
}

export default AmbientTimeline;
