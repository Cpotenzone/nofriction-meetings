import { useState } from 'react';
import ThemeSelector from "./ThemeSelector";
import {
    Mic,
    Rewind,
    Activity,
    BookOpen,
    Lightbulb,
    BrainCircuit,
    ShieldAlert,
    Settings,
    HelpCircle
} from 'lucide-react';

interface SidebarProps {
    activeTab: string;
    onTabChange: (tab: string) => void;
    recording: any;
    onToggleRecording: () => void;
}

interface NavItem {
    id: string;
    icon: React.ElementType;
    label: string;
    title: string;
}

const NAV_ITEMS: NavItem[] = [
    { id: 'live', icon: Mic, label: 'Live', title: 'Live Transcription' },
    { id: 'rewind', icon: Rewind, label: 'Rewind', title: 'Meeting Playback' },
    { id: 'timeline', icon: Activity, label: 'Timeline', title: 'Activity Timeline' },
    { id: 'kb', icon: BookOpen, label: 'Knowledge', title: 'Knowledge Base' },
    { id: 'insights', icon: Lightbulb, label: 'Insights', title: 'Activity Analytics' },
    { id: 'intel', icon: BrainCircuit, label: 'Intel', title: 'Deep Intel' },
    { id: 'admin', icon: ShieldAlert, label: 'Admin', title: 'Management Suite' },
    { id: 'settings', icon: Settings, label: 'Settings', title: 'App Settings' },
    { id: 'help', icon: HelpCircle, label: 'Help', title: 'Help & Documentation' },
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
            <div className="edition-label">V2.4 // PROFESSIONAL</div>

            {/* Navigation */}
            <nav className="sidebar-nav">
                {NAV_ITEMS.map((item) => {
                    const Icon = item.icon;
                    return (
                        <button
                            key={item.id}
                            className={`sidebar-nav-item ${activeTab === item.id ? 'active' : ''}`}
                            onClick={() => onTabChange(item.id)}
                            onMouseEnter={() => setHovered(item.id)}
                            onMouseLeave={() => setHovered(null)}
                            title={item.title}
                        >
                            <span className="nav-icon">
                                <Icon size={20} strokeWidth={1.5} />
                            </span>
                            <span className="nav-label">{item.label}</span>
                            {activeTab === item.id && <div className="active-indicator" />}
                        </button>
                    );
                })}
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
