import React from 'react';
import { motion } from 'framer-motion';
import { emit } from '@tauri-apps/api/event';
import { AgencyMode } from './AgencyLayout';

interface AgencyNavbarProps {
    activeMode: AgencyMode;
    onModeChange: (mode: AgencyMode) => void;
    isRecording: boolean;
    onToggleRecording: () => void;
    onOpenSettings: () => void;
}

export const AgencyNavbar: React.FC<AgencyNavbarProps> = ({
    activeMode,
    onModeChange,
    isRecording,
    onToggleRecording,
    onOpenSettings
}) => {
    const handleEnterGenie = async () => {
        await emit('enter-genie-mode');
    };

    return (
        <nav className="agency-navbar">
            <div className="agency-nav-left">
                <div className="agency-logo">
                    <span className="logo-icon">⚡️</span>
                    <span className="logo-text">NOFRICTION</span>
                </div>

                <div className="agency-status-pill">
                    <div className={`status-dot ${isRecording ? 'recording' : 'idle'}`} />
                    <span className="status-text">
                        {isRecording ? 'LIVE INTELLIGENCE ACTIVE' : 'SYSTEM READY'}
                    </span>
                </div>
            </div>

            <div className="agency-nav-center">
                <div className="agency-mode-switcher">
                    <button
                        className={`mode-btn ${activeMode === 'flow' ? 'active' : ''}`}
                        onClick={() => onModeChange('flow')}
                    >
                        <span className="mode-icon">🌊</span>
                        FLOW
                    </button>
                    <button
                        className={`mode-btn ${activeMode === 'deck' ? 'active' : ''}`}
                        onClick={() => onModeChange('deck')}
                    >
                        <span className="mode-icon">🧠</span>
                        DECK
                    </button>
                    <button
                        className={`mode-btn ${activeMode === 'zen' ? 'active' : ''}`}
                        onClick={() => onModeChange('zen')}
                    >
                        <span className="mode-icon">🧘</span>
                        ZEN
                    </button>
                    <button
                        className={`mode-btn ${activeMode === 'vault' ? 'active' : ''}`}
                        onClick={() => onModeChange('vault')}
                    >
                        <span className="mode-icon">📚</span>
                        VAULT
                    </button>
                </div>
            </div>

            <div className="agency-nav-right">
                {isRecording && (
                    <motion.button
                        className="agency-genie-btn"
                        onClick={handleEnterGenie}
                        whileHover={{ scale: 1.05 }}
                        whileTap={{ scale: 0.95 }}
                        title="Enter Genie Mode (minimal overlay)"
                    >
                        ✨ GENIE
                    </motion.button>
                )}

                <motion.button
                    className={`agency-action-btn ${isRecording ? 'recording' : ''}`}
                    onClick={onToggleRecording}
                    whileHover={{ scale: 1.05 }}
                    whileTap={{ scale: 0.95 }}
                >
                    {isRecording ? 'STOP CAPTURE' : 'START CAPTURE'}
                </motion.button>

                <button className="agency-icon-btn" onClick={onOpenSettings}>
                    ⚙️
                </button>
            </div>
        </nav>
    );
};
