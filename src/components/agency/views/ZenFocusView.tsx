import React from 'react';
import { useRecording } from '../../../hooks/useRecording';
import { motion } from 'framer-motion';

interface ZenFocusViewProps {
    recording: ReturnType<typeof useRecording>;
}

export const ZenFocusView: React.FC<ZenFocusViewProps> = ({ recording }) => {
    return (
        <div className="agency-view zen-focus">
            <div className="zen-container">
                <div className="zen-widget">
                    <div className={`zen-status-ring ${recording.isRecording ? 'recording' : 'idle'}`}>
                        <div className="zen-status-core" />
                    </div>

                    <div className="zen-controls">
                        <h2 className="zen-status-text">
                            {recording.isRecording ? 'RECORDING ACTIVE' : 'ZEN MODE'}
                        </h2>
                        <span className="zen-subtitle">
                            {recording.isRecording ? 'Capture in progress...' : 'Ready to capture'}
                        </span>

                        <motion.button
                            className="zen-primary-action"
                            onClick={recording.isRecording ? recording.stopRecording : recording.startRecording}
                            whileHover={{ scale: 1.05 }}
                            whileTap={{ scale: 0.95 }}
                        >
                            {recording.isRecording ? 'STOP' : 'START'}
                        </motion.button>
                    </div>
                </div>
            </div>
        </div>
    );
};
