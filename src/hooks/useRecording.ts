// noFriction Meetings - Recording Hook
// Manages recording state and audio capture

import { useState, useEffect, useCallback, useRef } from "react";
import * as tauri from "../lib/tauri";


export interface RecordingState {
    isRecording: boolean;
    isPaused: boolean;
    meetingId: string | null;
    duration: number;
    videoFrames: number;
    audioSamples: number;
}

export function useRecording() {
    const [state, setState] = useState<RecordingState>({
        isRecording: false,
        isPaused: false,
        meetingId: null,
        duration: 0,
        videoFrames: 0,
        audioSamples: 0,
    });
    const [error, setError] = useState<string | null>(null);
    const intervalRef = useRef<number | null>(null);

    // Poll recording status while recording
    useEffect(() => {
        if (state.isRecording && !state.isPaused) {
            intervalRef.current = window.setInterval(async () => {
                try {
                    const status = await tauri.getRecordingStatus();
                    setState((prev) => ({
                        ...prev,
                        isRecording: status.is_recording,
                        duration: status.duration_seconds,
                        videoFrames: status.video_frames,
                        audioSamples: status.audio_samples,
                    }));
                } catch (err) {
                    console.error("Failed to get recording status:", err);
                }
            }, 1000);
        } else if (intervalRef.current) {
            clearInterval(intervalRef.current);
            intervalRef.current = null;
        }

        return () => {
            if (intervalRef.current) {
                clearInterval(intervalRef.current);
            }
        };
    }, [state.isRecording, state.isPaused]);

    const startRecording = useCallback(async () => {
        try {
            setError(null);
            const meetingId = await tauri.startRecording();

            // Also start video recording
            try {
                await tauri.startVideoRecording(meetingId);
            } catch (videoErr) {
                console.warn('Video recording failed to start:', videoErr);
                // Continue without video - audio is the priority
            }

            setState((prev) => ({
                ...prev,
                isRecording: true,
                isPaused: false,
                meetingId,
                duration: 0,
                videoFrames: 0,
                audioSamples: 0,
            }));
            return meetingId;
        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            setError(message);
            throw err;
        }
    }, []);

    const stopRecording = useCallback(async () => {
        try {
            setError(null);
            const currentMeetingId = state.meetingId; // Capture ID before reset

            // Stop video recording first
            try {
                await tauri.stopVideoRecording();
            } catch (videoErr) {
                console.warn('Video recording failed to stop:', videoErr);
            }

            await tauri.stopRecording();

            setState((prev) => ({
                ...prev,
                isRecording: false,
                isPaused: false,
            }));

            // Trigger Intelligence Pipeline
            if (currentMeetingId) {
                console.log(`[Recording] Triggering intelligence pipeline for ${currentMeetingId}`);
                // Note: Intelligence pipeline removed (was stub code)
                // IntelligenceService.runPostMeetingAnalysis(currentMeetingId)
                //     .catch((e: Error) => console.error("[Intel] Pipeline failed:", e));
            }

        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            setError(message);
            throw err;
        }
    }, [state.meetingId]);

    const pauseRecording = useCallback(async () => {
        // Toggle pause state - actual pause implementation would call backend
        setState((prev) => ({
            ...prev,
            isPaused: !prev.isPaused,
        }));
        // TODO: When backend pause_recording command exists, call it here
        // await tauri.pauseRecording();
    }, []);

    const toggleRecording = useCallback(async () => {
        if (state.isRecording) {
            await stopRecording();
        } else {
            await startRecording();
        }
    }, [state.isRecording, startRecording, stopRecording]);

    return {
        ...state,
        error,
        startRecording,
        stopRecording,
        pauseRecording,
        toggleRecording,
    };
}
