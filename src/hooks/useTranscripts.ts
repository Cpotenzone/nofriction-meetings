// noFriction Meetings - Transcripts Hook
// Manages transcript state and real-time updates

import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import * as tauri from "../lib/tauri";
import type { TranscriptEvent, Transcript, SearchResult } from "../lib/tauri";

export interface LiveTranscript {
    id: string;
    text: string;
    timestamp: Date;
    isFinal: boolean;
    confidence: number;
    speaker: string | null;
}

export function useTranscripts(meetingId: string | null) {
    const [liveTranscripts, setLiveTranscripts] = useState<LiveTranscript[]>([]);
    const [savedTranscripts, setSavedTranscripts] = useState<Transcript[]>([]);
    const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
    const [isSearching, setIsSearching] = useState(false);

    // Track the current interim transcript text to detect duplicates
    const lastInterimRef = useRef<string>("");
    const transcriptIdCounter = useRef<number>(0);

    // Listen for live transcript events
    useEffect(() => {
        let unlisten: (() => void) | null = null;

        const setupListener = async () => {
            console.log("Setting up live_transcript listener...");

            unlisten = await listen<TranscriptEvent>("live_transcript", (event) => {
                const { text, is_final, confidence, speaker } = event.payload;

                // Skip empty transcripts
                if (!text || text.trim() === "") {
                    return;
                }

                console.log(`Transcript received: "${text}" (final: ${is_final})`);

                setLiveTranscripts((prev) => {
                    if (is_final) {
                        // Final transcript - add it and clear interim
                        lastInterimRef.current = "";

                        // Check if this exact text was already added as final
                        const isDuplicate = prev.some(
                            (t) => t.isFinal && t.text === text
                        );

                        if (isDuplicate) {
                            return prev;
                        }

                        // Remove any interim transcripts that are similar to this final one
                        const filtered = prev.filter((t) => t.isFinal);

                        const newTranscript: LiveTranscript = {
                            id: `final-${++transcriptIdCounter.current}`,
                            text,
                            timestamp: new Date(),
                            isFinal: true,
                            confidence,
                            speaker,
                        };

                        return [...filtered, newTranscript].slice(-50);
                    } else {
                        // Interim transcript - update the preview
                        // Don't add if it's the same as the last interim
                        if (text === lastInterimRef.current) {
                            return prev;
                        }
                        lastInterimRef.current = text;

                        // Replace any existing interim with this one
                        const finals = prev.filter((t) => t.isFinal);

                        const interimTranscript: LiveTranscript = {
                            id: `interim-${++transcriptIdCounter.current}`,
                            text,
                            timestamp: new Date(),
                            isFinal: false,
                            confidence,
                            speaker,
                        };

                        return [...finals, interimTranscript].slice(-50);
                    }
                });
            });

            console.log("live_transcript listener ready");
        };

        setupListener();

        return () => {
            if (unlisten) {
                console.log("Cleaning up live_transcript listener");
                unlisten();
            }
        };
    }, []);

    // Load saved transcripts when meeting changes
    useEffect(() => {
        if (meetingId) {
            loadTranscripts(meetingId);
        } else {
            setSavedTranscripts([]);
        }
    }, [meetingId]);

    const loadTranscripts = useCallback(async (id: string) => {
        try {
            const transcripts = await tauri.getTranscripts(id);
            setSavedTranscripts(transcripts);
        } catch (err) {
            console.error("Failed to load transcripts:", err);
        }
    }, []);

    const search = useCallback(async (query: string) => {
        if (!query.trim()) {
            setSearchResults([]);
            return;
        }

        setIsSearching(true);
        try {
            const results = await tauri.searchTranscripts(query);
            setSearchResults(results);
        } catch (err) {
            console.error("Search failed:", err);
            setSearchResults([]);
        } finally {
            setIsSearching(false);
        }
    }, []);

    const clearLiveTranscripts = useCallback(() => {
        setLiveTranscripts([]);
        lastInterimRef.current = "";
        transcriptIdCounter.current = 0;
    }, []);

    return {
        liveTranscripts,
        savedTranscripts,
        searchResults,
        isSearching,
        search,
        clearLiveTranscripts,
        loadTranscripts,
    };
}
