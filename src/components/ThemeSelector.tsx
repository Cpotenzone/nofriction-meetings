import { useEffect, useState } from 'react';
import { getActiveTheme, setActiveTheme } from '../lib/tauri';

interface ThemeSelectorProps {
    compact?: boolean;
    showLabel?: boolean;
}

interface ThemeConfig {
    id: string;
    label: string;
    color: string;
    icon: string;
    description: string;
}

const THEMES: ThemeConfig[] = [
    {
        id: 'prospecting',
        label: 'Prospecting',
        color: '#10B981',
        icon: '🎯',
        description: 'Energetic, focused, growth-oriented'
    },
    {
        id: 'fundraising',
        label: 'Fundraising',
        color: '#8B5CF6',
        icon: '💰',
        description: 'Premium, strategic, high-stakes'
    },
    {
        id: 'product_dev',
        label: 'Product Dev',
        color: '#3B82F6',
        icon: '🛠️',
        description: 'Analytical, creative, deep work'
    },
    {
        id: 'admin',
        label: 'Admin',
        color: '#F59E0B',
        icon: '📋',
        description: 'Organized, efficient, task-focused'
    },
    {
        id: 'personal',
        label: 'Personal',
        color: '#64748B',
        icon: '🏖️',
        description: 'Relaxed, private, off-duty'
    }
];

export default function ThemeSelector({ compact = false }: ThemeSelectorProps) {
    const [activeTheme, setActiveThemeState] = useState<string>('prospecting');
    const [isOpen, setIsOpen] = useState(false);

    useEffect(() => {
        loadActiveTheme();
    }, []);

    const loadActiveTheme = async () => {
        try {
            const theme = await getActiveTheme();
            setActiveThemeState(theme);
        } catch (error) {
            console.error('Failed to load active theme:', error);
        }
    };

    const handleThemeChange = async (themeId: string) => {
        try {
            await setActiveTheme(themeId);
            setActiveThemeState(themeId);
            setIsOpen(false);
        } catch (error) {
            console.error('Failed to set theme:', error);
        }
    };

    const currentTheme = THEMES.find(t => t.id === activeTheme) || THEMES[0];

    if (compact) {
        return (
            <div className="theme-selector-compact">
                <button
                    onClick={() => setIsOpen(!isOpen)}
                    className="theme-selector-button"
                    style={{
                        borderLeft: `3px solid ${currentTheme.color}`,
                    }}
                >
                    <span className="theme-icon">{currentTheme.icon}</span>
                    <span className="theme-label">{currentTheme.label}</span>
                </button>

                {isOpen && (
                    <>
                        <div
                            className="theme-selector-backdrop"
                            onClick={() => setIsOpen(false)}
                        />

                        <div className="theme-selector-popover">
                            {THEMES.map((theme) => (
                                <button
                                    key={theme.id}
                                    onClick={() => handleThemeChange(theme.id)}
                                    className={`theme-option ${activeTheme === theme.id ? 'active' : ''}`}
                                    style={{
                                        borderLeft: `4px solid ${theme.color}`,
                                    }}
                                >
                                    <span className="theme-option-icon">{theme.icon}</span>
                                    <div className="theme-option-content">
                                        <div className="theme-option-title">{theme.label}</div>
                                        <div className="theme-option-description">{theme.description}</div>
                                    </div>
                                    {activeTheme === theme.id && (
                                        <svg className="theme-option-check" viewBox="0 0 20 20" fill="currentColor">
                                            <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                                        </svg>
                                    )}
                                </button>
                            ))}
                        </div>
                    </>
                )}
            </div>
        );
    }

    // Full view is handled in FullSettings now
    return null;
}
