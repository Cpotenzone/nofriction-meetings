
import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Entity {
    id: number;
    activity_id: number;
    entity_type: string;
    name: string;
    metadata: any;
    confidence: number;
    theme: string | null;
    created_at: string;
}

interface EntitiesViewProps {
    className?: string;
    activeTheme?: string;
}

export const EntitiesView: React.FC<EntitiesViewProps> = ({ className, activeTheme: propTheme }) => {
    const [entities, setEntities] = useState<Entity[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [filterType, setFilterType] = useState<string>('all');
    const [currentTheme, setCurrentTheme] = useState<string | undefined>(propTheme);

    // Load active theme if not provided
    useEffect(() => {
        if (!propTheme) {
            invoke<string>('get_active_theme')
                .then(theme => setCurrentTheme(theme))
                .catch(console.error);
        } else {
            setCurrentTheme(propTheme);
        }
    }, [propTheme]);

    // Load entities
    const loadEntities = async () => {
        setLoading(true);
        try {
            const result = await invoke<Entity[]>('get_recent_entities', {
                limit: 100,
                theme: currentTheme !== 'all' ? currentTheme : null,
            });
            setEntities(result);
            setError(null);
        } catch (e) {
            console.error('Failed to load entities:', e);
            setError(String(e));
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        loadEntities();
        // Poll every 30 seconds for updates
        const interval = setInterval(loadEntities, 30000);
        return () => clearInterval(interval);
    }, [currentTheme]);

    // Group entities by type
    const groupedEntities = entities.reduce((acc, entity) => {
        const type = entity.entity_type || 'other';
        if (!acc[type]) acc[type] = [];
        acc[type].push(entity);
        return acc;
    }, {} as Record<string, Entity[]>);

    const entityTypes = Object.keys(groupedEntities).sort();

    // Filter display
    const displayTypes = filterType === 'all'
        ? entityTypes
        : entityTypes.filter(t => t === filterType);

    const getConfidenceColor = (conf: number) => {
        if (conf >= 0.8) return 'bg-green-100 text-green-800 border-green-200';
        if (conf >= 0.5) return 'bg-yellow-100 text-yellow-800 border-yellow-200';
        return 'bg-red-100 text-red-800 border-red-200';
    };

    return (
        <div className={`p-6 h-full overflow-y-auto ${className}`}>
            <div className="flex justify-between items-center mb-6">
                <div>
                    <h2 className="text-2xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-blue-600 to-purple-600">
                        Intel Dashboard
                    </h2>
                    <p className="text-gray-500 text-sm mt-1">
                        Entities extracted from your {currentTheme ? currentTheme : 'recent'} activity
                    </p>
                </div>

                <div className="flex gap-2">
                    <button
                        onClick={loadEntities}
                        className="p-2 text-gray-400 hover:text-gray-600 rounded-full hover:bg-gray-100 transition-colors"
                        title="Refresh"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /><path d="M3 3v5h5" /><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16" /><path d="M16 16h5v5" /></svg>
                    </button>

                    <select
                        value={filterType}
                        onChange={(e) => setFilterType(e.target.value)}
                        className="text-sm border border-gray-200 rounded-lg px-3 py-1.5 focus:outline-none focus:ring-2 focus:ring-blue-500"
                    >
                        <option value="all">All Types</option>
                        {entityTypes.map(t => (
                            <option key={t} value={t}>{t.charAt(0).toUpperCase() + t.slice(1).replace('_', ' ')}</option>
                        ))}
                    </select>
                </div>
            </div>

            {loading && entities.length === 0 ? (
                <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 animate-pulse">
                    {[1, 2, 3, 4, 5, 6].map(i => (
                        <div key={i} className="h-40 bg-gray-100 rounded-xl"></div>
                    ))}
                </div>
            ) : error ? (
                <div className="p-4 bg-red-50 text-red-600 rounded-lg border border-red-100">
                    Error loading intel: {error}
                </div>
            ) : entities.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-20 text-gray-400">
                    <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" strokeLinecap="round" strokeLinejoin="round" className="mb-4"><circle cx="12" cy="12" r="10" /><path d="M16.2 7.8 12 12l-4.2 4.2" /><path d="M12 2v20" /><path d="M2 12h20" /></svg>
                    <p>No activity intel found yet.</p>
                    <p className="text-sm mt-2">Try switching themes or performing some work.</p>
                </div>
            ) : (
                <div className="space-y-8">
                    {displayTypes.map(type => (
                        <div key={type} className="animate-in fade-in slide-in-from-bottom-4 duration-500">
                            <h3 className="text-sm font-semibold text-gray-500 uppercase tracking-wider mb-3 ml-1 flex items-center gap-2">
                                <span className="w-2 h-2 rounded-full bg-blue-500"></span>
                                {type.replace('_', ' ')}s
                            </h3>

                            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4">
                                {groupedEntities[type].map(entity => (
                                    <div
                                        key={entity.id}
                                        className="group bg-white rounded-xl border border-gray-100 shadow-sm hover:shadow-md transition-all p-4 relative overflow-hidden"
                                    >
                                        <div className="flex justify-between items-start mb-2">
                                            <h4 className="font-medium text-gray-900 truncate pr-6" title={entity.name}>
                                                {entity.name}
                                            </h4>
                                            <span className={`text-[10px] px-1.5 py-0.5 rounded border uppercase font-medium ${getConfidenceColor(entity.confidence)}`}>
                                                {Math.round(entity.confidence * 100)}%
                                            </span>
                                        </div>

                                        {/* Metadata rendering */}
                                        {entity.metadata && typeof entity.metadata === 'object' && (
                                            <div className="text-xs text-gray-500 space-y-1 mb-2">
                                                {Object.entries(entity.metadata)
                                                    .filter(([k]) => !['name', 'confidence', 'type'].includes(k))
                                                    .slice(0, 3)
                                                    .map(([key, val]) => (
                                                        <div key={key} className="flex gap-1 truncate">
                                                            <span className="opacity-70">{key}:</span>
                                                            <span className="font-medium">{String(val)}</span>
                                                        </div>
                                                    ))
                                                }
                                            </div>
                                        )}

                                        <div className="mt-3 pt-3 border-t border-gray-50 flex justify-between items-center text-[10px] text-gray-400">
                                            <span>{new Date(entity.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
                                            {entity.theme && (
                                                <span className="capitalize bg-gray-50 px-1.5 py-0.5 rounded text-gray-500">
                                                    {entity.theme}
                                                </span>
                                            )}
                                        </div>
                                    </div>
                                ))}
                            </div>
                        </div>
                    ))}
                </div>
            )}
        </div>
    );
};
