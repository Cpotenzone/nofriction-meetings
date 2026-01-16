// noFriction Meetings - Live Transcript Component
// Near real-time paragraph-based transcript display

import { useEffect, useRef, useMemo } from "react";
import type { LiveTranscript } from "../hooks/useTranscripts";

interface LiveTranscriptProps {
    transcripts: LiveTranscript[];
    isRecording: boolean;
}

interface SpeakerBlock {
    id: string;
    speaker: string;
    initials: string;
    color: string;
    timestamp: Date;
    paragraphs: string[];
    currentInterim: string | null;
    isFinal: boolean;
}

// Generate consistent colors for speakers
const SPEAKER_COLORS = [
    "#8B5CF6", // Purple
    "#3B82F6", // Blue
    "#10B981", // Green
    "#F59E0B", // Amber
    "#EF4444", // Red
    "#EC4899", // Pink
    "#6366F1", // Indigo
    "#14B8A6", // Teal
];

function getSpeakerColor(speaker: string): string {
    const hash = speaker.split("").reduce((acc, char) => acc + char.charCodeAt(0), 0);
    return SPEAKER_COLORS[hash % SPEAKER_COLORS.length];
}

function getInitials(speaker: string): string {
    if (!speaker) return "?";
    if (speaker.startsWith("Speaker ")) {
        return `S${speaker.split(" ")[1]}`;
    }
    const parts = speaker.split(" ");
    if (parts.length >= 2) {
        return `${parts[0][0]}${parts[1][0]}`.toUpperCase();
    }
    return speaker.slice(0, 2).toUpperCase();
}

export function LiveTranscriptView({ transcripts, isRecording }: LiveTranscriptProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const autoScrollRef = useRef(true);

    // Group transcripts into speaker blocks with live interim support
    const speakerBlocks = useMemo(() => {
        const blocks: SpeakerBlock[] = [];
        let currentBlock: SpeakerBlock | null = null;

        for (const t of transcripts) {
            const speaker = t.speaker || "Speaker";

            // Start new block if different speaker or too much time passed
            const shouldStartNewBlock = !currentBlock ||
                currentBlock.speaker !== speaker ||
                t.timestamp.getTime() - currentBlock.timestamp.getTime() > 15000; // 15 second window

            if (shouldStartNewBlock) {
                if (currentBlock) {
                    blocks.push(currentBlock);
                }
                currentBlock = {
                    id: t.id,
                    speaker,
                    initials: getInitials(speaker),
                    color: getSpeakerColor(speaker),
                    timestamp: t.timestamp,
                    paragraphs: [],
                    currentInterim: null,
                    isFinal: t.isFinal,
                };
            }

            if (t.isFinal) {
                currentBlock!.paragraphs.push(t.text);
                currentBlock!.currentInterim = null;
                currentBlock!.isFinal = true;
            } else {
                // Update interim - this is the "live" text being spoken
                currentBlock!.currentInterim = t.text;
            }
        }

        if (currentBlock) {
            blocks.push(currentBlock);
        }

        return blocks;
    }, [transcripts]);

    // Auto-scroll to bottom immediately when transcripts change
    useEffect(() => {
        if (autoScrollRef.current && containerRef.current) {
            // Use requestAnimationFrame for smooth scrolling
            requestAnimationFrame(() => {
                if (containerRef.current) {
                    containerRef.current.scrollTop = containerRef.current.scrollHeight;
                }
            });
        }
    }, [transcripts]); // React to every transcript change for responsiveness

    // Detect manual scroll to pause auto-scroll
    const handleScroll = () => {
        if (!containerRef.current) return;
        const { scrollTop, scrollHeight, clientHeight } = containerRef.current;
        autoScrollRef.current = scrollHeight - scrollTop - clientHeight < 50;
    };

    const formatTime = (date: Date) => {
        return date.toLocaleTimeString("en-US", {
            hour: "numeric",
            minute: "2-digit",
            hour12: true,
        });
    };

    if (speakerBlocks.length === 0) {
        return (
            <div className="empty-state">
                <div className="empty-state-icon">🎙️</div>
                <p className="empty-state-text">
                    {isRecording
                        ? "Listening... Start speaking to see live transcription"
                        : "Start recording to capture live transcription"}
                </p>
                {isRecording && (
                    <div className="listening-indicator">
                        <span className="pulse-dot"></span>
                        <span className="pulse-dot"></span>
                        <span className="pulse-dot"></span>
                    </div>
                )}
            </div>
        );
    }

    return (
        <div
            ref={containerRef}
            className="transcript-conversation"
            onScroll={handleScroll}
        >
            {speakerBlocks.map((block) => (
                <div
                    key={block.id}
                    className="speaker-block"
                >
                    {/* Avatar */}
                    <div
                        className="speaker-avatar"
                        style={{ backgroundColor: block.color }}
                    >
                        {block.initials}
                    </div>

                    {/* Content */}
                    <div className="speaker-content">
                        {/* Header */}
                        <div className="speaker-header">
                            <span className="speaker-name">{block.speaker}</span>
                            <span className="speaker-time">{formatTime(block.timestamp)}</span>
                        </div>

                        {/* Finalized paragraphs */}
                        <div className="speaker-text">
                            {block.paragraphs.map((para, idx) => (
                                <span key={idx} className="final-text">
                                    {para}{" "}
                                </span>
                            ))}

                            {/* Live interim text - shown inline */}
                            {block.currentInterim && (
                                <span className="interim-text">
                                    {block.currentInterim}
                                    <span className="typing-cursor">|</span>
                                </span>
                            )}
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
}
