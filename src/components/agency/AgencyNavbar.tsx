import React, { useState, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { emit } from '@tauri-apps/api/event';
import { AgencyMode } from './AgencyLayout';

interface AgencyNavbarProps {
    activeMode: AgencyMode;
    onModeChange: (mode: AgencyMode) => void;
    isRecording: boolean;
    onToggleRecording: () => void;
    onOpenSettings: () => void;
}

// Primary modes: always visible
const PRIMARY_MODES: { mode: AgencyMode; icon: string; label: string }[] = [
    { mode: 'flow', icon: '🌊', label: 'FLOW' },
    { mode: 'intel', icon: '🔍', label: 'INTEL' },
    { mode: 'vault', icon: '📚', label: 'VAULT' },
    { mode: 'chat', icon: '💬', label: 'CHAT' },
];

// Secondary modes: in overflow menu
const SECONDARY_MODES: { mode: AgencyMode; icon: string; label: string }[] = [
    { mode: 'deck', icon: '🧠', label: 'DECK' },
    { mode: 'zen', icon: '🧘', label: 'ZEN' },
    { mode: 'prompts', icon: '🧠', label: 'PROMPTS' },
    { mode: 'help', icon: '📖', label: 'HELP' },
];

export const AgencyNavbar: React.FC<AgencyNavbarProps> = ({
    activeMode,
    onModeChange,
    isRecording,
    onToggleRecording,
    onOpenSettings
}) => {
    const [showMore, setShowMore] = useState(false);
    const moreRef = useRef<HTMLDivElement>(null);

    const handleEnterGenie = async () => {
        await emit('enter-genie-mode');
    };

    // Close overflow on outside click
    useEffect(() => {
        const handler = (e: MouseEvent) => {
            if (moreRef.current && !moreRef.current.contains(e.target as Node)) {
                setShowMore(false);
            }
        };
        document.addEventListener('mousedown', handler);
        return () => document.removeEventListener('mousedown', handler);
    }, []);

    const isSecondary = SECONDARY_MODES.some(m => m.mode === activeMode);

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
                    {/* Primary modes — always visible */}
                    {PRIMARY_MODES.map(({ mode, icon, label }) => (
                        <button
                            key={mode}
                            className={`mode-btn ${activeMode === mode ? 'active' : ''}`}
                            onClick={() => onModeChange(mode)}
                        >
                            <span className="mode-icon">{icon}</span>
                            {label}
                        </button>
                    ))}

                    {/* Overflow menu for secondary modes */}
                    <div className="mode-overflow" ref={moreRef}>
                        <button
                            className={`mode-btn mode-btn-more ${isSecondary ? 'active' : ''}`}
                            onClick={() => setShowMore(!showMore)}
                            title="More views"
                        >
                            <span className="mode-icon">•••</span>
                            MORE
                        </button>
                        <AnimatePresence>
                            {showMore && (
                                <motion.div
                                    className="mode-overflow-menu"
                                    initial={{ opacity: 0, y: -8, scale: 0.95 }}
                                    animate={{ opacity: 1, y: 0, scale: 1 }}
                                    exit={{ opacity: 0, y: -8, scale: 0.95 }}
                                    transition={{ duration: 0.15 }}
                                >
                                    {SECONDARY_MODES.map(({ mode, icon, label }) => (
                                        <button
                                            key={mode}
                                            className={`overflow-item ${activeMode === mode ? 'active' : ''}`}
                                            onClick={() => {
                                                onModeChange(mode);
                                                setShowMore(false);
                                            }}
                                        >
                                            <span className="overflow-icon">{icon}</span>
                                            {label}
                                        </button>
                                    ))}
                                </motion.div>
                            )}
                        </AnimatePresence>
                    </div>
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

