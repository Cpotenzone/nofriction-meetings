import React from 'react';
import { LiveTranscriptView } from '../../LiveTranscript';
import { useRecording } from '../../../hooks/useRecording';
import { useTranscripts } from '../../../hooks/useTranscripts';

interface FlowStateViewProps {
    recording: ReturnType<typeof useRecording>;
    transcripts: ReturnType<typeof useTranscripts>;
}

export const FlowStateView: React.FC<FlowStateViewProps> = ({ recording, transcripts }) => {
    return (
        <div className="agency-view flow-state">
            <div className="flow-content">
                {/* Main Transcript Area */}
                <div className="flow-transcript-container">
                    <LiveTranscriptView
                        isRecording={recording.isRecording}
                        transcripts={transcripts.liveTranscripts}
                    />
                </div>

                {/* Right Panel: Real-time Intelligence */}
                <aside className="flow-intelligence-panel">
                    <div className="panel-header">
                        <h3>LIVE INTELLIGENCE</h3>
                        <div className="live-indicator">
                            <span className="pulse-dot"></span>
                            ACTIVE
                        </div>
                    </div>

                    <div className="intelligence-stream">
                        <div className="placeholder-card">
                            <span className="icon">💡</span>
                            <p>Real-time insights will appear here...</p>
                        </div>
                    </div>
                </aside>
            </div>
        </div>
    );
};
