import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface TranscriptSegment {
    text: string;
    is_final: boolean;
    confidence: number;
    start: number;
    duration: number;
    speaker: string | null;
}

interface ProviderMetrics {
    provider: string;
    transcripts: TranscriptSegment[];
    firstWordTime: number | null;
    totalWords: number;
    avgConfidence: number;
    isActive: boolean;
}

const PROVIDERS = [
    { id: 'deepgram', name: 'Deepgram', icon: '🎙️' },
    { id: 'gemini', name: 'Gemini Live', icon: '✨' },
    { id: 'gladia', name: 'Gladia', icon: '🎧' },
    { id: 'google_stt', name: 'Google Cloud STT', icon: '☁️' },
];

export default function ComparisonLab() {
    const [selectedProviders, setSelectedProviders] = useState<string[]>(['deepgram', 'gemini']);
    const [isRecording, setIsRecording] = useState(false);
    const [metrics, setMetrics] = useState<Map<string, ProviderMetrics>>(new Map());
    const [startTime, setStartTime] = useState<number | null>(null);

    useEffect(() => {
        // Listen for transcripts from all providers
        const unlisten = listen<TranscriptSegment>('live_transcript', (event) => {
            const segment = event.payload;
            const now = Date.now();

            setMetrics(prev => {
                const newMetrics = new Map(prev);
                selectedProviders.forEach(provider => {
                    const current = newMetrics.get(provider) || {
                        provider,
                        transcripts: [],
                        firstWordTime: null,
                        totalWords: 0,
                        avgConfidence: 0,
                        isActive: false,
                    };

                    // Add transcript
                    current.transcripts.push(segment);

                    // Update metrics
                    if (!current.firstWordTime && segment.text.trim().length > 0) {
                        current.firstWordTime = now - (startTime || now);
                    }

                    const words = segment.text.split(/\s+/).filter(w => w.length > 0);
                    current.totalWords += words.length;

                    // Calculate average confidence
                    const allConfidences = current.transcripts.map(t => t.confidence);
                    current.avgConfidence = allConfidences.reduce((a, b) => a + b, 0) / allConfidences.length;

                    newMetrics.set(provider, current);
                });
                return newMetrics;
            });
        });

        return () => {
            unlisten.then(fn => fn());
        };
    }, [selectedProviders, startTime]);

    const toggleProvider = (providerId: string) => {
        if (isRecording) return;

        setSelectedProviders(prev => {
            if (prev.includes(providerId)) {
                return prev.filter(p => p !== providerId);
            } else if (prev.length < 3) {
                return [...prev, providerId];
            }
            return prev;
        });
    };

    const startComparison = async () => {
        if (selectedProviders.length < 2) {
            alert('Please select at least 2 providers to compare');
            return;
        }

        try {
            // Initialize metrics
            const newMetrics = new Map<string, ProviderMetrics>();
            selectedProviders.forEach(provider => {
                newMetrics.set(provider, {
                    provider,
                    transcripts: [],
                    firstWordTime: null,
                    totalWords: 0,
                    avgConfidence: 0,
                    isActive: true,
                });
            });
            setMetrics(newMetrics);
            setStartTime(Date.now());

            // Start recording with all selected providers
            // Note: This would require backend support for multi-provider recording
            // For now, we'll start with the first provider
            await invoke('start_recording');
            setIsRecording(true);
        } catch (error) {
            console.error('Failed to start comparison:', error);
            alert('Failed to start comparison test');
        }
    };

    const stopComparison = async () => {
        try {
            await invoke('stop_recording');
            setIsRecording(false);
        } catch (error) {
            console.error('Failed to stop comparison:', error);
        }
    };

    const exportResults = () => {
        const results = {
            timestamp: new Date().toISOString(),
            duration: startTime ? (Date.now() - startTime) / 1000 : 0,
            providers: Array.from(metrics.entries()).map(([provider, data]) => ({
                provider,
                firstWordLatency: data.firstWordTime,
                totalWords: data.totalWords,
                avgConfidence: data.avgConfidence,
                transcripts: data.transcripts,
            })),
        };

        const blob = new Blob([JSON.stringify(results, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `comparison-${Date.now()}.json`;
        a.click();
        URL.revokeObjectURL(url);
    };

    return (
        <div className="comparison-lab">
            <div className="lab-header">
                <div>
                    <h2>Transcription Comparison Lab</h2>
                    <p className="lab-subtitle">Test multiple providers side-by-side</p>
                </div>
                <div className="lab-actions">
                    {!isRecording ? (
                        <button
                            className="btn-primary"
                            onClick={startComparison}
                            disabled={selectedProviders.length < 2}
                        >
                            ▶️ Start Test Recording
                        </button>
                    ) : (
                        <>
                            <button className="btn-danger" onClick={stopComparison}>
                                ⏹️ Stop Recording
                            </button>
                            <button className="btn-secondary" onClick={exportResults}>
                                📥 Export Results
                            </button>
                        </>
                    )}
                </div>
            </div>

            <div className="provider-selection">
                <h3>Select Providers (2-3)</h3>
                <div className="provider-grid">
                    {PROVIDERS.map(provider => (
                        <button
                            key={provider.id}
                            className={`provider-select-card ${selectedProviders.includes(provider.id) ? 'selected' : ''} ${isRecording ? 'disabled' : ''}`}
                            onClick={() => toggleProvider(provider.id)}
                            disabled={isRecording}
                        >
                            <span className="provider-icon">{provider.icon}</span>
                            <span className="provider-name">{provider.name}</span>
                            {selectedProviders.includes(provider.id) && (
                                <span className="check-badge">✓</span>
                            )}
                        </button>
                    ))}
                </div>
            </div>

            {isRecording && (
                <div className="recording-indicator">
                    <span className="rec-dot"></span>
                    Recording in progress...
                </div>
            )}

            <div className="comparison-grid">
                {selectedProviders.map(providerId => {
                    const provider = PROVIDERS.find(p => p.id === providerId);
                    const data = metrics.get(providerId);

                    return (
                        <div key={providerId} className="comparison-panel">
                            <div className="panel-header">
                                <div className="panel-title">
                                    <span className="provider-icon">{provider?.icon}</span>
                                    <h3>{provider?.name}</h3>
                                </div>
                                {data?.isActive && (
                                    <span className="status-badge active">Active</span>
                                )}
                            </div>

                            <div className="metrics-row">
                                <div className="metric">
                                    <span className="metric-label">First Word</span>
                                    <span className="metric-value">
                                        {data?.firstWordTime ? `${(data.firstWordTime / 1000).toFixed(2)}s` : '-'}
                                    </span>
                                </div>
                                <div className="metric">
                                    <span className="metric-label">Total Words</span>
                                    <span className="metric-value">{data?.totalWords || 0}</span>
                                </div>
                                <div className="metric">
                                    <span className="metric-label">Avg Confidence</span>
                                    <span className="metric-value">
                                        {data?.avgConfidence ? `${(data.avgConfidence * 100).toFixed(1)}%` : '-'}
                                    </span>
                                </div>
                            </div>

                            <div className="transcript-display">
                                {data?.transcripts.length === 0 ? (
                                    <div className="empty-transcript">
                                        <p>Waiting for transcription...</p>
                                    </div>
                                ) : (
                                    <div className="transcript-content">
                                        {data?.transcripts.map((segment, idx) => (
                                            <div
                                                key={idx}
                                                className={`transcript-segment ${segment.is_final ? 'final' : 'interim'}`}
                                            >
                                                {segment.text}
                                            </div>
                                        ))}
                                    </div>
                                )}
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}
