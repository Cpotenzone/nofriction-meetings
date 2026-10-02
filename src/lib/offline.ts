// noFriction Meetings — Offline/Mock Detection
//
// Detects if the app is running outside Tauri (e.g., in a browser during
// development) and provides fallback mock data. This allows UI development
// without the full Rust backend.
//
// Usage:
//   import { isOffline, mockMeetings } from '../lib/offline';
//   const data = isOffline() ? mockMeetings : await tauri.getMeetings(50);

import type { Meeting } from './tauri';

/**
 * Returns true if Tauri IPC is not available (running in browser).
 */
export function isOffline(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ === 'undefined';
}

/**
 * Wraps a Tauri invoke call with a fallback for offline mode.
 * If the Tauri backend is unavailable, returns the provided fallback value.
 */
export async function withFallback<T>(
  tauriCall: () => Promise<T>,
  fallback: T,
): Promise<T> {
  // Mock data only in a browser preview. In the app a failing call must
  // surface as an error, never as fake meetings.
  if (isOffline()) return fallback;
  return tauriCall();
}

// ─── Mock Data ──────────────────────────────────────────────────────

export const mockMeetings: Meeting[] = [
  {
    id: 'mock-1',
    title: 'Weekly Design Review',
    started_at: new Date(Date.now() - 86400000).toISOString(),
    ended_at: new Date(Date.now() - 86400000 + 3600000).toISOString(),
    duration_seconds: 3600,
    calendar_event_id: null,
  },
  {
    id: 'mock-2',
    title: 'Sprint Planning',
    started_at: new Date(Date.now() - 172800000).toISOString(),
    ended_at: new Date(Date.now() - 172800000 + 1800000).toISOString(),
    duration_seconds: 1800,
    calendar_event_id: null,
  },
  {
    id: 'mock-3',
    title: 'Engineering Standup',
    started_at: new Date(Date.now() - 259200000).toISOString(),
    ended_at: new Date(Date.now() - 259200000 + 900000).toISOString(),
    duration_seconds: 900,
    calendar_event_id: null,
  },
];

export const mockTranscripts = [
  {
    id: 1,
    meeting_id: 'mock-1',
    text: 'We need to finalize the color palette for the new dashboard design.',
    speaker: 'Alice',
    timestamp: new Date(Date.now() - 86400000 + 600000).toISOString(),
    is_final: true,
    confidence: 0.95,
  },
  {
    id: 2,
    meeting_id: 'mock-1',
    text: 'I think we should go with the darker theme, it matches our brand better.',
    speaker: 'Bob',
    timestamp: new Date(Date.now() - 86400000 + 660000).toISOString(),
    is_final: true,
    confidence: 0.92,
  },
  {
    id: 3,
    meeting_id: 'mock-1',
    text: 'Agreed. Let me update the Figma file by end of day.',
    speaker: 'Alice',
    timestamp: new Date(Date.now() - 86400000 + 720000).toISOString(),
    is_final: true,
    confidence: 0.88,
  },
];
