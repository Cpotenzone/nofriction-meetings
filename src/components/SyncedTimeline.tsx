// noFriction Meetings - Synced Timeline Component
// Displays frames and transcripts aligned by timestamp for rewind

import { useEffect, useState, useCallback } from "react";
import * as tauri from "../lib/tauri";
import type { SyncedTimeline, TimelineFrame, TimelineTranscript } from "../lib/tauri";

interface SyncedTimelineProps {
    meetingId: string | null;
    onFrameSelect?: (frame: TimelineFrame) => void;
}

export function SyncedTimelineView({ meetingId, onFrameSelect }: SyncedTimelineProps) {
    const [timeline, setTimeline] = useState<SyncedTimeline | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [currentTimeMs, setCurrentTimeMs] = useState(0);
    const [selectedFrame, setSelectedFrame] = useState<TimelineFrame | null>(null);
    const [thumbnail, setThumbnail] = useState<string | null>(null);

    // Load timeline data
    useEffect(() => {
        if (!meetingId) {
            setTimeline(null);
            return;
        }

        const loadTimeline = async () => {
            setLoading(true);
            setError(null);
            try {
                const data = await tauri.getSyncedTimeline(meetingId);
                setTimeline(data);
                if (data && data.frames.length > 0) {
                    setCurrentTimeMs(0);
                }
            } catch (err) {
                setError(String(err));
            } finally {
                setLoading(false);
            }
        };

        loadTimeline();

        // Refresh every 5 seconds while recording
        const interval = setInterval(loadTimeline, 5000);
        return () => clearInterval(interval);
    }, [meetingId]);

    // Find the frame at current time
    useEffect(() => {
        if (!timeline || timeline.frames.length === 0) {
            setSelectedFrame(null);
            return;
        }

        // Find the frame closest to but not after currentTimeMs
        let closest: TimelineFrame | null = null;
        for (const frame of timeline.frames) {
            if (frame.timestamp_ms <= currentTimeMs) {
                closest = frame;
            } else {
                break;
            }
        }

        if (!closest && timeline.frames.length > 0) {
            closest = timeline.frames[0];
        }

        setSelectedFrame(closest);
    }, [timeline, currentTimeMs]);

    // Load thumbnail when frame changes
    useEffect(() => {
        if (!selectedFrame || !meetingId) {
            setThumbnail(null);
            return;
        }

        const loadThumbnail = async () => {
            try {
                const data = await tauri.getFrameThumbnail(selectedFrame.id);
                setThumbnail(data);
            } catch {
                setThumbnail(null);
            }
        };

        loadThumbnail();
    }, [selectedFrame, meetingId]);

    // Get transcript at current time
    const getCurrentTranscript = useCallback((): TimelineTranscript | null => {
        if (!timeline || timeline.transcripts.length === 0) return null;

        // Find the transcript closest to currentTimeMs
        let closest: TimelineTranscript | null = null;
        for (const t of timeline.transcripts) {
            if (t.timestamp_ms <= currentTimeMs + 5000) {
                if (!closest || Math.abs(t.timestamp_ms - currentTimeMs) < Math.abs(closest.timestamp_ms - currentTimeMs)) {
                    closest = t;
                }
            }
        }
        return closest;
    }, [timeline, currentTimeMs]);

    // Handle scrubber change
    const handleScrub = (e: React.ChangeEvent<HTMLInputElement>) => {
        const ms = parseInt(e.target.value, 10);
        setCurrentTimeMs(ms);

        if (selectedFrame && onFrameSelect) {
            onFrameSelect(selectedFrame);
        }
    };

    // Format duration
    const formatTime = (ms: number): string => {
        const totalSeconds = Math.floor(ms / 1000);
        const minutes = Math.floor(totalSeconds / 60);
        const seconds = totalSeconds % 60;
        return `${minutes}:${seconds.toString().padStart(2, "0")}`;
    };

    if (!meetingId) {
        return (
            <div className="synced-timeline empty-state">
                <div className="empty-state-icon">🎬</div>
                <p>Select a meeting or start recording to use the timeline</p>
            </div>
        );
    }

    if (loading && !timeline) {
        return (
            <div className="synced-timeline loading">
                <div className="spinner"></div>
                <p>Loading timeline...</p>
            </div>
        );
    }

    if (error) {
        return (
            <div className="synced-timeline error">
                <p>⚠️ {error}</p>
            </div>
        );
    }

    if (!timeline) {
        return (
            <div className="synced-timeline empty-state">
                <div className="empty-state-icon">📹</div>
                <p>No timeline data available</p>
            </div>
        );
    }

    const durationMs = timeline.duration_seconds * 1000 ||
        Math.max(
            ...timeline.frames.map(f => f.timestamp_ms),
            ...timeline.transcripts.map(t => t.timestamp_ms),
            1000
        );

    const currentTranscript = getCurrentTranscript();

    return (
        <div className="synced-timeline">
            {/* Header */}
            <div className="timeline-header">
                <h3>🎬 {timeline.meeting_title}</h3>
                <span className="timeline-stats">
                    {timeline.frames.length} frames • {timeline.transcripts.length} transcripts
                </span>
            </div>

            {/* Frame Preview */}
            <div className="timeline-preview">
                {thumbnail ? (
                    <img src={thumbnail} alt="Screen capture" />
                ) : (
                    <div className="preview-placeholder">
                        {selectedFrame ? "Loading..." : "No frame available"}
                    </div>
                )}
                <div className="preview-timestamp">
                    {formatTime(currentTimeMs)}
                </div>
            </div>

            {/* Current Transcript */}
            <div className="timeline-transcript-display">
                {currentTranscript ? (
                    <>
                        <div className="transcript-text">
                            "{currentTranscript.text}"
                        </div>
                        <div className="transcript-meta">
                            {currentTranscript.speaker && (
                                <span className="speaker">{currentTranscript.speaker}</span>
                            )}
                            <span className="time">{formatTime(currentTranscript.timestamp_ms)}</span>
                            {currentTranscript.is_final && (
                                <span className="final-badge">✓</span>
                            )}
                        </div>
                    </>
                ) : (
                    <div className="transcript-placeholder">
                        No transcript at this time
                    </div>
                )}
            </div>

            {/* Scrubber */}
            <div className="timeline-scrubber-container">
                <input
                    type="range"
                    min={0}
                    max={durationMs}
                    value={currentTimeMs}
                    onChange={handleScrub}
                    className="timeline-scrubber"
                />

                {/* Frame markers */}
                <div className="timeline-frame-markers">
                    {timeline.frames.map((frame) => (
                        <div
                            key={frame.id}
                            className={`frame-marker ${selectedFrame?.id === frame.id ? 'active' : ''}`}
                            style={{
                                left: `${(frame.timestamp_ms / durationMs) * 100}%`
                            }}
                            onClick={() => setCurrentTimeMs(frame.timestamp_ms)}
                            title={formatTime(frame.timestamp_ms)}
                        />
                    ))}
                </div>

                {/* Transcript markers */}
                <div className="timeline-transcript-markers">
                    {timeline.transcripts.filter(t => t.is_final).map((t) => (
                        <div
                            key={t.id}
                            className={`transcript-marker ${currentTranscript?.id === t.id ? 'active' : ''}`}
                            style={{
                                left: `${(t.timestamp_ms / durationMs) * 100}%`
                            }}
                            onClick={() => setCurrentTimeMs(t.timestamp_ms)}
                            title={t.text.slice(0, 50)}
                        />
                    ))}
                </div>
            </div>

            {/* Time labels */}
            <div className="timeline-labels">
                <span>0:00</span>
                <span>{formatTime(durationMs)}</span>
            </div>

            {/* Transcript list (scrollable) */}
            <div className="timeline-transcript-list">
                <h4>All Transcripts</h4>
                {timeline.transcripts.length === 0 ? (
                    <p className="no-transcripts">No transcripts yet</p>
                ) : (
                    <div className="transcript-items">
                        {timeline.transcripts.map((t) => (
                            <div
                                key={t.id}
                                className={`transcript-item ${currentTranscript?.id === t.id ? 'active' : ''}`}
                                onClick={() => setCurrentTimeMs(t.timestamp_ms)}
                            >
                                <span className="time">{formatTime(t.timestamp_ms)}</span>
                                <span className="text">{t.text}</span>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
