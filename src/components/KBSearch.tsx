// noFriction Meetings - Knowledge Base Search Component
// Search across local SQLite, Pinecone, and Supabase

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface KBSearchResult {
    id: string;
    source: string;
    timestamp: string | null;
    app_name: string | null;
    category: string | null;
    summary: string;
    score: number | null;
}

interface SearchOptions {
    query?: string;
    start_date?: string;
    end_date?: string;
    category?: string;
    limit?: number;
    sources?: string[];
}

export function KBSearch() {
    const [query, setQuery] = useState("");
    const [results, setResults] = useState<KBSearchResult[]>([]);
    const [isSearching, setIsSearching] = useState(false);
    const [searchSources, setSearchSources] = useState<string[]>(["local", "pinecone"]);
    const [error, setError] = useState<string | null>(null);

    const handleSearch = async () => {
        if (!query.trim()) return;

        setIsSearching(true);
        setError(null);

        try {
            const options: SearchOptions = {
                query: query.trim(),
                limit: 20,
                sources: searchSources,
            };

            const searchResults = await invoke<KBSearchResult[]>("search_knowledge_base", { options });
            setResults(searchResults);
        } catch (err) {
            console.error("Search failed:", err);
            setError(String(err));
            setResults([]);
        } finally {
            setIsSearching(false);
        }
    };

    const handleKeyDown = (e: React.KeyboardEvent) => {
        if (e.key === "Enter") {
            handleSearch();
        }
    };

    const toggleSource = (source: string) => {
        setSearchSources((prev) =>
            prev.includes(source)
                ? prev.filter((s) => s !== source)
                : [...prev, source]
        );
    };

    const formatTimestamp = (ts: string | null) => {
        if (!ts) return "";
        try {
            return new Date(ts).toLocaleString();
        } catch {
            return ts;
        }
    };

    const getSourceIcon = (source: string) => {
        switch (source) {
            case "local": return "💾";
            case "pinecone": return "🌲";
            case "supabase": return "🐘";
            default: return "📄";
        }
    };

    return (
        <div className="kb-search">
            {/* Search Header */}
            <div className="search-header">
                <h3>
                    <span className="icon">🔍</span>
                    Knowledge Base Search
                </h3>
            </div>

            {/* Search Input */}
            <div className="search-input-container" style={{ display: "flex", gap: "var(--spacing-sm)", marginBottom: "var(--spacing-md)" }}>
                <input
                    type="text"
                    className="settings-input"
                    placeholder="Search your knowledge base..."
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    onKeyDown={handleKeyDown}
                    style={{ flex: 1 }}
                />
                <button
                    className="btn btn-primary"
                    onClick={handleSearch}
                    disabled={isSearching || !query.trim()}
                >
                    {isSearching ? "Searching..." : "Search"}
                </button>
            </div>

            {/* Source Filters */}
            <div className="search-filters" style={{ display: "flex", gap: "var(--spacing-sm)", marginBottom: "var(--spacing-md)" }}>
                <span style={{ color: "var(--text-secondary)", fontSize: "0.875rem" }}>Sources:</span>
                {["local", "pinecone", "supabase"].map((source) => (
                    <button
                        key={source}
                        className={`filter-chip ${searchSources.includes(source) ? "active" : ""}`}
                        onClick={() => toggleSource(source)}
                        style={{
                            padding: "4px 12px",
                            borderRadius: "20px",
                            border: "1px solid var(--border)",
                            background: searchSources.includes(source) ? "var(--primary)" : "transparent",
                            color: searchSources.includes(source) ? "white" : "var(--text-secondary)",
                            cursor: "pointer",
                            fontSize: "0.75rem",
                        }}
                    >
                        {getSourceIcon(source)} {source}
                    </button>
                ))}
            </div>

            {/* Error Message */}
            {error && (
                <div style={{ padding: "var(--spacing-md)", background: "var(--error-bg)", borderRadius: "8px", marginBottom: "var(--spacing-md)" }}>
                    <span style={{ color: "var(--error)" }}>⚠️ {error}</span>
                </div>
            )}

            {/* Results */}
            <div className="search-results" style={{ display: "flex", flexDirection: "column", gap: "var(--spacing-sm)" }}>
                {results.length === 0 && !isSearching && query && (
                    <div style={{ textAlign: "center", padding: "var(--spacing-lg)", color: "var(--text-tertiary)" }}>
                        No results found
                    </div>
                )}

                {results.map((result, idx) => (
                    <div
                        key={`${result.source}-${result.id}-${idx}`}
                        className="search-result-item"
                        style={{
                            padding: "var(--spacing-md)",
                            background: "var(--bg-secondary)",
                            borderRadius: "8px",
                            border: "1px solid var(--border)",
                        }}
                    >
                        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "var(--spacing-xs)" }}>
                            <div style={{ display: "flex", gap: "var(--spacing-sm)", alignItems: "center" }}>
                                <span>{getSourceIcon(result.source)}</span>
                                <span style={{ fontSize: "0.75rem", color: "var(--text-tertiary)" }}>{result.source}</span>
                                {result.category && (
                                    <span style={{ fontSize: "0.75rem", padding: "2px 8px", background: "var(--primary)", borderRadius: "10px", color: "white" }}>
                                        {result.category}
                                    </span>
                                )}
                            </div>
                            {result.score !== null && (
                                <span style={{ fontSize: "0.75rem", color: "var(--text-secondary)" }}>
                                    Score: {(result.score * 100).toFixed(0)}%
                                </span>
                            )}
                        </div>

                        <div style={{ marginBottom: "var(--spacing-xs)" }}>
                            <span style={{ fontWeight: 500 }}>{result.summary || "(No summary)"}</span>
                        </div>

                        <div style={{ fontSize: "0.75rem", color: "var(--text-tertiary)", display: "flex", gap: "var(--spacing-md)" }}>
                            {result.app_name && <span>📱 {result.app_name}</span>}
                            {result.timestamp && <span>🕒 {formatTimestamp(result.timestamp)}</span>}
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}
