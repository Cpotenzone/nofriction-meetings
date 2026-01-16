// noFriction Meetings - Recording Controls Component
// Start/stop/pause buttons with status display

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
            <button
                className={`record-button ${isRecording ? "recording" : "ready"} ${isPaused ? "paused" : ""}`}
                onClick={onToggle}
                disabled={disabled}
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
                    </div>
                    <div className="recording-stats">
                        <span>🎬 {formatNumber(videoFrames)} frames</span>
                        <span>🔊 {formatNumber(audioSamples)} samples</span>
                    </div>
                    <button
                        className="pin-moment-btn"
                        onClick={async () => {
                            try {
                                const { invoke } = await import("@tauri-apps/api/core");
                                await invoke("video_pin_moment", { label: null });
                            } catch (err) {
                                console.error("Failed to pin moment:", err);
                            }
                        }}
                    >
                        📌 Pin Moment
                    </button>
                </div>
            )}
        </div>
    );
}
