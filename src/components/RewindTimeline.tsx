// noFriction Meetings - Rewind Timeline Component
// Visual timeline for scrubbing through captured frames

import { useCallback, useEffect, useRef, useState } from "react";
import type { Frame } from "../lib/tauri";
import * as tauri from "../lib/tauri";

interface RewindTimelineProps {
    meetingId: string;
    onFrameSelect?: (frame: Frame) => void;
}

export function RewindTimeline({ meetingId, onFrameSelect }: RewindTimelineProps) {
    const [frames, setFrames] = useState<Frame[]>([]);
    const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
    const [thumbnailCache, setThumbnailCache] = useState<Map<number, string>>(new Map());
    const [isLoading, setIsLoading] = useState(true);
    const [previewImage, setPreviewImage] = useState<string | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);

    // Load frames for the meeting
    useEffect(() => {
        const loadFrames = async () => {
            if (!meetingId) return;

            setIsLoading(true);
            try {
                const loadedFrames = await tauri.getFrames(meetingId, 1000);
                setFrames(loadedFrames);
                if (loadedFrames.length > 0) {
                    setSelectedIndex(loadedFrames.length - 1); // Start at latest
                }
            } catch (err) {
                console.error("Failed to load frames:", err);
            } finally {
                setIsLoading(false);
            }
        };

        loadFrames();
    }, [meetingId]);

    // Load thumbnail for a frame
    const loadThumbnail = useCallback(async (frame: Frame) => {
        if (thumbnailCache.has(frame.id)) {
            return thumbnailCache.get(frame.id);
        }

        try {
            const thumbnail = await tauri.getFrameThumbnail(String(frame.id));
            if (thumbnail) {
                setThumbnailCache(prev => new Map(prev).set(frame.id, thumbnail));
                return thumbnail;
            }
        } catch (err) {
            console.error("Failed to load thumbnail:", err);
        }
        return null;
    }, [meetingId, thumbnailCache]);

    // Load preview image when selected frame changes
    useEffect(() => {
        if (selectedIndex === null || !frames[selectedIndex]) return;

        const loadPreview = async () => {
            const thumbnail = await loadThumbnail(frames[selectedIndex]);
            setPreviewImage(thumbnail || null);
            if (onFrameSelect) {
                onFrameSelect(frames[selectedIndex]);
            }
        };

        loadPreview();
    }, [selectedIndex, frames, loadThumbnail, onFrameSelect]);

    const handleFrameClick = (index: number) => {
        setSelectedIndex(index);
    };

    const formatTime = (timestamp: string) => {
        const date = new Date(timestamp);
        return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    };

    const getDuration = () => {
        if (frames.length < 2) return null;
        const first = new Date(frames[0].timestamp);
        const last = new Date(frames[frames.length - 1].timestamp);
        const diffMs = last.getTime() - first.getTime();
        const minutes = Math.floor(diffMs / 60000);
        const seconds = Math.floor((diffMs % 60000) / 1000);
        return `${minutes}:${seconds.toString().padStart(2, "0")}`;
    };

    if (isLoading) {
        return (
            <div className="rewind-timeline glass-panel">
                <div className="timeline-loading">
                    <div className="spinner"></div>
                    <span>Loading timeline...</span>
                </div>
            </div>
        );
    }

    if (frames.length === 0) {
        return (
            <div className="rewind-timeline glass-panel">
                <div className="timeline-empty">
                    <span>No frames captured yet</span>
                    <p>Start recording to capture screen frames</p>
                </div>
            </div>
        );
    }

    const [zoom, setZoom] = useState(1);
    const [pan, setPan] = useState({ x: 0, y: 0 });
    const [isDragging, setIsDragging] = useState(false);
    const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

    // Reset zoom when frame changes
    useEffect(() => {
        setZoom(1);
        setPan({ x: 0, y: 0 });
    }, [selectedIndex]);

    const handleWheel = (e: React.WheelEvent) => {
        if (e.ctrlKey || e.metaKey) { // Pinch gesture or Ctrl+Wheel
            e.preventDefault();
            const delta = -e.deltaY * 0.01;
            setZoom(z => Math.min(Math.max(1, z + delta), 5)); // Limit zoom 1x to 5x
        }
    };

    const handleMouseDown = (e: React.MouseEvent) => {
        if (zoom > 1) {
            setIsDragging(true);
            setDragStart({ x: e.clientX - pan.x, y: e.clientY - pan.y });
        }
    };

    const handleMouseMove = (e: React.MouseEvent) => {
        if (isDragging && zoom > 1) {
            setPan({
                x: e.clientX - dragStart.x,
                y: e.clientY - dragStart.y
            });
        }
    };

    const handleMouseUp = () => {
        setIsDragging(false);
    };

    // ... (rest of component)

    return (
        <div className="rewind-timeline glass-panel">
            {/* Header ... */}
            <div className="timeline-header">
                <h3>🎬 Rewind</h3>
                <div className="timeline-stats">
                    {zoom > 1 && (
                        <button
                            className="btn-tiny"
                            onClick={() => { setZoom(1); setPan({ x: 0, y: 0 }); }}
                            style={{
                                background: '#3b82f6', color: 'white', border: 'none',
                                borderRadius: '4px', padding: '2px 6px', fontSize: '10px',
                                marginRight: '8px', cursor: 'pointer'
                            }}
                        >
                            Reset Zoom ({(zoom * 100).toFixed(0)}%)
                        </button>
                    )}
                    <span>{frames.length} frames</span>
                    {getDuration() && <span> • {getDuration()}</span>}
                </div>
            </div>

            {/* Preview area */}
            <div
                className="timeline-preview"
                onWheel={handleWheel}
                onMouseDown={handleMouseDown}
                onMouseMove={handleMouseMove}
                onMouseUp={handleMouseUp}
                onMouseLeave={handleMouseUp}
                style={{ overflow: 'hidden', cursor: zoom > 1 ? (isDragging ? 'grabbing' : 'grab') : 'default' }}
            >
                {previewImage ? (
                    <img
                        src={previewImage}
                        alt="Frame preview"
                        style={{
                            transform: `scale(${zoom}) translate(${pan.x / zoom}px, ${pan.y / zoom}px)`,
                            transition: isDragging ? 'none' : 'transform 0.1s ease-out',
                            transformOrigin: 'center center',
                            pointerEvents: 'none' // Let events bubble to container
                        }}
                    />
                ) : (
                    <div className="preview-placeholder">
                        <span>Select a frame to preview</span>
                    </div>
                )}
                {selectedIndex !== null && frames[selectedIndex] && (
                    <div className="preview-timestamp">
                        {formatTime(frames[selectedIndex].timestamp)}
                    </div>
                )}
            </div>

            {/* Timeline scrubber */}
            <div className="timeline-scrubber" ref={containerRef}>
                <div className="timeline-track">
                    {frames.map((frame, index) => (
                        <div
                            key={frame.id}
                            className={`timeline-frame ${index === selectedIndex ? "selected" : ""}`}
                            onClick={() => handleFrameClick(index)}
                            title={formatTime(frame.timestamp)}
                        />
                    ))}
                </div>
            </div>

            {/* Time markers */}
            <div className="timeline-markers">
                <span>{frames.length > 0 ? formatTime(frames[0].timestamp) : ""}</span>
                <span>{frames.length > 0 ? formatTime(frames[frames.length - 1].timestamp) : ""}</span>
            </div>
        </div>
    );
}
