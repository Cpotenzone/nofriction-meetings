import React, { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface GenieViewProps {
    onRestore: () => void;
    liveTranscripts: string[];
    isRecording: boolean;
    onStop: () => void;
}

interface GenieInsight {
    type: 'context' | 'action' | 'connection';
    text: string;
}

export const GenieView: React.FC<GenieViewProps> = ({ onRestore, liveTranscripts, isRecording, onStop }) => {
    const [insight, setInsight] = useState<GenieInsight | null>(null);
    const [isAnalyzing, setIsAnalyzing] = useState(false);
    const lastInsightRef = useRef<string>("");

    // Get the last 2 transcripts for display
    const recentTranscripts = liveTranscripts.slice(-2);

    // Generate AI insight periodically based on recent transcripts
    useEffect(() => {
        // Only generate insights if we have enough transcripts and not already analyzing
        if (liveTranscripts.length < 3 || isAnalyzing) return;

        const latestText = liveTranscripts.slice(-3).join(" ");
        // Avoid re-analyzing the same content
        if (latestText === lastInsightRef.current) return;
        lastInsightRef.current = latestText;

        // Generate insight (mock AI for now - can be replaced with real API call)
        generateInsight(latestText);
    }, [liveTranscripts, isAnalyzing]);

    const generateInsight = async (context: string) => {
        setIsAnalyzing(true);

        // Simulate AI thinking time
        await new Promise(resolve => setTimeout(resolve, 1500));

        // Smart insight generation based on keywords
        const insights: GenieInsight[] = [];

        // Check for action items
        if (/should|need to|have to|must|will|action|task|todo/i.test(context)) {
            insights.push({
                type: 'action',
                text: '📋 Potential action item detected. Consider noting this decision.'
            });
        }

        // Check for questions
        if (/\?|how|what|when|where|why|who/i.test(context)) {
            insights.push({
                type: 'context',
                text: '❓ Discussion topic identified. Related meeting context may be relevant.'
            });
        }

        // Check for agreements/decisions
        if (/agree|decide|confirm|approve|ok|yes|let's/i.test(context)) {
            insights.push({
                type: 'connection',
                text: '✅ Decision point reached. This may connect to previous discussions.'
            });
        }

        // Check for important topics
        if (/important|critical|priority|urgent|key|main/i.test(context)) {
            insights.push({
                type: 'context',
                text: '⚡ High-priority topic flagged for attention.'
            });
        }

        // Default insight if nothing specific
        if (insights.length === 0) {
            insights.push({
                type: 'context',
                text: '💭 Monitoring conversation flow for insights...'
            });
        }

        // Pick a relevant insight
        setInsight(insights[0]);
        setIsAnalyzing(false);
    };

    const formatTime = () => {
        const now = new Date();
        return now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
    };

    const handleRestore = async () => {
        const { setGenieMode } = await import("../lib/tauri");
        await setGenieMode(false);
        onRestore();
    };

    return (
        <motion.div
            className="genie-container"
            initial={{ opacity: 0, scale: 0.8 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.8 }}
            transition={{ duration: 0.3, ease: "easeOut" }}
        >
            <div className="genie-glass genie-enhanced">
                {/* Header */}
                <div className="genie-header" data-tauri-drag-region>
                    <div className={`genie-status ${isRecording ? 'recording' : ''}`} />
                    <span>GENIE MODE</span>
                    <div className="genie-header-right">
                        <span className="genie-time">{formatTime()}</span>
                    </div>
                </div>

                {/* Main Content: Latest Transcripts */}
                <div className="genie-main-content">
                    {recentTranscripts.length === 0 ? (
                        <div className="genie-awaiting">
                            <div className="genie-pulse-icon">✦</div>
                            <span>Listening for intelligence...</span>
                        </div>
                    ) : (
                        <div className="genie-transcript-display">
                            <AnimatePresence mode="popLayout">
                                {recentTranscripts.map((text, index) => (
                                    <motion.div
                                        key={`${liveTranscripts.length - 2 + index}-${text.slice(0, 15)}`}
                                        className={`genie-sentence ${index === recentTranscripts.length - 1 ? 'current' : 'previous'}`}
                                        initial={{ opacity: 0, y: 15 }}
                                        animate={{ opacity: 1, y: 0 }}
                                        exit={{ opacity: 0, y: -15 }}
                                        transition={{ duration: 0.3 }}
                                    >
                                        <span className="sentence-marker">{index === recentTranscripts.length - 1 ? '▸' : '▹'}</span>
                                        <p>{text}</p>
                                    </motion.div>
                                ))}
                            </AnimatePresence>
                        </div>
                    )}
                </div>

                {/* AI Insight Panel */}
                <div className="genie-insight-panel">
                    <div className="insight-header">
                        <span className="insight-label">AI INSIGHT</span>
                        {isAnalyzing && <span className="insight-analyzing">analyzing...</span>}
                    </div>
                    <AnimatePresence mode="wait">
                        {insight ? (
                            <motion.div
                                key={insight.text}
                                className={`insight-content insight-${insight.type}`}
                                initial={{ opacity: 0, x: -10 }}
                                animate={{ opacity: 1, x: 0 }}
                                exit={{ opacity: 0, x: 10 }}
                                transition={{ duration: 0.3 }}
                            >
                                {insight.text}
                            </motion.div>
                        ) : (
                            <motion.div
                                className="insight-content insight-idle"
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 0.6 }}
                            >
                                Waiting for enough context...
                            </motion.div>
                        )}
                    </AnimatePresence>
                </div>

                {/* Footer with Controls */}
                <div className="genie-footer">
                    <div className="genie-footer-stats">
                        <span className="genie-line-count">{liveTranscripts.length} captured</span>
                    </div>

                    <div className="genie-controls">
                        {isRecording && (
                            <motion.button
                                className="genie-stop-btn"
                                whileHover={{ scale: 1.05 }}
                                whileTap={{ scale: 0.95 }}
                                onClick={onStop}
                            >
                                ■ STOP
                            </motion.button>
                        )}

                        <motion.div
                            className="genie-restore-btn"
                            whileHover={{ scale: 1.08 }}
                            whileTap={{ scale: 0.92 }}
                            onClick={handleRestore}
                        >
                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                                <polyline points="15 3 21 3 21 9" />
                                <polyline points="9 21 3 21 3 15" />
                                <line x1="21" y1="3" x2="14" y2="10" />
                                <line x1="3" y1="21" x2="10" y2="14" />
                            </svg>
                            <span>Expand</span>
                        </motion.div>
                    </div>
                </div>
            </div>
        </motion.div>
    );
};
