import React, { useState, useRef, useEffect } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { emit } from '@tauri-apps/api/event';
import { AgencyMode } from './AgencyLayout';
import {
    LiveIcon,
    RewindIcon,
    RadarIcon,
    ChatIcon,
    VaultIcon,
    ZenIcon,
    PromptIcon,
    HelpIcon,
    GearIcon,
    SparkleIcon,
    SearchIcon,
    MoreIcon,
} from '../icons';

interface AgencyNavbarProps {
    activeMode: AgencyMode;
    onModeChange: (mode: AgencyMode) => void;
    isRecording: boolean;
    onToggleRecording: () => void;
    onOpenSettings: () => void;
    onOpenCommandPalette?: () => void;
}

// Primary modes: the product's core loop — record, rewind, understand, ask.
// Rewind (recordings library + playback) is the signature feature and must
// stay one click away; power-user views live in the overflow menu.
const PRIMARY_MODES: { mode: AgencyMode; icon: React.ReactNode; label: string }[] = [
    { mode: 'flow', icon: <LiveIcon size={15} />, label: 'LIVE' },
    { mode: 'deck', icon: <RewindIcon size={15} />, label: 'REWIND' },
    { mode: 'intel', icon: <RadarIcon size={15} />, label: 'INTEL' },
    { mode: 'chat', icon: <ChatIcon size={15} />, label: 'CHAT' },
];

// Secondary modes: in overflow menu
const SECONDARY_MODES: { mode: AgencyMode; icon: React.ReactNode; label: string }[] = [
    { mode: 'vault', icon: <VaultIcon size={15} />, label: 'VAULT' },
    { mode: 'zen', icon: <ZenIcon size={15} />, label: 'ZEN' },
    { mode: 'prompts', icon: <PromptIcon size={15} />, label: 'PROMPTS' },
    { mode: 'help', icon: <HelpIcon size={15} />, label: 'HELP' },
];

export const AgencyNavbar: React.FC<AgencyNavbarProps> = ({
    activeMode,
    onModeChange,
    isRecording,
    onToggleRecording,
    onOpenSettings,
    onOpenCommandPalette
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
                        {isRecording ? 'RECORDING' : 'READY'}
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
                            <span className="mode-icon"><MoreIcon size={15} /></span>
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
                {onOpenCommandPalette && (
                    <button
                        className="agency-cmdk-btn"
                        onClick={onOpenCommandPalette}
                        title="Search & commands (⌘K)"
                    >
                        <SearchIcon size={14} />
                        <kbd>⌘K</kbd>
                    </button>
                )}

                {isRecording && (
                    <motion.button
                        className="agency-genie-btn"
                        onClick={handleEnterGenie}
                        whileHover={{ scale: 1.05 }}
                        whileTap={{ scale: 0.95 }}
                        title="Enter Genie Mode (minimal overlay)"
                    >
                        <SparkleIcon size={14} /> GENIE
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

                <button className="agency-icon-btn" onClick={onOpenSettings} title="Settings" aria-label="Settings">
                    <GearIcon size={17} />
                </button>
            </div>
        </nav>
    );
};
