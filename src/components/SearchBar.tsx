// noFriction Meetings - Search Bar Component
// Full-text search across transcripts

import { useState, useCallback, useRef } from "react";

interface SearchBarProps {
    onSearch: (query: string) => void;
    isSearching: boolean;
}

export function SearchBar({ onSearch, isSearching }: SearchBarProps) {
    const [query, setQuery] = useState("");
    const debounceRef = useRef<number | null>(null);

    const handleChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) => {
            const value = e.target.value;
            setQuery(value);

            // Debounce search
            if (debounceRef.current) {
                clearTimeout(debounceRef.current);
            }

            debounceRef.current = window.setTimeout(() => {
                onSearch(value);
            }, 300);
        },
        [onSearch]
    );

    const handleClear = () => {
        setQuery("");
        onSearch("");
    };

    return (
        <div className="search-container">
            <svg
                className="search-icon"
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
            >
                <circle cx="11" cy="11" r="8" />
                <path d="M21 21l-4.35-4.35" />
            </svg>
            <input
                type="text"
                className="search-input"
                placeholder="Search transcripts..."
                value={query}
                onChange={handleChange}
            />
            {query && (
                <button
                    onClick={handleClear}
                    style={{
                        position: "absolute",
                        right: "12px",
                        top: "50%",
                        transform: "translateY(-50%)",
                        background: "none",
                        border: "none",
                        color: "var(--text-tertiary)",
                        cursor: "pointer",
                        padding: "4px",
                    }}
                >
                    ✕
                </button>
            )}
            {isSearching && (
                <div
                    style={{
                        position: "absolute",
                        right: "40px",
                        top: "50%",
                        transform: "translateY(-50%)",
                        width: "16px",
                        height: "16px",
                        border: "2px solid var(--accent-primary)",
                        borderTopColor: "transparent",
                        borderRadius: "50%",
                        animation: "spin 1s linear infinite",
                    }}
                />
            )}
            <style>{`
        @keyframes spin {
          to { transform: translateY(-50%) rotate(360deg); }
        }
      `}</style>
        </div>
    );
}
