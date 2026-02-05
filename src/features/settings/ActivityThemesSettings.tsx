import { useEffect, useState } from 'react';
import { getActiveTheme, setActiveTheme, getThemeSettings, setThemeInterval, getThemeTimeToday, type ThemeSettings } from '../lib/tauri';

interface ThemeConfig {
    id: string;
    label: string;
    color: string;
    icon: string;
    fullDescription: string;
}

const THEMES: ThemeConfig[] = [
    {
        id: 'prospecting',
        label: 'Prospecting',
        color: '#10B981',
        icon: '🎯',
        fullDescription: 'Focused on outreach, lead generation, and sales meetings.'
    },
    {
        id: 'fundraising',
        label: 'Fundraising',
        color: '#8B5CF6',
        icon: '💰',
        fullDescription: 'Pitching, investor relations, and grant applications.'
    },
    {
        id: 'product_dev',
        label: 'Product Dev',
        color: '#3B82F6',
        icon: '🛠️',
        fullDescription: 'Coding, design, testing, and roadmap planning.'
    },
    {
        id: 'admin',
        label: 'Admin',
        color: '#F59E0B',
        icon: '📋',
        fullDescription: 'Emails, scheduling, reports, and team coordination.'
    },
    {
        id: 'personal',
        label: 'Personal',
        color: '#64748B',
        icon: '🏖️',
        fullDescription: 'Browsing, social media, breaks, and personal tasks.'
    }
];

export function ActivityThemesSettings() {
    const [activeTheme, setActiveThemeState] = useState<string>('prospecting');
    const [, setThemeSettingsState] = useState<ThemeSettings | null>(null);
    const [interval, setInterval] = useState<number>(1.5);
    const [intervalDebounce, setIntervalDebounce] = useState<ReturnType<typeof setTimeout> | null>(null);
    const [hoursInTheme, setHoursInTheme] = useState<number>(0);
    const [loadingTime, setLoadingTime] = useState<boolean>(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        loadSettings();
    }, []);

    useEffect(() => {
        if (activeTheme) {
            loadTimeForTheme(activeTheme);
        }
    }, [activeTheme]);

    const loadSettings = async () => {
        try {
            setError(null);
            const [theme, settings] = await Promise.all([
                getActiveTheme(),
                getThemeSettings()
            ]);
            setActiveThemeState(theme);
            setThemeSettingsState(settings);

            // Set interval for active theme
            const intervalMs = (settings as any)[`${theme}_interval_ms`];
            if (intervalMs) {
                setInterval(intervalMs / 1000);
            }
        } catch (error) {
            console.error('Failed to load theme settings:', error);
            setError('Failed to load theme settings');
        }
    };

    const loadTimeForTheme = async (theme: string) => {
        try {
            setLoadingTime(true);
            const hours = await getThemeTimeToday(theme);
            setHoursInTheme(hours);
        } catch (error) {
            console.error('Failed to load time for theme:', error);
            setHoursInTheme(0); // Fallback to 0
        } finally {
            setLoadingTime(false);
        }
    };

    const handleThemeChange = async (themeId: string) => {
        try {
            setError(null);
            await setActiveTheme(themeId);
            setActiveThemeState(themeId);
            await loadSettings();
            await loadTimeForTheme(themeId); // Load time for new theme
        } catch (error) {
            console.error('Failed to set theme:', error);
            setError(`Failed to switch to ${themeId}`);
        }
    };

    const handleIntervalChange = (newIntervalSecs: number) => {
        setInterval(newIntervalSecs);

        // Debounce backend updates to avoid spamming API
        if (intervalDebounce) {
            clearTimeout(intervalDebounce);
        }

        const timeout = setTimeout(async () => {
            try {
                const intervalMs = Math.round(newIntervalSecs * 1000);
                await setThemeInterval(activeTheme, intervalMs);
                console.log(`Updated ${activeTheme} interval to ${newIntervalSecs}s`);
            } catch (error) {
                console.error('Failed to save interval:', error);
                setError('Failed to save interval setting');
            }
        }, 500); // Wait 500ms after user stops dragging

        setIntervalDebounce(timeout);
    };

    const currentThemeConfig = THEMES.find(t => t.id === activeTheme) || THEMES[0];
    const inactiveThemes = THEMES.filter(t => t.id !== activeTheme);

    return (
        <div className="theme-settings-container">
            {error && (
                <div style={{
                    padding: '12px 16px',
                    background: 'rgba(239, 68, 68, 0.1)',
                    border: '1px solid rgba(239, 68, 68, 0.3)',
                    borderRadius: '8px',
                    color: '#ef4444',
                    fontSize: '0.875rem',
                    marginBottom: '16px'
                }}>
                    ⚠️ {error}
                </div>
            )}

            {/* Hero Card - Active Theme */}
            <div className={`theme-hero-card ${activeTheme}`}>
                <div className="theme-hero-header">
                    <div className="theme-hero-title-section">
                        <span className="theme-hero-icon">{currentThemeConfig.icon}</span>
                        <div>
                            <div className="theme-hero-title">{currentThemeConfig.label}</div>
                            <div className="theme-hero-description">
                                {currentThemeConfig.fullDescription}
                            </div>
                        </div>
                    </div>
                    <div className="theme-hero-stats">
                        <div className="theme-hero-stats-label">Today's Activity</div>
                        <div className="theme-hero-stats-value">
                            {loadingTime ? '...' : hoursInTheme.toFixed(1)} hours
                        </div>
                        <div className="theme-hero-stats-sublabel">in {currentThemeConfig.label}</div>
                    </div>
                </div>

                <div className="theme-interval-control">
                    <div className="theme-interval-label">
                        <span>Screenshot Interval</span>
                        <span className="theme-interval-value">{interval.toFixed(1)} seconds</span>
                    </div>
                    <input
                        type="range"
                        min="0.5"
                        max="10"
                        step="0.5"
                        value={interval}
                        onChange={(e) => handleIntervalChange(parseFloat(e.target.value))}
                        className="theme-interval-slider"
                    />
                </div>
            </div>

            {/* Inactive Themes Grid */}
            <div className="theme-grid">
                {inactiveThemes.map((theme) => (
                    <div
                        key={theme.id}
                        className="theme-card"
                        style={{ borderLeft: `4px solid ${theme.color}` }}
                    >
                        <div className="theme-card-header">
                            <span className="theme-card-icon">{theme.icon}</span>
                            <span className="theme-card-title">{theme.label}</span>
                        </div>
                        <div className="theme-card-description">
                            {theme.fullDescription}
                        </div>
                        <button
                            className="theme-card-activate"
                            onClick={() => handleThemeChange(theme.id)}
                        >
                            Activate
                        </button>
                    </div>
                ))}
            </div>
        </div>
    );
}
