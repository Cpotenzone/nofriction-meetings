// noFriction Meetings - Rewind Gallery Component
// Full meeting review with frame gallery, timeline, and synced transcripts

import { useState, useEffect, useRef, useCallback } from "react";
import { getSyncedTimeline, getFrameThumbnail, debugLog } from "../lib/tauri";
import type { SyncedTimeline, TimelineFrame } from "../lib/tauri";

interface RewindGalleryProps {
    meetingId: string | null;
    isRecording: boolean;
}

export function RewindGallery({ meetingId, isRecording }: RewindGalleryProps) {
    const [timeline, setTimeline] = useState<SyncedTimeline | null>(null);
    const [currentTime, setCurrentTime] = useState(0);
    const [selectedFrame, setSelectedFrame] = useState<TimelineFrame | null>(null);
    const [frameImage, setFrameImage] = useState<string | null>(null);
    const [thumbnails, setThumbnails] = useState<Map<string, string>>(new Map());
    const [isLoading, setIsLoading] = useState(false);

    const galleryRef = useRef<HTMLDivElement>(null);
    const transcriptRef = useRef<HTMLDivElement>(null);

    // Load timeline data
    useEffect(() => {
        // Reset state immediately when meeting changes
        setTimeline(null);
        setSelectedFrame(null);
        setFrameImage(null);
        setThumbnails(new Map());

        if (!meetingId) {
            return;
        }

        const loadTimeline = async () => {
            debugLog(`🔄 RewindGallery: Loading timeline for meetingId: ${meetingId}`);
            console.log("🔄 RewindGallery: Loading timeline for meetingId:", meetingId);
            setIsLoading(true);
            try {
                const data = await getSyncedTimeline(meetingId);
                debugLog(`✅ RewindGallery: Received timeline data with ${data.frames.length} frames`);
                console.log("✅ RewindGallery: Received timeline data:", data);
                setTimeline(data);

                // Select first frame if available
                if (data.frames.length > 0) {
                    debugLog(`📸 RewindGallery: Selecting first frame of ${data.frames.length} frames`);
                    console.log(`📸 RewindGallery: Selecting first frame of ${data.frames.length} frames`);
                    setSelectedFrame(data.frames[0]);
                    setCurrentTime(data.frames[0].timestamp_ms);
                } else {
                    debugLog("⚠️ RewindGallery: No frames found in timeline data");
                    console.warn("⚠️ RewindGallery: No frames found in timeline data");
                }
            } catch (err) {
                debugLog(`❌ RewindGallery: Failed to load timeline: ${err}`);
                console.error("Failed to load timeline:", err);
            } finally {
                setIsLoading(false);
            }
        };

        loadTimeline();

        // Auto-refresh while recording
        let interval: ReturnType<typeof setInterval> | null = null;
        if (isRecording) {
            interval = setInterval(loadTimeline, 3000);
        }

        return () => {
            if (interval) clearInterval(interval);
        };
    }, [meetingId, isRecording]);

    // Load selected frame image
    useEffect(() => {
        if (!selectedFrame) {
            setFrameImage(null);
            return;
        }

        const loadFrame = async () => {
            try {
                const base64 = await getFrameThumbnail(selectedFrame.id, false);
                if (base64) {
                    setFrameImage(`data:image/jpeg;base64,${base64}`);
                }
            } catch (err) {
                console.error("Failed to load frame:", err);
            }
        };

        loadFrame();
    }, [selectedFrame]);

    // Load thumbnails progressively
    useEffect(() => {
        if (!timeline) return;

        const loadThumbnails = async () => {
            for (const frame of timeline.frames) {
                if (!thumbnails.has(frame.id)) {
                    try {
                        const base64 = await getFrameThumbnail(frame.id, true);
                        if (base64) {
                            // debugLog(`framedata: ${base64.substring(0, 50)}...`);
                            setThumbnails((prev) => new Map(prev).set(frame.id, `data:image/jpeg;base64,${base64}`));
                        }
                    } catch (err) {
                        console.error(`Failed to load thumbnail for frame ${frame.id}:`, err);
                        // debugLog(`❌ Failed to load thumbnail for frame ${frame.id}: ${err}`);
                    }
                }
            }
        };

        loadThumbnails();
    }, [timeline]);

    // Handle timeline scrubbing
    const handleScrub = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
        const time = parseInt(e.target.value, 10);
        setCurrentTime(time);

        // Find nearest frame
        if (timeline) {
            const nearestFrame = timeline.frames.reduce((prev, curr) =>
                Math.abs(curr.timestamp_ms - time) < Math.abs(prev.timestamp_ms - time) ? curr : prev
            );
            setSelectedFrame(nearestFrame);
        }
    }, [timeline]);

    // Transcripts visible at current time are filtered for display below

    // Find current transcript
    const currentTranscript = timeline?.transcripts.find((t) => {
        const start = t.timestamp_ms;
        const end = start + (t.duration_seconds * 1000);
        return currentTime >= start && currentTime <= end;
    });

    // Format time as MM:SS
    const formatTime = (ms: number) => {
        const seconds = Math.floor(ms / 1000);
        const mins = Math.floor(seconds / 60);
        const secs = seconds % 60;
        return `${mins}:${secs.toString().padStart(2, "0")}`;
    };

    // Calculate max time
    const maxTime = timeline ? Math.max(
        ...timeline.frames.map((f) => f.timestamp_ms),
        ...timeline.transcripts.map((t) => t.timestamp_ms + t.duration_seconds * 1000),
        1000
    ) : 1000;

    if (!meetingId) {
        return (
            <div className="rewind-empty">
                <div className="empty-state">
                    <div className="empty-state-icon">🎬</div>
                    <p className="empty-state-text">Select a past meeting to review</p>
                </div>
            </div>
        );
    }

    if (isLoading && !timeline) {
        return (
            <div className="rewind-loading">
                <div className="loading-spinner"></div>
                <p>Loading meeting timeline...</p>
            </div>
        );
    }

    return (
        <div className="rewind-gallery">
            {/* Top section: Frame preview + Transcripts */}
            <div className="rewind-main">
                {/* Frame preview */}
                <div className="rewind-frame-preview">
                    {frameImage ? (
                        <img src={frameImage} alt="Frame preview" />
                    ) : (
                        <div className="frame-placeholder">
                            <span>📷</span>
                            <p>No frame selected</p>
                        </div>
                    )}
                    <div className="frame-timestamp">
                        {selectedFrame && formatTime(selectedFrame.timestamp_ms)}
                    </div>
                </div>

                {/* Transcript panel */}
                <div className="rewind-transcripts scrollable" ref={transcriptRef}>
                    <h3>Transcripts</h3>
                    {timeline?.transcripts.length === 0 ? (
                        <p className="no-transcripts">No transcripts yet</p>
                    ) : (
                        <div className="transcript-entries">
                            {timeline?.transcripts.map((t) => (
                                <div
                                    key={t.id}
                                    className={`transcript-entry ${t.id === currentTranscript?.id ? "active" : ""}`}
                                    onClick={() => {
                                        setCurrentTime(t.timestamp_ms);
                                        // Find frame at this time
                                        const nearestFrame = timeline.frames.reduce((prev, curr) =>
                                            Math.abs(curr.timestamp_ms - t.timestamp_ms) < Math.abs(prev.timestamp_ms - t.timestamp_ms) ? curr : prev
                                        );
                                        setSelectedFrame(nearestFrame);
                                    }}
                                >
                                    <span className="entry-time">{formatTime(t.timestamp_ms)}</span>
                                    <span className="entry-speaker">{t.speaker || "Speaker"}</span>
                                    <p className="entry-text">{t.text}</p>
                                </div>
                            ))}
                        </div>
                    )}
                </div>
            </div>

            {/* Timeline scrubber */}
            <div className="rewind-timeline">
                <span className="timeline-time">{formatTime(0)}</span>
                <div className="timeline-track">
                    <input
                        type="range"
                        min={0}
                        max={maxTime}
                        value={currentTime}
                        onChange={handleScrub}
                        className="timeline-slider"
                    />
                    {/* Frame markers */}
                    <div className="timeline-markers">
                        {timeline?.frames.map((f) => (
                            <div
                                key={f.id}
                                className="timeline-marker frame-marker"
                                style={{ left: `${(f.timestamp_ms / maxTime) * 100}%` }}
                                title={`Frame at ${formatTime(f.timestamp_ms)}`}
                            />
                        ))}
                        {/* Transcript markers */}
                        {timeline?.transcripts.filter(t => t.is_final).map((t) => (
                            <div
                                key={t.id}
                                className="timeline-marker transcript-marker"
                                style={{ left: `${(t.timestamp_ms / maxTime) * 100}%` }}
                                title={t.text.slice(0, 50)}
                            />
                        ))}
                    </div>
                </div>
                <span className="timeline-time">{formatTime(maxTime)}</span>
            </div>

            {/* Thumbnail gallery */}
            <div className="rewind-thumbnails scrollable" ref={galleryRef}>
                {timeline?.frames.map((frame) => (
                    <div
                        key={frame.id}
                        className={`thumbnail ${frame.id === selectedFrame?.id ? "selected" : ""}`}
                        onClick={() => {
                            setSelectedFrame(frame);
                            setCurrentTime(frame.timestamp_ms);
                        }}
                    >
                        {thumbnails.has(frame.id) ? (
                            <img src={thumbnails.get(frame.id)} alt={`Frame ${frame.frame_number}`} />
                        ) : (
                            <div className="thumbnail-loading">
                                <span>🖼️</span>
                            </div>
                        )}
                        <span className="thumbnail-time">{formatTime(frame.timestamp_ms)}</span>
                    </div>
                ))}
            </div>

            {/* Stats bar */}
            <div className="rewind-stats">
                <span>📷 {timeline?.frames.length || 0} frames</span>
                <span>💬 {timeline?.transcripts.length || 0} transcripts</span>
                <span>⏱️ {formatTime(maxTime)} duration</span>
            </div>
        </div>
    );
}
