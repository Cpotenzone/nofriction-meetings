import { useState } from 'react';
import ThemeSelector from "./ThemeSelector";

interface SidebarProps {
    activeTab: string;
    onTabChange: (tab: string) => void;
    recording: any;
    onToggleRecording: () => void;
}

interface NavItem {
    id: string;
    icon: string;
    label: string;
    title: string;
}

const NAV_ITEMS: NavItem[] = [
    { id: 'live', icon: '🎤', label: 'Live', title: 'Live Transcription' },
    { id: 'rewind', icon: '⏮️', label: 'Rewind', title: 'Meeting Playback' },
    { id: 'timeline', icon: '📊', label: 'Timeline', title: 'Activity Timeline' },
    { id: 'kb', icon: '📚', label: 'Knowledge', title: 'Knowledge Base' },
    { id: 'insights', icon: '💡', label: 'Insights', title: 'Activity Analytics' },
    { id: 'intel', icon: '🧠', label: 'Intel', title: 'Deep Intel' },
    { id: 'admin', icon: '🛡️', label: 'Admin', title: 'Management Suite' },
    { id: 'settings', icon: '⚙️', label: 'Settings', title: 'App Settings' },
    { id: 'help', icon: '❓', label: 'Help', title: 'Help & Documentation' },
];

export function Sidebar({ activeTab, onTabChange, recording, onToggleRecording }: SidebarProps) {
    const [_hovered, setHovered] = useState<string | null>(null);

    return (
        <div className="sidebar">
            {/* Logo */}
            <div className="sidebar-logo">
                <span className="logo-icon">noFriction</span>
                <span className="logo-text">Meetings</span>
            </div>
            <div className="edition-label">V2.1 // PROFESSIONAL</div>

            {/* Navigation */}
            <nav className="sidebar-nav">
                {NAV_ITEMS.map((item) => (
                    <button
                        key={item.id}
                        className={`sidebar-nav-item ${activeTab === item.id ? 'active' : ''}`}
                        onClick={() => onTabChange(item.id)}
                        onMouseEnter={() => setHovered(item.id)}
                        onMouseLeave={() => setHovered(null)}
                        title={item.title}
                    >
                        <span className="nav-icon">{item.icon}</span>
                        <span className="nav-label">{item.label}</span>
                        {activeTab === item.id && <div className="active-indicator" />}
                    </button>
                ))}
            </nav>

            {/* Footer Area with Theme & Recording */}
            <div className="sidebar-footer">
                <div style={{ marginBottom: '16px' }}>
                    <ThemeSelector compact={true} showLabel={false} />
                </div>

                <button
                    className={`recording-toggle ${recording.isRecording ? 'recording' : ''}`}
                    onClick={onToggleRecording}
                    title={recording.isRecording ? 'Stop Recording' : 'Start Recording'}
                >
                    {recording.isRecording ? (
                        <>
                            <span className="rec-dot">●</span>
                            <span>HALT_REC</span>
                        </>
                    ) : (
                        <>
                            <span>⏺</span>
                            <span>ARM / REC</span>
                        </>
                    )}
                </button>
            </div>
        </div>
    );
}
