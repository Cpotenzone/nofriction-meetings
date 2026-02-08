// RewindTab - Synchronized Meeting Timeline View
// Shows audio transcripts, accessibility captures, and screenshots in sync

import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { convertFileSrc } from '@tauri-apps/api/core';
import * as tauri from '../lib/tauri';
import './RewindTab.css';

// Types - field names match Rust snake_case
interface TimelineEntry {
    id: string;
    entry_type: 'transcript' | 'accessibility' | 'screenshot';
    timestamp: string;
    text: string | null;
    speaker: string | null;
    app_name: string | null;
    window_title: string | null;
    image_path: string | null;
    confidence: number | null;
}

interface MeetingTimeline {
    meeting_id: string;
    title: string;
    started_at: string;
    ended_at: string | null;
    entries: TimelineEntry[];
    transcript_count: number;
    accessibility_count: number;
    screenshot_count: number;
}

interface RewindTabProps {
    meetingId: string;
}

type FilterType = 'all' | 'transcript' | 'accessibility' | 'screenshot';

export function RewindTab({ meetingId }: RewindTabProps) {
    const [timeline, setTimeline] = useState<MeetingTimeline | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [selectedEntry, setSelectedEntry] = useState<string | null>(null);
    const [filter, setFilter] = useState<FilterType>('all');
    const [syncScrolling, setSyncScrolling] = useState(true);
    const [expandedImage, setExpandedImage] = useState<string | null>(null);
    const [searchQuery, setSearchQuery] = useState('');
    const [scrollProgress, setScrollProgress] = useState(0);
    const [galleryIndex, setGalleryIndex] = useState(0);

    const transcriptRef = useRef<HTMLDivElement>(null);
    const accessibilityRef = useRef<HTMLDivElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);
    const isScrollingSynced = useRef(false);

    // Load timeline data
    useEffect(() => {
        async function loadTimeline() {
            try {
                setLoading(true);
                const data = await invoke<MeetingTimeline>('get_meeting_timeline', { meetingId });
                setTimeline(data);
                setError(null);
            } catch (e) {
                setError(e as string);
            } finally {
                setLoading(false);
            }
        }
        loadTimeline();
    }, [meetingId]);

    // Get relative time from meeting start
    const getRelativeTime = useCallback((ts: string) => {
        if (!timeline) return '';
        try {
            const entryTime = new Date(ts).getTime();
            const startTime = new Date(timeline.started_at).getTime();
            const diffSecs = Math.floor((entryTime - startTime) / 1000);
            const mins = Math.floor(diffSecs / 60);
            const secs = diffSecs % 60;
            return `${mins}:${secs.toString().padStart(2, '0')}`;
        } catch {
            return '';
        }
    }, [timeline]);

    // Get total meeting duration string
    const getTotalDuration = useCallback(() => {
        if (!timeline?.ended_at) return null;
        try {
            const start = new Date(timeline.started_at).getTime();
            const end = new Date(timeline.ended_at).getTime();
            const diffSecs = Math.floor((end - start) / 1000);
            const mins = Math.floor(diffSecs / 60);
            const secs = diffSecs % 60;
            return `${mins}:${secs.toString().padStart(2, '0')}`;
        } catch {
            return null;
        }
    }, [timeline]);

    const handleExportToVault = async () => {
        if (!meetingId) return;

        try {
            const status = await tauri.getVaultStatus();
            if (!status.valid) {
                alert("Please configure your Obsidian vault in Settings first.");
                return;
            }

            const topics = await tauri.listVaultTopics();
            let topicName = "";

            if (topics.length === 0) {
                const name = prompt("Enter a new Topic name for this export:");
                if (!name) return;
                await tauri.createVaultTopic(name, []);
                topicName = name;
            } else {
                const topicNames = topics.map(t => t.name).join(", ");
                const entry = prompt(`Export to which topic? (${topicNames})`, topics[0]?.name || "Meetings");
                if (!entry) return;
                topicName = entry;
            }

            if (topicName) {
                await tauri.exportMeetingToVault(topicName, meetingId);
                alert(`Successfully exported to Obsidian vault: ${topicName}`);
            }
        } catch (err) {
            console.error("Export failed:", err);
            alert(`Export failed: ${err}`);
        }
    };

    // Synchronized scrolling handler with progress tracking
    const handleScroll = useCallback((source: 'transcript' | 'accessibility') => {
        const sourceRef = source === 'transcript' ? transcriptRef : accessibilityRef;
        if (sourceRef.current) {
            const scrollHeight = sourceRef.current.scrollHeight - sourceRef.current.clientHeight;
            const progress = scrollHeight > 0 ? sourceRef.current.scrollTop / scrollHeight : 0;
            setScrollProgress(Math.min(1, Math.max(0, progress)));
        }

        if (!syncScrolling || isScrollingSynced.current) return;

        const targetRef = source === 'transcript' ? accessibilityRef : transcriptRef;

        if (!sourceRef.current || !targetRef.current) return;

        isScrollingSynced.current = true;

        const sourceScrollTop = sourceRef.current.scrollTop;
        const sourceScrollHeight = sourceRef.current.scrollHeight - sourceRef.current.clientHeight;
        const scrollPercent = sourceScrollHeight > 0 ? sourceScrollTop / sourceScrollHeight : 0;

        const targetScrollHeight = targetRef.current.scrollHeight - targetRef.current.clientHeight;
        targetRef.current.scrollTop = scrollPercent * targetScrollHeight;

        requestAnimationFrame(() => {
            isScrollingSynced.current = false;
        });
    }, [syncScrolling]);

    // Attach scroll listeners
    useEffect(() => {
        const transcriptEl = transcriptRef.current;
        const accessibilityEl = accessibilityRef.current;

        const handleTranscriptScroll = () => handleScroll('transcript');
        const handleAccessibilityScroll = () => handleScroll('accessibility');

        transcriptEl?.addEventListener('scroll', handleTranscriptScroll);
        accessibilityEl?.addEventListener('scroll', handleAccessibilityScroll);

        return () => {
            transcriptEl?.removeEventListener('scroll', handleTranscriptScroll);
            accessibilityEl?.removeEventListener('scroll', handleAccessibilityScroll);
        };
    }, [handleScroll]);

    // Filter and search entries (must be before keyboard navigation useEffect)
    const filteredEntries = useMemo(() => {
        let entries = timeline?.entries || [];

        // Apply type filter
        if (filter !== 'all') {
            entries = entries.filter(e => e.entry_type === filter);
        }

        // Apply search filter
        if (searchQuery.trim()) {
            const query = searchQuery.toLowerCase();
            entries = entries.filter(e =>
                e.text?.toLowerCase().includes(query) ||
                e.speaker?.toLowerCase().includes(query) ||
                e.app_name?.toLowerCase().includes(query) ||
                e.window_title?.toLowerCase().includes(query)
            );
        }

        return entries;
    }, [timeline, filter, searchQuery]);

    // Separate entries by type for dual-lane view
    const transcriptEntries = useMemo(() =>
        filteredEntries.filter(e => e.entry_type === 'transcript'),
        [filteredEntries]
    );
    const accessibilityEntries = useMemo(() =>
        filteredEntries.filter(e => e.entry_type === 'accessibility'),
        [filteredEntries]
    );
    const screenshotEntries = useMemo(() =>
        filteredEntries.filter(e => e.entry_type === 'screenshot'),
        [filteredEntries]
    );

    // Keyboard navigation
    useEffect(() => {
        const handleKeyDown = (e: KeyboardEvent) => {
            if (!timeline) return;

            // Handle expanded image navigation
            if (expandedImage) {
                const screenshots = screenshotEntries;
                const currentIdx = screenshots.findIndex(s => s.image_path === expandedImage);
                switch (e.key) {
                    case 'ArrowLeft':
                        e.preventDefault();
                        if (currentIdx > 0) {
                            setExpandedImage(screenshots[currentIdx - 1].image_path);
                        }
                        break;
                    case 'ArrowRight':
                        e.preventDefault();
                        if (currentIdx < screenshots.length - 1) {
                            setExpandedImage(screenshots[currentIdx + 1].image_path);
                        }
                        break;
                    case 'Escape':
                        e.preventDefault();
                        setExpandedImage(null);
                        break;
                }
                return;
            }

            // Gallery mode navigation (screenshot filter)
            if (filter === 'screenshot') {
                const screenshots = screenshotEntries;
                switch (e.key) {
                    case 'ArrowLeft':
                        e.preventDefault();
                        setGalleryIndex(i => Math.max(0, i - 1));
                        break;
                    case 'ArrowRight':
                        e.preventDefault();
                        setGalleryIndex(i => Math.min(screenshots.length - 1, i + 1));
                        break;
                    case 'ArrowUp':
                        e.preventDefault();
                        setGalleryIndex(i => Math.max(0, i - 6)); // Jump row up (6 per row)
                        break;
                    case 'ArrowDown':
                        e.preventDefault();
                        setGalleryIndex(i => Math.min(screenshots.length - 1, i + 6)); // Jump row down
                        break;
                    case 'Home':
                        e.preventDefault();
                        setGalleryIndex(0);
                        break;
                    case 'End':
                        e.preventDefault();
                        setGalleryIndex(screenshots.length - 1);
                        break;
                    case 'PageUp':
                        e.preventDefault();
                        setGalleryIndex(i => Math.max(0, i - 10));
                        break;
                    case 'PageDown':
                        e.preventDefault();
                        setGalleryIndex(i => Math.min(screenshots.length - 1, i + 10));
                        break;
                    case 'Enter':
                        e.preventDefault();
                        if (screenshots[galleryIndex]?.image_path) {
                            setExpandedImage(screenshots[galleryIndex].image_path);
                        }
                        break;
                    case 'Escape':
                        setGalleryIndex(0);
                        setSearchQuery('');
                        break;
                    case '/':
                        if (!e.ctrlKey && !e.metaKey) {
                            e.preventDefault();
                            document.getElementById('rewind-search')?.focus();
                        }
                        break;
                }
                return;
            }

            // Standard timeline navigation
            const entries = filteredEntries;
            const currentIndex = selectedEntry
                ? entries.findIndex(e => e.id === selectedEntry)
                : -1;

            switch (e.key) {
                case 'ArrowDown':
                case 'j':
                    e.preventDefault();
                    if (currentIndex < entries.length - 1) {
                        setSelectedEntry(entries[currentIndex + 1].id);
                    } else if (currentIndex === -1 && entries.length > 0) {
                        setSelectedEntry(entries[0].id);
                    }
                    break;
                case 'ArrowUp':
                case 'k':
                    e.preventDefault();
                    if (currentIndex > 0) {
                        setSelectedEntry(entries[currentIndex - 1].id);
                    }
                    break;
                case 'Escape':
                    setSelectedEntry(null);
                    setSearchQuery('');
                    break;
                case '/':
                    if (!e.ctrlKey && !e.metaKey) {
                        e.preventDefault();
                        document.getElementById('rewind-search')?.focus();
                    }
                    break;
            }
        };

        document.addEventListener('keydown', handleKeyDown);
        return () => document.removeEventListener('keydown', handleKeyDown);
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [timeline, selectedEntry, expandedImage, filter, galleryIndex, screenshotEntries, filteredEntries]);

    // Handle entry click
    const handleEntryClick = (id: string) => {
        setSelectedEntry(id);
    };

    // Convert file path to displayable URL

    // Convert file path to displayable URL
    const getImageUrl = (path: string | null) => {
        if (!path) return null;
        try {
            return convertFileSrc(path);
        } catch {
            return null;
        }
    };

    if (loading) {
        return (
            <div className="rewind-loading">
                <div className="rewind-spinner"></div>
                <p>Loading timeline...</p>
            </div>
        );
    }

    if (error) {
        return (
            <div className="rewind-error">
                <span className="error-icon">⚠️</span>
                <p>{error}</p>
                <button
                    className="retry-btn"
                    onClick={() => window.location.reload()}
                >
                    Retry
                </button>
            </div>
        );
    }

    if (!timeline) {
        return (
            <div className="rewind-empty">
                <span className="empty-icon">📭</span>
                <p>No timeline data available</p>
            </div>
        );
    }

    const totalDuration = getTotalDuration();

    return (
        <div className="rewind-tab" ref={containerRef}>
            {/* Progress Bar */}
            <div className="rewind-progress-container">
                <div
                    className="rewind-progress-bar"
                    style={{ width: `${scrollProgress * 100}%` }}
                />
                <span className="progress-time">
                    {totalDuration && `/ ${totalDuration}`}
                </span>
            </div>

            {/* Header with stats */}
            <div className="rewind-header">
                <div className="rewind-stats">
                    <span className="stat">
                        <span className="stat-icon">🎤</span>
                        <span className="stat-value">{timeline.transcript_count}</span>
                        <span className="stat-label">Transcripts</span>
                    </span>
                    <span className="stat">
                        <span className="stat-icon">📺</span>
                        <span className="stat-value">{timeline.accessibility_count}</span>
                        <span className="stat-label">Screen Text</span>
                    </span>
                    <span className="stat">
                        <span className="stat-icon">📸</span>
                        <span className="stat-value">{timeline.screenshot_count}</span>
                        <span className="stat-label">Screenshots</span>
                    </span>
                </div>

                {/* Search */}
                <div className="rewind-search">
                    <input
                        id="rewind-search"
                        type="text"
                        placeholder="Search... (press /)"
                        value={searchQuery}
                        onChange={(e) => setSearchQuery(e.target.value)}
                        className="search-input"
                    />
                    {searchQuery && (
                        <button
                            className="search-clear"
                            onClick={() => setSearchQuery('')}
                        >
                            ✕
                        </button>
                    )}
                </div>

                {/* Filter buttons */}
                <div className="rewind-filters">
                    <button
                        className={`filter-btn ${filter === 'all' ? 'active' : ''}`}
                        onClick={() => setFilter('all')}
                    >
                        All
                    </button>
                    <button
                        className={`filter-btn ${filter === 'transcript' ? 'active' : ''}`}
                        onClick={() => setFilter('transcript')}
                    >
                        🎤 Audio
                    </button>
                    <button
                        className={`filter-btn ${filter === 'accessibility' ? 'active' : ''}`}
                        onClick={() => setFilter('accessibility')}
                    >
                        📺 Screen
                    </button>
                    <button
                        className={`filter-btn ${filter === 'screenshot' ? 'active' : ''}`}
                        onClick={() => setFilter('screenshot')}
                    >
                        📸 Images
                    </button>
                    <button
                        className={`sync-btn ${syncScrolling ? 'active' : ''}`}
                        onClick={() => setSyncScrolling(!syncScrolling)}
                        title="Sync scrolling between lanes (toggle)"
                    >
                        🔗
                    </button>
                </div>
            </div>

            {/* Keyboard hints */}
            <div className="keyboard-hints">
                {filter === 'screenshot' ? (
                    <>
                        <span>←→ Navigate</span>
                        <span>Enter Expand</span>
                        <span>Esc Close</span>
                    </>
                ) : (
                    <>
                        <span>↑↓ Navigate</span>
                        <span>/ Search</span>
                        <span>Esc Clear</span>
                    </>
                )}
            </div>

            {/* Dual-lane timeline view */}
            <div className="rewind-content">
                {filter === 'all' ? (
                    <div className="rewind-dual-view">
                        {/* Transcript Lane */}
                        <div className="rewind-lane transcript-lane" ref={transcriptRef}>
                            <div className="lane-header">
                                <span className="lane-icon">🎤</span> Audio Transcript
                                <span className="lane-count">{transcriptEntries.length}</span>
                            </div>
                            <div className="lane-entries">
                                {transcriptEntries.map((entry) => (
                                    <div
                                        key={entry.id}
                                        className={`timeline-entry transcript-entry ${selectedEntry === entry.id ? 'selected' : ''}`}
                                        onClick={() => handleEntryClick(entry.id)}
                                    >
                                        <div className="entry-time">{getRelativeTime(entry.timestamp)}</div>
                                        <div className="entry-content">
                                            {entry.speaker && <span className="entry-speaker">{entry.speaker}:</span>}
                                            <span className="entry-text">{entry.text}</span>
                                        </div>
                                    </div>
                                ))}
                                {transcriptEntries.length === 0 && (
                                    <div className="lane-empty">
                                        <span className="empty-icon">🎙️</span>
                                        <p>No transcripts recorded</p>
                                        <small>Audio transcription happens during recording</small>
                                    </div>
                                )}
                            </div>
                        </div>

                        {/* Screen Activity Lane */}
                        <div className="rewind-lane accessibility-lane" ref={accessibilityRef}>
                            <div className="lane-header">
                                <span className="lane-icon">📺</span> Screen Activity
                                {screenshotEntries.length > 0 && (
                                    <span className="lane-badge">{screenshotEntries.length} 📸</span>
                                )}
                                <span className="lane-count">{accessibilityEntries.length + screenshotEntries.length}</span>
                            </div>
                            <div className="lane-entries">
                                {[...accessibilityEntries, ...screenshotEntries]
                                    .sort((a, b) => a.timestamp.localeCompare(b.timestamp))
                                    .map((entry) => (
                                        <div
                                            key={entry.id}
                                            className={`timeline-entry ${entry.entry_type}-entry ${selectedEntry === entry.id ? 'selected' : ''}`}
                                            onClick={() => handleEntryClick(entry.id)}
                                        >
                                            <div className="entry-time">{getRelativeTime(entry.timestamp)}</div>
                                            <div className="entry-content">
                                                {entry.entry_type === 'screenshot' ? (
                                                    <div className="screenshot-container">
                                                        {entry.image_path && (
                                                            <img
                                                                src={getImageUrl(entry.image_path) || ''}
                                                                alt="Screenshot"
                                                                className="screenshot-thumbnail"
                                                                onClick={(e) => {
                                                                    e.stopPropagation();
                                                                    setExpandedImage(entry.image_path);
                                                                }}
                                                            />
                                                        )}
                                                        {entry.text && (
                                                            <div className="screenshot-ocr">{entry.text}</div>
                                                        )}
                                                    </div>
                                                ) : (
                                                    <>
                                                        <div className="entry-app">
                                                            <span className="app-name">{entry.app_name || 'Unknown App'}</span>
                                                            {entry.window_title && (
                                                                <span className="window-title">{entry.window_title}</span>
                                                            )}
                                                        </div>
                                                        <div className="entry-text accessibility-text">{entry.text}</div>
                                                    </>
                                                )}
                                            </div>
                                        </div>
                                    ))}
                                {accessibilityEntries.length === 0 && screenshotEntries.length === 0 && (
                                    <div className="lane-empty">
                                        <span className="empty-icon">🖥️</span>
                                        <p>No screen activity recorded</p>
                                        <small>Screen capture runs during meetings</small>
                                    </div>
                                )}
                            </div>
                        </div>
                    </div>
                ) : filter === 'screenshot' ? (
                    /* Screenshot Gallery View */
                    <div className="screenshot-gallery-view">
                        {/* Gallery header with navigation */}
                        <div className="gallery-nav">
                            {/* Jump to start */}
                            <button
                                className="gallery-nav-btn gallery-jump"
                                onClick={() => setGalleryIndex(0)}
                                disabled={galleryIndex === 0}
                                title="Jump to start (Home)"
                            >
                                ⏮
                            </button>
                            {/* Page back (-10) */}
                            <button
                                className="gallery-nav-btn"
                                onClick={() => setGalleryIndex(i => Math.max(0, i - 10))}
                                disabled={galleryIndex < 10}
                                title="Back 10 (Page Up)"
                            >
                                ◀◀
                            </button>
                            <button
                                className="gallery-nav-btn"
                                onClick={() => setGalleryIndex(i => Math.max(0, i - 1))}
                                disabled={galleryIndex === 0}
                            >
                                ◀ Prev
                            </button>
                            <span className="gallery-counter">
                                {screenshotEntries.length > 0
                                    ? `${galleryIndex + 1} of ${screenshotEntries.length.toLocaleString()}`
                                    : '0 screenshots'}
                            </span>
                            <button
                                className="gallery-nav-btn"
                                onClick={() => setGalleryIndex(i => Math.min(screenshotEntries.length - 1, i + 1))}
                                disabled={galleryIndex >= screenshotEntries.length - 1}
                            >
                                Next ▶
                            </button>
                            {/* Page forward (+10) */}
                            <button
                                className="gallery-nav-btn"
                                onClick={() => setGalleryIndex(i => Math.min(screenshotEntries.length - 1, i + 10))}
                                disabled={galleryIndex >= screenshotEntries.length - 10}
                                title="Forward 10 (Page Down)"
                            >
                                ▶▶
                            </button>
                            {/* Jump to end */}
                            <button
                                className="gallery-nav-btn gallery-jump"
                                onClick={() => setGalleryIndex(screenshotEntries.length - 1)}
                                disabled={galleryIndex >= screenshotEntries.length - 1}
                                title="Jump to end (End)"
                            >
                                ⏭
                            </button>

                            <button
                                className="gallery-nav-btn gallery-export-btn"
                                onClick={handleExportToVault}
                                title="Export current meeting to Obsidian Vault"
                            >
                                🚢 Export to Vault
                            </button>
                        </div>

                        {/* Timeline Scrubber */}
                        {screenshotEntries.length > 1 && (
                            <div className="timeline-scrubber">
                                <span className="scrubber-label">0:00</span>
                                <input
                                    type="range"
                                    min={0}
                                    max={screenshotEntries.length - 1}
                                    value={galleryIndex}
                                    onChange={(e) => setGalleryIndex(parseInt(e.target.value, 10))}
                                    className="scrubber-slider"
                                />
                                <span className="scrubber-label">
                                    {screenshotEntries.length > 0 && getRelativeTime(screenshotEntries[screenshotEntries.length - 1].timestamp)}
                                </span>
                            </div>
                        )}

                        {/* Large preview of current image */}
                        {screenshotEntries[galleryIndex] && (
                            <div className="gallery-preview">
                                <img
                                    src={getImageUrl(screenshotEntries[galleryIndex].image_path) || ''}
                                    alt="Screenshot preview"
                                    className="gallery-preview-img"
                                    onClick={() => setExpandedImage(screenshotEntries[galleryIndex].image_path)}
                                />
                                <div className="gallery-preview-time">
                                    {getRelativeTime(screenshotEntries[galleryIndex].timestamp)}
                                </div>
                            </div>
                        )}

                        {/* Thumbnail grid - windowed for performance */}
                        <div className="gallery-grid">
                            {/* Show info if we're in a large gallery */}
                            {screenshotEntries.length > 100 && (
                                <div className="gallery-window-info">
                                    Showing thumbnails {Math.max(0, galleryIndex - 50) + 1}-{Math.min(screenshotEntries.length, galleryIndex + 50)}
                                </div>
                            )}
                            {screenshotEntries
                                .slice(
                                    Math.max(0, galleryIndex - 50),
                                    Math.min(screenshotEntries.length, galleryIndex + 50)
                                )
                                .map((entry, relIdx) => {
                                    const idx = Math.max(0, galleryIndex - 50) + relIdx;
                                    return (
                                        <div
                                            key={entry.id}
                                            className={`gallery-thumb ${idx === galleryIndex ? 'selected' : ''}`}
                                            onClick={() => setGalleryIndex(idx)}
                                        >
                                            <img
                                                src={getImageUrl(entry.image_path) || ''}
                                                alt={`Screenshot ${idx + 1}`}
                                                loading="lazy"
                                            />
                                            <span className="thumb-time">{getRelativeTime(entry.timestamp)}</span>
                                            {idx === galleryIndex && (
                                                <span className="thumb-current">{idx + 1}</span>
                                            )}
                                        </div>
                                    );
                                })}
                        </div>

                        {screenshotEntries.length === 0 && (
                            <div className="lane-empty">
                                <span className="empty-icon">📸</span>
                                <p>No screenshots found</p>
                            </div>
                        )}
                    </div>
                ) : (
                    /* Single filtered view for transcript/accessibility */
                    <div className="rewind-single-view">
                        <div className="lane-entries">
                            {filteredEntries.map((entry) => (
                                <div
                                    key={entry.id}
                                    className={`timeline-entry ${entry.entry_type}-entry ${selectedEntry === entry.id ? 'selected' : ''}`}
                                    onClick={() => handleEntryClick(entry.id)}
                                >
                                    <div className="entry-time">{getRelativeTime(entry.timestamp)}</div>
                                    <div className="entry-content">
                                        {entry.entry_type === 'transcript' ? (
                                            <>
                                                {entry.speaker && <span className="entry-speaker">{entry.speaker}:</span>}
                                                <span className="entry-text">{entry.text}</span>
                                            </>
                                        ) : (
                                            <>
                                                <div className="entry-app">
                                                    <span className="app-name">{entry.app_name || 'Unknown App'}</span>
                                                    {entry.window_title && (
                                                        <span className="window-title">{entry.window_title}</span>
                                                    )}
                                                </div>
                                                <div className="entry-text accessibility-text">{entry.text}</div>
                                            </>
                                        )}
                                    </div>
                                </div>
                            ))}
                            {filteredEntries.length === 0 && (
                                <div className="lane-empty">
                                    <span className="empty-icon">🔍</span>
                                    <p>No entries found</p>
                                    {searchQuery && <small>Try a different search term</small>}
                                </div>
                            )}
                        </div>
                    </div>
                )}
            </div>

            {/* Expanded Image Modal with navigation */}
            {expandedImage && (
                <div className="image-modal" onClick={() => setExpandedImage(null)}>
                    <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                        <button
                            className="modal-nav modal-prev"
                            onClick={() => {
                                const idx = screenshotEntries.findIndex(s => s.image_path === expandedImage);
                                if (idx > 0) setExpandedImage(screenshotEntries[idx - 1].image_path);
                            }}
                            disabled={screenshotEntries.findIndex(s => s.image_path === expandedImage) === 0}
                        >
                            ◀
                        </button>
                        <img src={getImageUrl(expandedImage) || ''} alt="Full Screenshot" />
                        <button
                            className="modal-nav modal-next"
                            onClick={() => {
                                const idx = screenshotEntries.findIndex(s => s.image_path === expandedImage);
                                if (idx < screenshotEntries.length - 1) setExpandedImage(screenshotEntries[idx + 1].image_path);
                            }}
                            disabled={screenshotEntries.findIndex(s => s.image_path === expandedImage) >= screenshotEntries.length - 1}
                        >
                            ▶
                        </button>
                        <button className="modal-close" onClick={() => setExpandedImage(null)}>✕</button>
                        <div className="modal-counter">
                            {screenshotEntries.findIndex(s => s.image_path === expandedImage) + 1} / {screenshotEntries.length}
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}

export default RewindTab;
