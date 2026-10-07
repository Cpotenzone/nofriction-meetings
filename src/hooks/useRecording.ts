// noFriction Meetings - Recording Hook
// Manages recording state and audio capture

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useState, useEffect, useCallback, useRef } from "react";
import * as tauri from "../lib/tauri";
import { getCapabilities } from "../lib/build";
import type { StartPlan } from "../lib/recordPlan";


export interface RecordingState {
    isRecording: boolean;
    isPaused: boolean;
    meetingId: string | null;
    duration: number;
    videoFrames: number;
    audioSamples: number;
    /** Why other participants' audio isn't being captured, if it isn't */
    audioWarning?: string | null;
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

    // The backend stopped the recording itself (time limit or meeting end,
    // when the UI didn't). Polling is off while paused, so sync here too.
    useEffect(() => {
        let disposed = false;
        let off: (() => void) | null = null;
        listen("recording-stopped-automatically", () => {
            setState((prev) => ({ ...prev, isRecording: false, isPaused: false }));
        }).then((fn) => (disposed ? fn() : (off = fn)));
        return () => {
            disposed = true;
            off?.();
        };
    }, []);

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
                        audioWarning: status.audio_warning ?? null,
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

    /** No plan: the remembered length (shortcut, tray, palette). The Record sheet passes one. */
    const startRecording = useCallback(async (plan?: StartPlan) => {
        try {
            setError(null);
            const meetingId = await tauri.startRecording(plan);

            // Link accessibility captures to this meeting
            try {
                await tauri.setAccessibilityMeetingId(meetingId);
            } catch (accErr) {
                console.warn('Failed to link accessibility captures:', accErr);
            }

            // Also start video recording (ffmpeg; not in the App Store build,
            // where screenshots from the capture engine feed the timeline)
            try {
                if ((await getCapabilities()).video_recording) {
                    await tauri.startVideoRecording(meetingId);
                }
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

            // Stop video recording first
            try {
                if ((await getCapabilities()).video_recording) {
                    await tauri.stopVideoRecording();
                }
            } catch (videoErr) {
                console.warn('Video recording failed to stop:', videoErr);
            }

            // Unlink accessibility captures from meeting
            try {
                await tauri.setAccessibilityMeetingId(null);
            } catch (accErr) {
                console.warn('Failed to unlink accessibility captures:', accErr);
            }

            await tauri.stopRecording();

            setState((prev) => ({
                ...prev,
                isRecording: false,
                isPaused: false,
            }));


        } catch (err) {
            const message = err instanceof Error ? err.message : String(err);
            setError(message);
            throw err;
        }
    }, [state.meetingId]);

    const pauseRecording = useCallback(async () => {
        await invoke("pause_recording");
        setState((prev) => ({
            ...prev,
            isPaused: true,
        }));
    }, []);

    const resumeRecording = useCallback(async () => {
        await invoke("resume_recording");
        setState((prev) => ({
            ...prev,
            isPaused: false,
        }));
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
        resumeRecording,
        toggleRecording,
    };
}
