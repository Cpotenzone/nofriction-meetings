// noFriction Meetings - Transcripts Hook
// Manages transcript state and real-time updates

import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import * as tauri from "../lib/tauri";
import type { TranscriptEvent, Transcript, SearchResult } from "../lib/tauri";
import { renderPlain } from "../lib/redaction";

export interface LiveTranscript {
    id: string;
    text: string;
    timestamp: Date;
    isFinal: boolean;
    confidence: number;
    speaker: string | null;
    utteranceId?: string | null;
}

export function useTranscripts(meetingId: string | null) {
    const [liveTranscripts, setLiveTranscripts] = useState<LiveTranscript[]>([]);
    const [savedTranscripts, setSavedTranscripts] = useState<Transcript[]>([]);
    const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
    const [isSearching, setIsSearching] = useState(false);

    const transcriptIdCounter = useRef<number>(0);

    // Listen for live transcript events.
    //
    // Interim (is_final=false) text is a moving hypothesis for the utterance
    // currently being spoken; it is replaced in place until that utterance's
    // final segment(s) arrive. Providers that send utterance_id get exact
    // replacement; others fall back to "one interim at a time".
    useEffect(() => {
        const unlisteners: Array<() => void> = [];
        let disposed = false;

        const setup = async () => {
            const offTranscript = await listen<TranscriptEvent>("live_transcript", (event) => {
                const { text, is_final, confidence, speaker, start, utterance_id } = event.payload;
                if (!text || text.trim() === "") return;

                // Local Whisper sends epoch seconds; cloud providers send stream-relative
                const timestamp = start && start > 1e9 ? new Date(start * 1000) : new Date();
                const key = utterance_id ?? null;

                setLiveTranscripts((prev) => {
                    // Drop the interim this event supersedes
                    const withoutInterim = prev.filter(
                        (t) => t.isFinal || (key !== null && t.utteranceId !== key)
                    );

                    if (is_final) {
                        // Guard against a provider re-sending the same final segment
                        const echo = prev.some(
                            (t) =>
                                t.isFinal &&
                                t.text === text &&
                                Math.abs(t.timestamp.getTime() - timestamp.getTime()) < 2000
                        );
                        if (echo) return withoutInterim;

                        const finalT: LiveTranscript = {
                            id: `final-${++transcriptIdCounter.current}`,
                            text,
                            timestamp,
                            isFinal: true,
                            confidence,
                            speaker,
                            utteranceId: key,
                        };
                        return [...withoutInterim, finalT].slice(-500);
                    }

                    const existing = prev.find((t) => !t.isFinal && t.utteranceId === key);
                    if (existing?.text === text) return prev;
                    const interim: LiveTranscript = {
                        // Stable id per utterance so React updates the line in place
                        id: key ? `interim-${key}` : "interim",
                        text,
                        timestamp: existing?.timestamp ?? timestamp,
                        isFinal: false,
                        confidence,
                        speaker,
                        utteranceId: key,
                    };
                    const finals = key === null ? withoutInterim.filter((t) => t.isFinal) : withoutInterim;
                    return [...finals, interim].slice(-500);
                });
            });

            // Utterance resolved to nothing (noise, hallucination filtered)
            const offDiscard = await listen<{ utterance_id: string }>("live_transcript_discard", (event) => {
                const key = event.payload.utterance_id;
                setLiveTranscripts((prev) => prev.filter((t) => t.isFinal || t.utteranceId !== key));
            });

            if (disposed) {
                offTranscript();
                offDiscard();
            } else {
                unlisteners.push(offTranscript, offDiscard);
            }
        };

        setup();

        return () => {
            disposed = true;
            unlisteners.forEach((off) => off());
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
            // This view draws plain text: show strike markers as the placeholder
            setSavedTranscripts(transcripts.map((t) => ({ ...t, text: renderPlain(t.text) })));
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
