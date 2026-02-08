// noFriction Meetings - Recording Controls Component
// Start/stop/pause buttons with status display

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface RecordingControlsProps {
    isRecording: boolean;
    isPaused?: boolean;
    duration: number;
    videoFrames: number;
    audioSamples: number;
    onToggle: () => void;
    onPause?: () => void;
    disabled?: boolean;
}

export function RecordingControls({
    isRecording,
    isPaused = false,
    duration,
    videoFrames,
    audioSamples,
    onToggle,
    onPause,
    disabled = false,
}: RecordingControlsProps) {
    const [sessionMode, setSessionMode] = useState<"standard" | "dork">("standard");
    const [isLoadingMode, setIsLoadingMode] = useState(false);

    // Load session mode on mount
    useEffect(() => {
        invoke<string>("get_session_mode")
            .then((mode) => setSessionMode(mode as "standard" | "dork"))
            .catch(console.error);
    }, []);

    const toggleMode = useCallback(async () => {
        if (isRecording) return; // Don't change mode during recording

        setIsLoadingMode(true);
        const newMode = sessionMode === "standard" ? "dork" : "standard";
        try {
            await invoke("set_session_mode", { mode: newMode });
            setSessionMode(newMode);
        } catch (err) {
            console.error("Failed to set session mode:", err);
        } finally {
            setIsLoadingMode(false);
        }
    }, [sessionMode, isRecording]);

    const formatDuration = (seconds: number) => {
        const hrs = Math.floor(seconds / 3600);
        const mins = Math.floor((seconds % 3600) / 60);
        const secs = seconds % 60;

        if (hrs > 0) {
            return `${hrs}:${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
        }
        return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
    };

    const formatNumber = (num: number) => {
        if (num >= 1000000) {
            return `${(num / 1000000).toFixed(1)}M`;
        }
        if (num >= 1000) {
            return `${(num / 1000).toFixed(1)}K`;
        }
        return num.toString();
    };

    return (
        <div className="recording-controls glass-panel">
            {/* Dork Mode Toggle */}
            <div
                className="mode-toggle"
                style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "12px 16px",
                    marginBottom: "var(--spacing-md)",
                    background: sessionMode === "dork"
                        ? "linear-gradient(135deg, rgba(147, 51, 234, 0.2), rgba(79, 70, 229, 0.2))"
                        : "var(--bg-secondary)",
                    borderRadius: "12px",
                    border: sessionMode === "dork"
                        ? "1px solid rgba(147, 51, 234, 0.5)"
                        : "1px solid var(--border)",
                    transition: "all 0.3s ease",
                    opacity: isRecording ? 0.6 : 1,
                    cursor: isRecording ? "not-allowed" : "pointer",
                }}
                onClick={toggleMode}
            >
                <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
                    <span style={{ fontSize: "1.25rem" }}>
                        {sessionMode === "dork" ? "📚" : "🎙️"}
                    </span>
                    <div>
                        <div style={{
                            fontWeight: 600,
                            fontSize: "0.9rem",
                            color: sessionMode === "dork" ? "var(--accent)" : "var(--text-primary)"
                        }}>
                            {sessionMode === "dork" ? "Dork Mode" : "Standard Mode"}
                        </div>
                        <div style={{
                            fontSize: "0.75rem",
                            color: "var(--text-muted)",
                            marginTop: "2px"
                        }}>
                            {sessionMode === "dork"
                                ? "Study materials & quiz at end"
                                : "Normal recording session"
                            }
                        </div>
                    </div>
                </div>
                <div
                    className="toggle-switch"
                    style={{
                        width: "44px",
                        height: "24px",
                        borderRadius: "12px",
                        background: sessionMode === "dork"
                            ? "linear-gradient(135deg, #9333ea, #4f46e5)"
                            : "var(--bg-tertiary)",
                        position: "relative",
                        transition: "all 0.3s ease",
                        boxShadow: sessionMode === "dork"
                            ? "0 0 12px rgba(147, 51, 234, 0.4)"
                            : "none",
                    }}
                >
                    <div style={{
                        width: "18px",
                        height: "18px",
                        borderRadius: "50%",
                        background: "#fff",
                        position: "absolute",
                        top: "3px",
                        left: sessionMode === "dork" ? "23px" : "3px",
                        transition: "left 0.3s ease",
                        boxShadow: "0 2px 4px rgba(0,0,0,0.2)",
                    }} />
                </div>
            </div>

            <button
                className={`record-button ${isRecording ? "recording" : "ready"} ${isPaused ? "paused" : ""}`}
                onClick={onToggle}
                disabled={disabled || isLoadingMode}
            >
                <span className="record-indicator" />
                {isRecording ? "Stop Recording" : "Start Recording"}
            </button>

            {isRecording && onPause && (
                <button
                    className={`pause-button ${isPaused ? "paused" : ""}`}
                    onClick={onPause}
                    style={{
                        marginTop: "var(--spacing-sm)",
                        padding: "8px 16px",
                        borderRadius: "8px",
                        border: "1px solid var(--border)",
                        background: isPaused ? "var(--warning)" : "var(--bg-secondary)",
                        color: isPaused ? "white" : "var(--text-primary)",
                        cursor: "pointer",
                        width: "100%",
                    }}
                >
                    {isPaused ? "▶️ Resume" : "⏸️ Pause"}
                </button>
            )}

            {isRecording && (
                <div className="recording-status">
                    <div className="recording-duration">
                        {formatDuration(duration)}
                        {isPaused && <span style={{ marginLeft: "8px", color: "var(--warning)" }}>(Paused)</span>}
                        {sessionMode === "dork" && (
                            <span style={{
                                marginLeft: "8px",
                                color: "var(--accent)",
                                fontSize: "0.8rem"
                            }}>
                                📚 Study Mode
                            </span>
                        )}
                    </div>
                    <div className="recording-stats">
                        <span>🎬 {formatNumber(videoFrames)} frames</span>
                        <span>🔊 {formatNumber(audioSamples)} samples</span>
                    </div>
                    <button
                        className="pin-moment-btn"
                        onClick={async () => {
                            try {
                                const { videoPinMoment } = await import("../lib/tauri");
                                await videoPinMoment();
                            } catch (err) {
                                console.error("Failed to pin moment:", err);
                            }
                        }}
                    >
                        📌 Pin Moment
                    </button>
                </div>
            )
            }
        </div >
    );
}
