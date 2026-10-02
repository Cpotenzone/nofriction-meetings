// ═══════════════════════════════════════════════════════════════════
// Intel Dashboard — Cross-Meeting Intelligence Hub
// Obsidian-powered visualizations across all meetings
// ═══════════════════════════════════════════════════════════════════

import React, { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import ForceGraph2D from 'react-force-graph-2d';
import {
    Search,
    Network,
    Clock,
    Users,
    Hash,
    FileText,
    Building2,
    Loader2,
    RefreshCcw,
    Folder,
    Zap,
    Link2,
    TrendingUp,
    CalendarDays,
} from 'lucide-react';
import * as tauri from '../../../lib/tauri';
import {
    VaultGraph as VaultGraphData,
    VaultTopic,
    VaultTag,
    Meeting,
    VaultSearchResult,
} from '../../../lib/tauri';
import './IntelDashboard.css';
import { friendlyAiError, isNoProviderError } from '../../../lib/ai';
import { AiSetupNotice } from '../../AiSetupNotice';

// ─── Types ───────────────────────────────────────────────────────

type IntelTab = 'overview' | 'graph' | 'timeline' | 'people' | 'topics' | 'search';

interface PersonInfo {
    name: string;
    company: string;
    email: string;
    meetingCount: number;
    initials: string;
}

interface CompanyInfo {
    name: string;
    domain: string;
    people: string[];
}

interface MostConnected {
    id: string;
    label: string;
    connections: number;
    type: string;
}

// ─── Component ───────────────────────────────────────────────────

export const IntelDashboard: React.FC = () => {
    // Tab state
    const [activeTab, setActiveTab] = useState<IntelTab>('overview');

    // Data state
    const [graphData, setGraphData] = useState<VaultGraphData>({ nodes: [], edges: [] });
    const [topics, setTopics] = useState<VaultTopic[]>([]);
    const [tags, setTags] = useState<VaultTag[]>([]);
    const [meetings, setMeetings] = useState<Meeting[]>([]);
    const [people, setPeople] = useState<PersonInfo[]>([]);
    const [companies, setCompanies] = useState<CompanyInfo[]>([]);
    const [searchQuery, setSearchQuery] = useState('');
    const [searchResults, setSearchResults] = useState<VaultSearchResult[]>([]);
    const [isSearching, setIsSearching] = useState(false);
    const [isLoading, setIsLoading] = useState(true);
    const [calendarEvents, setCalendarEvents] = useState<any[]>([]);
    const [eventIntel, setEventIntel] = useState<Record<string, any>>({});
    const [lookingUp, setLookingUp] = useState<Record<string, boolean>>({});
    const [lookupError, setLookupError] = useState<string | null>(null);
    const [lookupNeedsAi, setLookupNeedsAi] = useState(false);

    // Graph state
    const graphContainerRef = useRef<HTMLDivElement>(null);
    const [graphDimensions, setGraphDimensions] = useState({ width: 800, height: 400 });
    const [hoverNode, setHoverNode] = useState<string | null>(null);
    const [highlightNodes, setHighlightNodes] = useState(new Set<string>());
    const [highlightLinks, setHighlightLinks] = useState(new Set<string>());

    // ─── Data Loading ──────────────────────────────────────────────

    const loadAllData = useCallback(async () => {
        setIsLoading(true);
        try {
            const [vaultGraph, vaultTopics, vaultTags, meetingList] = await Promise.all([
                tauri.getVaultGraph().catch(() => ({ nodes: [], edges: [] })),
                tauri.listVaultTopics().catch(() => []),
                tauri.listVaultTags().catch(() => []),
                tauri.getMeetings(500).catch(() => []),
            ]);

            setGraphData(vaultGraph);
            setTopics(vaultTopics);
            setTags(vaultTags.sort((a, b) => b.fileCount - a.fileCount));
            setMeetings(meetingList);

            // Load calendar events
            try {
                const events = await tauri.getEnrichedCalendarEvents();
                const sorted = (events as any[]).sort((a: any, b: any) =>
                    new Date(a.start_time).getTime() - new Date(b.start_time).getTime()
                );
                // Show only upcoming events (future or happening now)
                const now = new Date();
                const upcoming = sorted.filter((e: any) => new Date(e.end_time) > now);
                setCalendarEvents(upcoming.slice(0, 5));
            } catch (err) {
                console.error('Calendar events load failed:', err);
            }

            // Load people & companies from vault files
            await loadPeopleAndCompanies();
        } catch (err) {
            console.error('Intel dashboard load failed:', err);
        } finally {
            setIsLoading(false);
        }
    }, []);

    const loadPeopleAndCompanies = async () => {
        try {
            const [peopleFiles, companyFiles] = await Promise.all([
                tauri.listVaultFiles('noFriction/people').catch(() => []),
                tauri.listVaultFiles('noFriction/companies').catch(() => []),
            ]);

            // Parse people from markdown frontmatter
            const parsedPeople: PersonInfo[] = [];
            for (const file of peopleFiles.filter(f => f.name.endsWith('.md'))) {
                try {
                    const content = await tauri.readVaultFile(file.path);
                    const fm = content.frontmatter || {};
                    const meetingLinks = (content.body.match(/\[\[.*?\]\]/g) || []).length;
                    const name = String(fm.title || file.name.replace('.md', ''));
                    parsedPeople.push({
                        name,
                        company: String(fm.company || '').replace(/\[\[|\]\]/g, ''),
                        email: String(fm.email || ''),
                        meetingCount: meetingLinks,
                        initials: name.split(' ').map(w => w[0]).join('').toUpperCase().substring(0, 2),
                    });
                } catch { /* skip unreadable files */ }
            }
            setPeople(parsedPeople.sort((a, b) => b.meetingCount - a.meetingCount));

            // Parse companies
            const parsedCompanies: CompanyInfo[] = [];
            for (const file of companyFiles.filter(f => f.name.endsWith('.md'))) {
                try {
                    const content = await tauri.readVaultFile(file.path);
                    const fm = content.frontmatter || {};
                    const peopleSection = content.body.match(/## People\n([\s\S]*?)(?:\n##|$)/);
                    const personLinks = peopleSection
                        ? (peopleSection[1].match(/\[\[([^\]]+)\]\]/g) || [])
                            .map(l => l.replace(/\[\[|\]\]/g, ''))
                        : [];
                    parsedCompanies.push({
                        name: String(fm.title || file.name.replace('.md', '')),
                        domain: String(fm.domain || ''),
                        people: personLinks,
                    });
                } catch { /* skip */ }
            }
            setCompanies(parsedCompanies);
        } catch (err) {
            console.error('Failed to load people/companies:', err);
        }
    };

    useEffect(() => {
        loadAllData();
    }, [loadAllData]);

    // Graph dimensions
    useEffect(() => {
        const updateDimensions = () => {
            if (graphContainerRef.current) {
                setGraphDimensions({
                    width: graphContainerRef.current.clientWidth,
                    height: graphContainerRef.current.clientHeight,
                });
            }
        };
        updateDimensions();
        window.addEventListener('resize', updateDimensions);
        return () => window.removeEventListener('resize', updateDimensions);
    }, [activeTab]);

    // ─── Derived Data ──────────────────────────────────────────────

    // Most connected nodes (hub analysis)
    const mostConnected = useMemo((): MostConnected[] => {
        const connectionCount = new Map<string, number>();
        graphData.edges.forEach(e => {
            connectionCount.set(e.source, (connectionCount.get(e.source) || 0) + 1);
            connectionCount.set(e.target, (connectionCount.get(e.target) || 0) + 1);
        });
        return Array.from(connectionCount.entries())
            .sort((a, b) => b[1] - a[1])
            .slice(0, 8)
            .map(([id, count]) => {
                const node = graphData.nodes.find(n => n.id === id);
                return {
                    id,
                    label: node?.label || id,
                    connections: count,
                    type: node?.fileType || 'note',
                };
            });
    }, [graphData]);

    // Insights: busiest day, top tag, avg meeting length
    const insights = useMemo(() => {
        const topTag = tags.length > 0 ? `#${tags[0].name}` : '—';
        const topHub = mostConnected.length > 0 ? mostConnected[0].label : '—';

        // Calculate total meeting hours
        const totalSecs = meetings.reduce((sum, m) => sum + (m.duration_seconds || 0), 0);
        const totalHours = Math.round(totalSecs / 3600 * 10) / 10;

        return { topTag, topHub, totalHours };
    }, [tags, mostConnected, meetings]);

    // ─── Search ────────────────────────────────────────────────────

    const handleSearch = useCallback(async (query: string) => {
        if (!query.trim()) {
            setSearchResults([]);
            return;
        }
        setIsSearching(true);
        try {
            const results = await tauri.searchVault(query);
            setSearchResults(results);
            if (activeTab !== 'search') {
                setActiveTab('search');
            }
        } catch (err) {
            console.error('Search failed:', err);
        } finally {
            setIsSearching(false);
        }
    }, [activeTab]);

    const handleSearchKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === 'Enter') {
            handleSearch(searchQuery);
        }
    };

    // ─── Graph Interaction ─────────────────────────────────────────

    const handleNodeHover = useCallback((node: any) => {
        const newHighlightNodes = new Set<string>();
        const newHighlightLinks = new Set<string>();

        if (node) {
            newHighlightNodes.add(node.id);
            graphData.edges.forEach(link => {
                if (link.source === node.id || link.target === node.id) {
                    newHighlightLinks.add(`${link.source}-${link.target}`);
                    newHighlightNodes.add(link.source);
                    newHighlightNodes.add(link.target);
                }
            });
        }
        setHoverNode(node ? node.id : null);
        setHighlightNodes(newHighlightNodes);
        setHighlightLinks(newHighlightLinks);
    }, [graphData.edges]);

    // ForceGraph data
    const forceGraphData = useMemo(() => ({
        nodes: graphData.nodes.map(n => ({
            id: n.id,
            name: n.label,
            val: n.fileType === 'meeting' ? 6 : n.fileType === 'topic' ? 8 : n.fileType === 'company' ? 6 : 4,
            color: n.fileType === 'meeting' ? '#ffd700'
                : n.fileType === 'topic' ? '#818cf8'
                    : n.fileType === 'person' ? '#4a9eff'
                        : n.fileType === 'company' ? '#f97316'
                            : 'rgba(255,255,255,0.3)',
            type: n.fileType,
        })),
        links: graphData.edges.map(e => ({ source: e.source, target: e.target })),
    }), [graphData]);

    // ─── Helpers ───────────────────────────────────────────────────

    const getTagSize = (count: number): string => {
        const max = tags.length > 0 ? tags[0].fileCount : 1;
        const ratio = count / max;
        if (ratio > 0.7) return 'size-xl';
        if (ratio > 0.4) return 'size-lg';
        if (ratio > 0.15) return 'size-md';
        return 'size-sm';
    };

    const formatDate = (dateStr: string) => {
        const d = new Date(dateStr);
        return {
            month: d.toLocaleDateString('en-US', { month: 'short' }),
            day: d.getDate().toString(),
            full: d.toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' }),
        };
    };

    const formatDuration = (secs: number | null) => {
        if (!secs) return '';
        const h = Math.floor(secs / 3600);
        const m = Math.floor((secs % 3600) / 60);
        if (h > 0) return `${h}h ${m}m`;
        return m > 0 ? `${m}m` : `${secs}s`;
    };

    // ─── Graph Render Helpers ──────────────────────────────────────

    const renderMiniGraphNode = (node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
        const radius = node.type === 'meeting' ? 4 : node.type === 'topic' ? 5 : 3;
        ctx.beginPath();
        ctx.arc(node.x, node.y, radius, 0, 2 * Math.PI, false);
        ctx.fillStyle = node.color;
        ctx.shadowBlur = 5;
        ctx.shadowColor = node.color;
        ctx.fill();
        ctx.shadowBlur = 0;

        if (globalScale > 2) {
            const fontSize = 10 / globalScale;
            ctx.font = `${fontSize}px Inter, sans-serif`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillStyle = 'rgba(255,255,255,0.5)';
            ctx.fillText(node.name, node.x, node.y + radius + 2);
        }
    };

    const renderFullGraphNode = (node: any, ctx: CanvasRenderingContext2D, globalScale: number) => {
        const label = node.name;
        const fontSize = 12 / globalScale;
        ctx.font = `${fontSize}px Inter, sans-serif`;
        const isHighlighted = highlightNodes.has(node.id);
        const radius = node.type === 'meeting' ? 5
            : node.type === 'topic' ? 6
                : node.type === 'company' ? 5
                    : 3;

        if (isHighlighted || node.id === hoverNode) {
            ctx.beginPath();
            ctx.arc(node.x, node.y, radius * 1.8, 0, 2 * Math.PI, false);
            ctx.fillStyle = node.color + '33';
            ctx.fill();
        }

        ctx.beginPath();
        ctx.arc(node.x, node.y, radius, 0, 2 * Math.PI, false);
        ctx.fillStyle = node.color;
        ctx.shadowBlur = isHighlighted ? 15 : 5;
        ctx.shadowColor = node.color;
        ctx.fill();
        ctx.shadowBlur = 0;

        if (globalScale > 1.5 || isHighlighted) {
            const textWidth = ctx.measureText(label).width;
            const bckgDimensions = [textWidth, fontSize].map(n => n + fontSize * 0.2);
            ctx.fillStyle = 'rgba(0,0,0,0.6)';
            ctx.fillRect(
                node.x - bckgDimensions[0] / 2,
                node.y + radius + 2,
                bckgDimensions[0],
                bckgDimensions[1]
            );
            ctx.textAlign = 'center';
            ctx.textBaseline = 'top';
            ctx.fillStyle = isHighlighted ? '#fff' : 'rgba(255,255,255,0.6)';
            ctx.fillText(label, node.x, node.y + radius + 2);
        }
    };

    const nodePointerPaint = (node: any, color: string, ctx: CanvasRenderingContext2D) => {
        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.arc(node.x, node.y, 8, 0, 2 * Math.PI, false);
        ctx.fill();
    };

    const getLinkColor = (link: any) =>
        highlightLinks.has(`${link.source?.id || link.source}-${link.target?.id || link.target}`)
            ? '#FFB800'
            : 'rgba(255,255,255,0.06)';

    const getLinkWidth = (link: any) =>
        highlightLinks.has(`${link.source?.id || link.source}-${link.target?.id || link.target}`)
            ? 2 : 1;

    const getParticleWidth = (link: any) =>
        highlightLinks.has(`${link.source?.id || link.source}-${link.target?.id || link.target}`)
            ? 4 : 0;

    // ─── Render ────────────────────────────────────────────────────

    const tabs: { id: IntelTab; label: string; icon: React.ReactNode }[] = [
        { id: 'overview', label: 'Overview', icon: <Zap size={14} /> },
        { id: 'graph', label: 'Graph', icon: <Network size={14} /> },
        { id: 'timeline', label: 'Timeline', icon: <Clock size={14} /> },
        { id: 'people', label: 'People', icon: <Users size={14} /> },
        { id: 'topics', label: 'Topics', icon: <Hash size={14} /> },
        { id: 'search', label: 'Search', icon: <Search size={14} /> },
    ];

    return (
        <div className="intel-dashboard">
            {/* Top Bar */}
            <div className="intel-topbar">
                <div className="intel-topbar-left">
                    <h2>🔍 INTEL</h2>
                    <span className="intel-badge">Cross-Meeting Intelligence</span>
                </div>

                <div className="intel-search-bar">
                    <Search size={14} />
                    <input
                        type="text"
                        placeholder="Search across all meetings..."
                        value={searchQuery}
                        onChange={e => setSearchQuery(e.target.value)}
                        onKeyDown={handleSearchKeyDown}
                    />
                    {isSearching && <Loader2 size={14} className="spinning" />}
                </div>
            </div>

            {/* Tabs */}
            <div className="intel-tabs">
                {tabs.map(tab => (
                    <button
                        key={tab.id}
                        className={`intel-tab ${activeTab === tab.id ? 'active' : ''}`}
                        onClick={() => setActiveTab(tab.id)}
                    >
                        {tab.icon}
                        {tab.label}
                    </button>
                ))}
            </div>

            {/* Content */}
            <div className="intel-content">
                {isLoading ? (
                    <div className="intel-loading">
                        <div className="loading-spinner" />
                        <span>Loading intelligence data...</span>
                    </div>
                ) : (
                    <>
                        {/* ── Overview Tab ────────────────────────── */}
                        {activeTab === 'overview' && (
                            <>
                                {/* Stats Row */}
                                <div className="intel-overview">
                                    <div className="intel-stat-card gold">
                                        <span className="intel-stat-value">{meetings.length}</span>
                                        <span className="intel-stat-label">Meetings</span>
                                    </div>
                                    <div className="intel-stat-card blue">
                                        <span className="intel-stat-value">{people.length}</span>
                                        <span className="intel-stat-label">People</span>
                                    </div>
                                    <div className="intel-stat-card purple">
                                        <span className="intel-stat-value">{topics.length}</span>
                                        <span className="intel-stat-label">Topics</span>
                                    </div>
                                    <div className="intel-stat-card green">
                                        <span className="intel-stat-value">{graphData.edges.length}</span>
                                        <span className="intel-stat-label">Connections</span>
                                    </div>
                                </div>

                                {/* Insights Strip */}
                                <div className="intel-insights-strip">
                                    <div className="intel-insight-chip">
                                        <div className="intel-insight-icon gold-glow">
                                            <Clock size={16} />
                                        </div>
                                        <div className="intel-insight-text">
                                            <span className="intel-insight-label">Total Hours</span>
                                            <span className="intel-insight-value">{insights.totalHours}h recorded</span>
                                        </div>
                                    </div>
                                    <div className="intel-insight-chip">
                                        <div className="intel-insight-icon purple-glow">
                                            <Hash size={16} />
                                        </div>
                                        <div className="intel-insight-text">
                                            <span className="intel-insight-label">Top Tag</span>
                                            <span className="intel-insight-value">{insights.topTag}</span>
                                        </div>
                                    </div>
                                    <div className="intel-insight-chip">
                                        <div className="intel-insight-icon blue-glow">
                                            <TrendingUp size={16} />
                                        </div>
                                        <div className="intel-insight-text">
                                            <span className="intel-insight-label">Top Hub</span>
                                            <span className="intel-insight-value">{insights.topHub}</span>
                                        </div>
                                    </div>
                                </div>

                                {/* Mini Graph */}
                                <div className="intel-section">
                                    <div className="intel-section-header">
                                        <h3><Network size={14} /> Knowledge Graph</h3>
                                        <button className="intel-nav-link" onClick={() => setActiveTab('graph')}>
                                            Expand →
                                        </button>
                                    </div>
                                    <div className="intel-section-body" style={{ padding: 0 }}>
                                        <div className="intel-graph-container" ref={graphContainerRef} style={{ height: 280 }}>
                                            {graphData.nodes.length > 0 ? (
                                                <>
                                                    <ForceGraph2D
                                                        graphData={forceGraphData}
                                                        width={graphDimensions.width}
                                                        height={280}
                                                        nodeLabel="name"
                                                        nodeCanvasObject={renderMiniGraphNode}
                                                        nodePointerAreaPaint={nodePointerPaint}
                                                        linkColor={() => 'rgba(255,255,255,0.06)'}
                                                        linkWidth={1}
                                                        backgroundColor="rgba(0,0,0,0)"
                                                        d3AlphaDecay={0.04}
                                                        d3VelocityDecay={0.3}
                                                    />
                                                    <div className="intel-graph-legend">
                                                        <div className="intel-legend-item">
                                                            <span className="intel-legend-dot meeting" />
                                                            <span>Meeting</span>
                                                        </div>
                                                        <div className="intel-legend-item">
                                                            <span className="intel-legend-dot topic" />
                                                            <span>Topic</span>
                                                        </div>
                                                        <div className="intel-legend-item">
                                                            <span className="intel-legend-dot note" />
                                                            <span>Note</span>
                                                        </div>
                                                    </div>
                                                    <div className="intel-graph-stats">
                                                        <span className="graph-stat-badge">
                                                            <strong>{graphData.nodes.length}</strong> nodes
                                                        </span>
                                                        <span className="graph-stat-badge">
                                                            <strong>{graphData.edges.length}</strong> edges
                                                        </span>
                                                    </div>
                                                </>
                                            ) : (
                                                <div className="intel-empty">No graph data yet. Export meetings to your Obsidian vault first.</div>
                                            )}
                                        </div>
                                    </div>
                                </div>

                                {/* Most Connected Nodes */}
                                {mostConnected.length > 0 && (
                                    <div className="intel-section">
                                        <div className="intel-section-header">
                                            <h3><Link2 size={14} /> Most Connected</h3>
                                            <button className="intel-nav-link" onClick={() => setActiveTab('graph')}>
                                                View Graph →
                                            </button>
                                        </div>
                                        <div className="intel-section-body">
                                            <div className="intel-connections-grid">
                                                {mostConnected.map((node, i) => (
                                                    <div key={node.id} className="intel-connection-node">
                                                        <span className="intel-connection-rank">{i + 1}</span>
                                                        <div className="intel-connection-info">
                                                            <div className="intel-connection-name">{node.label}</div>
                                                            <div className="intel-connection-count">
                                                                {node.connections} connections · {node.type}
                                                            </div>
                                                        </div>
                                                    </div>
                                                ))}
                                            </div>
                                        </div>
                                    </div>
                                )}

                                {/* Upcoming Calendar */}
                                {calendarEvents.length > 0 && (
                                    <div className="intel-section">
                                        <div className="intel-section-header">
                                            <h3><CalendarDays size={14} /> Upcoming Meetings</h3>
                                        </div>
                                        <div className="intel-section-body">
                                            {lookupNeedsAi && <AiSetupNotice feature="Attendee lookups" compact />}
                                            {lookupError && <p className="intel-timeline-meta" role="alert">Lookup failed: {lookupError}</p>}
                                            <div className="intel-timeline">
                                                {calendarEvents.map((event: any) => {
                                                    const start = new Date(event.start_time);
                                                    const timeStr = start.toLocaleTimeString('en-US', {
                                                        hour: 'numeric', minute: '2-digit', hour12: true
                                                    });
                                                    const dateStr = start.toLocaleDateString('en-US', {
                                                        month: 'short', day: 'numeric'
                                                    });
                                                    const intel = eventIntel[event.event_id];
                                                    const isLooking = lookingUp[event.event_id];
                                                    return (
                                                        <div key={event.event_id} className="intel-timeline-item" style={{ flexDirection: 'column', gap: '8px' }}>
                                                            <div style={{ display: 'flex', alignItems: 'flex-start', gap: '12px' }}>
                                                                <div className="intel-timeline-marker">
                                                                    <div className="intel-timeline-date">
                                                                        {dateStr}
                                                                    </div>
                                                                    <div className="intel-timeline-dot" style={{ background: '#4ade80' }} />
                                                                </div>
                                                                <div className="intel-timeline-body" style={{ flex: 1 }}>
                                                                    <div className="intel-timeline-title">
                                                                        {event.title}
                                                                    </div>
                                                                    <div className="intel-timeline-meta">
                                                                        {timeStr}
                                                                        {event.attendee_count > 0 && ` · ${event.attendee_count} attendees`}
                                                                        {event.meeting_url && ' · 🔗'}
                                                                    </div>
                                                                </div>
                                                                {event.attendees && event.attendees.length > 0 && !intel && (
                                                                    <button
                                                                        className="intel-nav-link"
                                                                        disabled={isLooking}
                                                                        style={{ fontSize: '0.7rem', whiteSpace: 'nowrap' }}
                                                                        onClick={async () => {
                                                                            setLookingUp(prev => ({ ...prev, [event.event_id]: true }));
                                                                            setLookupError(null);
                                                                            setLookupNeedsAi(false);
                                                                            try {
                                                                                const emails = event.attendees.map((a: any) => a.email);
                                                                                const result = await tauri.lookupAttendees(event.title, emails);
                                                                                setEventIntel(prev => ({ ...prev, [event.event_id]: result }));
                                                                            } catch (err) {
                                                                                console.error('Lookup failed:', friendlyAiError(err));
                                                                                if (isNoProviderError(err)) setLookupNeedsAi(true);
                                                                                else setLookupError(friendlyAiError(err));
                                                                            } finally {
                                                                                setLookingUp(prev => ({ ...prev, [event.event_id]: false }));
                                                                            }
                                                                        }}
                                                                    >
                                                                        {isLooking ? '⏳ Looking up...' : '🔍 Lookup'}
                                                                    </button>
                                                                )}
                                                            </div>
                                                            {/* Attendee Intel Panel */}
                                                            {intel && (
                                                                <div style={{
                                                                    marginLeft: '52px',
                                                                    padding: '12px',
                                                                    borderRadius: '8px',
                                                                    background: 'rgba(74, 222, 128, 0.05)',
                                                                    border: '1px solid rgba(74, 222, 128, 0.15)',
                                                                    fontSize: '0.75rem',
                                                                }}>
                                                                    {/* Attendee profiles */}
                                                                    {intel.attendees?.map((person: any) => (
                                                                        <div key={person.email} style={{
                                                                            marginBottom: '8px',
                                                                            paddingBottom: '8px',
                                                                            borderBottom: '1px solid rgba(255,255,255,0.05)',
                                                                        }}>
                                                                            <div style={{ fontWeight: 600, color: 'var(--text-primary)' }}>
                                                                                {person.name}
                                                                                <span style={{ color: 'var(--text-tertiary)', fontWeight: 400, marginLeft: '6px' }}>
                                                                                    {person.company} · {person.email}
                                                                                </span>
                                                                            </div>
                                                                            <div style={{ color: 'var(--text-secondary)', marginTop: '4px', lineHeight: 1.4 }}>
                                                                                {person.briefing.substring(0, 200)}{person.briefing.length > 200 ? '...' : ''}
                                                                            </div>
                                                                        </div>
                                                                    ))}
                                                                    {/* Meeting prep */}
                                                                    {intel.meeting_prep && (
                                                                        <details style={{ marginTop: '8px' }}>
                                                                            <summary style={{ cursor: 'pointer', color: '#4ade80', fontWeight: 600 }}>
                                                                                📋 Meeting Prep Brief
                                                                            </summary>
                                                                            <div style={{
                                                                                marginTop: '8px',
                                                                                color: 'var(--text-secondary)',
                                                                                lineHeight: 1.5,
                                                                                whiteSpace: 'pre-wrap',
                                                                            }}>
                                                                                {intel.meeting_prep}
                                                                            </div>
                                                                        </details>
                                                                    )}
                                                                </div>
                                                            )}
                                                        </div>
                                                    );
                                                })}
                                            </div>
                                        </div>
                                    </div>
                                )}

                                {/* Recent Meetings */}
                                <div className="intel-section">
                                    <div className="intel-section-header">
                                        <h3><Clock size={14} /> Recent Meetings</h3>
                                        <button className="intel-nav-link" onClick={() => setActiveTab('timeline')}>
                                            View All →
                                        </button>
                                    </div>
                                    <div className="intel-section-body">
                                        <div className="intel-timeline">
                                            {meetings.slice(0, 5).map(m => {
                                                const date = formatDate(m.started_at);
                                                return (
                                                    <div key={m.id} className="intel-timeline-item">
                                                        <div className="intel-timeline-marker">
                                                            <div className="intel-timeline-date">
                                                                {date.month}<br />{date.day}
                                                            </div>
                                                            <div className="intel-timeline-dot" />
                                                        </div>
                                                        <div className="intel-timeline-body">
                                                            <div className="intel-timeline-title">
                                                                {m.title || 'Untitled Meeting'}
                                                            </div>
                                                            <div className="intel-timeline-meta">
                                                                {m.duration_seconds ? formatDuration(m.duration_seconds) : ''}
                                                            </div>
                                                        </div>
                                                    </div>
                                                );
                                            })}
                                            {meetings.length === 0 && (
                                                <div className="intel-empty">No meetings recorded yet.</div>
                                            )}
                                        </div>
                                    </div>
                                </div>

                                {/* Tag Cloud Preview */}
                                {tags.length > 0 && (
                                    <div className="intel-section">
                                        <div className="intel-section-header">
                                            <h3><Hash size={14} /> Top Tags</h3>
                                            <button className="intel-nav-link" onClick={() => setActiveTab('topics')}>
                                                View All →
                                            </button>
                                        </div>
                                        <div className="intel-section-body">
                                            <div className="intel-tag-cloud">
                                                {tags.slice(0, 15).map(tag => (
                                                    <div
                                                        key={tag.name}
                                                        className={`intel-tag ${getTagSize(tag.fileCount)}`}
                                                        onClick={() => {
                                                            setSearchQuery(`#${tag.name}`);
                                                            handleSearch(`#${tag.name}`);
                                                        }}
                                                    >
                                                        <span className="tag-name">#{tag.name}</span>
                                                        <span className="tag-count">{tag.fileCount}</span>
                                                    </div>
                                                ))}
                                            </div>
                                        </div>
                                    </div>
                                )}
                            </>
                        )}

                        {/* ── Full Graph Tab ─────────────────────── */}
                        {activeTab === 'graph' && (
                            <div className="intel-section">
                                <div className="intel-section-header">
                                    <h3><Network size={14} /> Vault Knowledge Graph</h3>
                                    <button className="intel-refresh-btn" onClick={loadAllData}>
                                        <RefreshCcw size={12} /> Refresh
                                    </button>
                                </div>
                                <div className="intel-section-body" style={{ padding: 0 }}>
                                    <div className="intel-graph-container" ref={graphContainerRef} style={{ height: 500 }}>
                                        {graphData.nodes.length > 0 ? (
                                            <>
                                                <ForceGraph2D
                                                    graphData={forceGraphData}
                                                    width={graphDimensions.width}
                                                    height={500}
                                                    nodeLabel="name"
                                                    nodeCanvasObject={renderFullGraphNode}
                                                    nodePointerAreaPaint={nodePointerPaint}
                                                    linkColor={getLinkColor}
                                                    linkWidth={getLinkWidth}
                                                    linkDirectionalParticles={2}
                                                    linkDirectionalParticleWidth={getParticleWidth}
                                                    onNodeHover={handleNodeHover}
                                                    backgroundColor="rgba(0,0,0,0)"
                                                    d3AlphaDecay={0.02}
                                                    d3VelocityDecay={0.3}
                                                />
                                                <div className="intel-graph-legend">
                                                    <div className="intel-legend-item">
                                                        <span className="intel-legend-dot meeting" />
                                                        <span>Meeting</span>
                                                    </div>
                                                    <div className="intel-legend-item">
                                                        <span className="intel-legend-dot topic" />
                                                        <span>Topic</span>
                                                    </div>
                                                    <div className="intel-legend-item">
                                                        <span className="intel-legend-dot person" />
                                                        <span>Person</span>
                                                    </div>
                                                    <div className="intel-legend-item">
                                                        <span className="intel-legend-dot company" />
                                                        <span>Company</span>
                                                    </div>
                                                    <div className="intel-legend-item">
                                                        <span className="intel-legend-dot note" />
                                                        <span>Note</span>
                                                    </div>
                                                </div>
                                                <div className="intel-graph-stats">
                                                    <span className="graph-stat-badge">
                                                        <strong>{graphData.nodes.length}</strong> nodes
                                                    </span>
                                                    <span className="graph-stat-badge">
                                                        <strong>{graphData.edges.length}</strong> edges
                                                    </span>
                                                </div>
                                            </>
                                        ) : (
                                            <div className="intel-empty">
                                                No graph data. Export meetings to your vault to build the knowledge graph.
                                            </div>
                                        )}
                                    </div>
                                </div>
                            </div>
                        )}

                        {/* ── Timeline Tab ────────────────────────── */}
                        {activeTab === 'timeline' && (
                            <div className="intel-section">
                                <div className="intel-section-header">
                                    <h3><Clock size={14} /> Meeting Timeline</h3>
                                    <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.35)' }}>
                                        {meetings.length} meetings · {insights.totalHours}h total
                                    </span>
                                </div>
                                <div className="intel-section-body">
                                    <div className="intel-timeline">
                                        {meetings.map(m => {
                                            const date = formatDate(m.started_at);
                                            const matchingTopic = topics.find(t =>
                                                t.meetings.some(tm =>
                                                    tm.toLowerCase().includes((m.title || '').toLowerCase().substring(0, 20))
                                                )
                                            );
                                            return (
                                                <div key={m.id} className="intel-timeline-item">
                                                    <div className="intel-timeline-marker">
                                                        <div className="intel-timeline-date">
                                                            {date.month}<br />{date.day}
                                                        </div>
                                                        <div className="intel-timeline-dot" />
                                                    </div>
                                                    <div className="intel-timeline-body">
                                                        <div className="intel-timeline-title">
                                                            {m.title || 'Untitled Meeting'}
                                                        </div>
                                                        <div className="intel-timeline-meta">
                                                            {m.duration_seconds ? formatDuration(m.duration_seconds) : ''}
                                                            {matchingTopic && (
                                                                <span className="intel-timeline-topic-chip">
                                                                    {matchingTopic.name}
                                                                </span>
                                                            )}
                                                        </div>
                                                    </div>
                                                </div>
                                            );
                                        })}
                                        {meetings.length === 0 && (
                                            <div className="intel-empty">No meetings recorded yet. Start a recording to begin building your timeline.</div>
                                        )}
                                    </div>
                                </div>
                            </div>
                        )}

                        {/* ── People & Companies Tab ──────────────── */}
                        {activeTab === 'people' && (
                            <>
                                {/* People */}
                                <div className="intel-section">
                                    <div className="intel-section-header">
                                        <h3><Users size={14} /> People</h3>
                                        <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.35)' }}>
                                            {people.length} contacts
                                        </span>
                                    </div>
                                    <div className="intel-section-body">
                                        {people.length > 0 ? (
                                            <div className="intel-people-grid">
                                                {people.map(p => (
                                                    <div key={p.name} className="intel-person-card">
                                                        <div className="intel-person-avatar">{p.initials}</div>
                                                        <div className="intel-person-name">{p.name}</div>
                                                        <div className="intel-person-company">
                                                            <Building2 size={11} /> {p.company || 'Unknown'}
                                                        </div>
                                                        <div className="intel-person-stats">
                                                            <div className="intel-person-stat">
                                                                <strong>{p.meetingCount}</strong>
                                                                Mentions
                                                            </div>
                                                        </div>
                                                    </div>
                                                ))}
                                            </div>
                                        ) : (
                                            <div className="intel-empty">
                                                No people tracked yet. Generate meeting intel from your calendar to populate contacts.
                                            </div>
                                        )}
                                    </div>
                                </div>

                                {/* Companies */}
                                <div className="intel-section">
                                    <div className="intel-section-header">
                                        <h3><Building2 size={14} /> Companies</h3>
                                        <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.35)' }}>
                                            {companies.length} organizations
                                        </span>
                                    </div>
                                    <div className="intel-section-body">
                                        {companies.length > 0 ? (
                                            <div className="intel-people-grid">
                                                {companies.map(c => (
                                                    <div key={c.name} className="intel-company-card">
                                                        <div className="intel-company-name">{c.name}</div>
                                                        <div className="intel-company-domain">{c.domain}</div>
                                                        {c.people.length > 0 && (
                                                            <div className="intel-company-people">
                                                                {c.people.map(p => (
                                                                    <span key={p} className="intel-company-person-chip">{p}</span>
                                                                ))}
                                                            </div>
                                                        )}
                                                    </div>
                                                ))}
                                            </div>
                                        ) : (
                                            <div className="intel-empty">
                                                No companies tracked yet. Generate meeting intel to populate organizations.
                                            </div>
                                        )}
                                    </div>
                                </div>
                            </>
                        )}

                        {/* ── Topics & Tags Tab ──────────────────── */}
                        {activeTab === 'topics' && (
                            <>
                                {/* Tag Cloud */}
                                {tags.length > 0 && (
                                    <div className="intel-section">
                                        <div className="intel-section-header">
                                            <h3><Hash size={14} /> Tag Cloud</h3>
                                            <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.35)' }}>
                                                {tags.length} tags
                                            </span>
                                        </div>
                                        <div className="intel-section-body">
                                            <div className="intel-tag-cloud">
                                                {tags.map(tag => (
                                                    <div
                                                        key={tag.name}
                                                        className={`intel-tag ${getTagSize(tag.fileCount)}`}
                                                        onClick={() => {
                                                            setSearchQuery(`#${tag.name}`);
                                                            handleSearch(`#${tag.name}`);
                                                        }}
                                                    >
                                                        <span className="tag-name">#{tag.name}</span>
                                                        <span className="tag-count">{tag.fileCount}</span>
                                                    </div>
                                                ))}
                                            </div>
                                        </div>
                                    </div>
                                )}

                                {/* Topic Cards */}
                                <div className="intel-section">
                                    <div className="intel-section-header">
                                        <h3><Folder size={14} /> Topics</h3>
                                        <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.35)' }}>
                                            {topics.length} topics
                                        </span>
                                    </div>
                                    <div className="intel-section-body">
                                        {topics.length > 0 ? (
                                            <div className="intel-topic-grid">
                                                {topics.map(topic => (
                                                    <div key={topic.name} className="intel-topic-card">
                                                        <div className="intel-topic-card-name">{topic.name}</div>
                                                        <div className="intel-topic-card-stats">
                                                            <div className="intel-topic-card-stat">
                                                                <strong>{topic.meetings.length}</strong>
                                                                Meetings
                                                            </div>
                                                            <div className="intel-topic-card-stat">
                                                                <strong>{topic.noteCount}</strong>
                                                                Notes
                                                            </div>
                                                        </div>
                                                        {topic.tags.length > 0 && (
                                                            <div className="intel-topic-tags">
                                                                {topic.tags.map(t => (
                                                                    <span key={t} className="intel-topic-tag">#{t}</span>
                                                                ))}
                                                            </div>
                                                        )}
                                                    </div>
                                                ))}
                                            </div>
                                        ) : (
                                            <div className="intel-empty">
                                                No topics created yet. Create topics in the Vault tab to organize your meetings.
                                            </div>
                                        )}
                                    </div>
                                </div>
                            </>
                        )}

                        {/* ── Search Tab ──────────────────────────── */}
                        {activeTab === 'search' && (
                            <div className="intel-section">
                                <div className="intel-section-header">
                                    <h3><Search size={14} /> Cross-Meeting Search</h3>
                                    <span style={{ fontSize: 11, color: 'rgba(255,255,255,0.35)' }}>
                                        {searchResults.length} results
                                    </span>
                                </div>
                                <div className="intel-section-body">
                                    {searchResults.length > 0 ? (
                                        <div className="intel-search-results">
                                            {searchResults.map((r, i) => (
                                                <div key={`${r.filePath}-${r.lineNumber}-${i}`} className="intel-search-result">
                                                    <div className="intel-search-result-file">
                                                        <FileText size={12} />
                                                        {r.fileName}
                                                    </div>
                                                    <div className="intel-search-result-line">
                                                        L{r.lineNumber}
                                                    </div>
                                                    <div className="intel-search-result-context">
                                                        {r.context}
                                                    </div>
                                                </div>
                                            ))}
                                        </div>
                                    ) : (
                                        <div className="intel-search-empty">
                                            <Search size={32} />
                                            {searchQuery
                                                ? 'No results found. Try a different search term.'
                                                : 'Type a query and press Enter to search across all meeting notes, transcripts, and vault files.'}
                                        </div>
                                    )}
                                </div>
                            </div>
                        )}
                    </>
                )}
            </div>
        </div>
    );
};
