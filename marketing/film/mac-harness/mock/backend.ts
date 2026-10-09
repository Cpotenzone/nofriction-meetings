// A mocked Tauri backend for the film harness: answers every command the
// Mac app's screens call (src/lib/tauri.ts and friends) from the fictional
// demo data in mock-data.ts, keeps a small event bus for listen/emit, and
// simulates a live recording (transcript lines streaming in, screen
// captures, live insights) when recording starts.
//
// Unknown commands are logged as "[mock] unhandled" and answered with null.

import { mockConvertFileSrc, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { DEMO_MEETINGS, LIVE_SCRIPTS, RECENT_NOTEBOOKS, type DemoMeeting, type Kind } from "./mock-data";

type Args = Record<string, any> | undefined;

// ── Harness flags (URL parameters, see harness.ts) ──────────────────────
//   ?setup=1  first run: nothing allowed yet, the speech model downloading
//   ?pro=0    not subscribed: AI calls fail with PRO_REQUIRED (the paywall)
//   ?ai=none  no AI set up yet
const flags = new URLSearchParams(location.search);
const FIRST_RUN = flags.get("setup") === "1";
let isPro = flags.get("pro") !== "0";
const AI_NONE = flags.get("ai") === "none";
// ?ai=openai: the ChatGPT card saved with a key, not yet allowed (the
// consent dialog names api.openai.com on the first AI action)
const AI_OPENAI = flags.get("ai") === "openai";
let openaiConsent = false;

const PRODUCTS = [
    { id: "com.nofriction.meetings.pro.monthly", displayName: "Pro Monthly", description: "AI notes, summaries and reviews, monthly", displayPrice: "$0.99", period: "1 month", periodUnit: "month", periodValue: 1, introOffer: "1 week free" },
    { id: "com.nofriction.meetings.pro.yearly", displayName: "Pro Yearly", description: "AI notes, summaries and reviews, yearly", displayPrice: "$5.99", period: "1 year", periodUnit: "year", periodValue: 1, introOffer: "1 week free" },
];
const entitlement = () =>
    isPro
        ? { isPro: true, productId: "com.nofriction.meetings.pro.yearly", expiration: new Date(Date.now() + 200 * 86_400_000).toISOString(), willRenew: true, loaded: true }
        : { isPro: false, productId: null, expiration: null, willRenew: null, loaded: true };

// ── Clock helpers ───────────────────────────────────────────────────────

const ms = (clock: string): number => {
    const parts = clock.split(":").map(Number);
    return parts.length === 3 ? ((parts[0] * 60 + parts[1]) * 60 + parts[2]) * 1000 : (parts[0] * 60 + parts[1]) * 1000;
};

function startOf(m: DemoMeeting): Date {
    const d = new Date();
    d.setDate(d.getDate() - m.daysAgo);
    const [h, min] = m.startTime.split(":").map(Number);
    d.setHours(h, min, 0, 0);
    return d;
}

// ── Stored state (mutable: marks and recordings change while filming) ──

interface MeetingRow {
    id: string;
    title: string;
    started_at: string;
    ended_at: string | null;
    duration_seconds: number | null;
    calendar_event_id: null;
    planned_minutes: number | null;
    class_name: string | null;
    recording_kind: Kind;
}

interface Marker {
    id: string;
    meeting_id: string;
    ts: string;
    kind: "important" | "question" | "test";
    note: string | null;
    created_at: string;
    offset_ms: number;
}

const meetings: MeetingRow[] = [];
const markers: Marker[] = [];
const demoById = new Map<string, DemoMeeting>();

for (const m of DEMO_MEETINGS) {
    demoById.set(m.id, m);
    const start = startOf(m);
    const dur = Math.round(m.durationMin * 60);
    meetings.push({
        id: m.id,
        title: m.title,
        started_at: start.toISOString(),
        ended_at: new Date(start.getTime() + dur * 1000).toISOString(),
        duration_seconds: dur,
        calendar_event_id: null,
        planned_minutes: m.plannedMinutes,
        class_name: m.notebook,
        recording_kind: m.kind,
    });
    (m.markers ?? []).forEach((k, i) => {
        const off = ms(k.at);
        markers.push({
            id: `${m.id}-mk${i + 1}`,
            meeting_id: m.id,
            ts: new Date(start.getTime() + off).toISOString(),
            kind: k.kind,
            note: k.note,
            created_at: new Date(start.getTime() + off).toISOString(),
            offset_ms: off,
        });
    });
}

const meetingById = (id: string) => meetings.find((m) => m.id === id) ?? null;

// ── Frames (getFrameThumbnail returns base64 JPEG) ──────────────────────

const base64Cache = new Map<string, Promise<string>>();
function imageBase64(url: string): Promise<string> {
    let p = base64Cache.get(url);
    if (!p) {
        p = fetch(url)
            .then((r) => {
                if (!r.ok) throw new Error(`${url}: ${r.status}`);
                return r.blob();
            })
            .then(
                (b) =>
                    new Promise<string>((resolve, reject) => {
                        const fr = new FileReader();
                        fr.onload = () => resolve(String(fr.result).split(",")[1] ?? "");
                        fr.onerror = () => reject(fr.error);
                        fr.readAsDataURL(b);
                    }),
            );
        base64Cache.set(url, p);
    }
    return p;
}

/** Frame ids are "<meetingId>::<image>" so the image is known from the id. */
const frameId = (meetingId: string, image: string, n: number) => `${meetingId}::${image}::${n}`;
const imageOfFrame = (id: string) => id.split("::")[1];

// ── Live recording simulation ───────────────────────────────────────────

interface LiveLine {
    id: string;
    speaker: string;
    text: string;
    at: number; // epoch ms
}

interface Live {
    id: string;
    kind: Kind;
    startedAt: number;
    /** When the simulation really started (startedAt minus any backdate) */
    realStart: number;
    deadline: number | null;
    plannedMinutes: number | null;
    lines: LiveLine[];
    frames: { id: string; image: string; at: number }[];
    timers: number[];
    stopped: boolean;
    warned?: boolean;
}

let live: Live | null = null;
let liveCount = 0;

/** Pacing, adjustable from the capture script (window.__harness.pace) */
export const pace = {
    /** First line this long after the start (ms) */
    firstLineMs: 1400,
    /** Time a line takes to "be spoken": interim steps then final (ms) */
    lineMs: 2000,
    /** Pause before the next line (ms) */
    gapMs: 450,
    /** A new screen capture every N ms */
    frameEveryMs: 5200,
};

const timedStatus = () =>
    live && !live.stopped
        ? {
              meetingId: live.id,
              startedAt: new Date(live.startedAt).toISOString(),
              plannedMinutes: live.plannedMinutes,
              deadline: live.deadline ? new Date(live.deadline).toISOString() : null,
              remainingSeconds: live.deadline ? Math.max(0, Math.round((live.deadline - Date.now()) / 1000)) : null,
              warned: !!live.warned,
          }
        : null;

function startLive(plan: { recordingKind?: Kind; duration?: string; notebook?: string | null } | null, opts: { prefill?: number; backdateSec?: number } = {}) {
    stopLive();
    const kind: Kind = plan?.recordingKind ?? "meeting";
    const script = LIVE_SCRIPTS[kind];
    const backdate = (opts.backdateSec ?? 0) * 1000;
    const startedAt = Date.now() - backdate;
    const minutes = plan?.duration && plan.duration !== "none" ? Number(plan.duration) : null;
    const id = `live-${++liveCount}`;
    live = {
        id,
        kind,
        startedAt,
        realStart: Date.now(),
        deadline: minutes ? startedAt + minutes * 60_000 : null,
        plannedMinutes: minutes,
        lines: [],
        frames: [],
        timers: [],
        stopped: false,
    };
    const notebook = plan?.notebook ?? (kind === "class" ? "BIO 101" : kind === "meeting" ? "Acme project" : null);
    meetings.unshift({
        id,
        title: script.title,
        started_at: new Date(startedAt).toISOString(),
        ended_at: null,
        duration_seconds: null,
        calendar_event_id: null,
        planned_minutes: minutes,
        class_name: notebook,
        recording_kind: kind,
    });

    const L = live;
    const later = (delay: number, fn: () => void) => L.timers.push(window.setTimeout(() => !L.stopped && fn(), delay));

    // Lines already spoken (a recording that has been running for a while)
    const prefill = Math.min(opts.prefill ?? 0, script.lines.length);
    const spacing = prefill > 0 ? Math.max(2500, Math.floor(backdate / (prefill + 1))) : 0;
    later(350, () => {
        for (let i = 0; i < prefill; i++) {
            const at = startedAt + spacing * (i + 1) - 1500;
            sayFinal(L, script.lines[i], at, `u${i}`);
        }
        // Screens captured so far
        const shots = Math.min(script.frames.length, prefill > 0 ? Math.max(1, Math.round(prefill / 2)) : 0);
        for (let i = 0; i < shots; i++) captureFrame(L, script.frames[i], startedAt + (i + 1) * spacing);
    });

    // The rest streams in at a speaking pace
    let t = (prefill > 0 ? 900 : pace.firstLineMs);
    for (let i = prefill; i < script.lines.length; i++) {
        const line = script.lines[i];
        const words = line.text.split(" ");
        const steps = [0.3, 0.6, 0.85].map((f) => Math.max(1, Math.ceil(words.length * f)));
        const stepMs = pace.lineMs / (steps.length + 1);
        steps.forEach((n, k) => later(t + k * stepMs, () => sayInterim(L, line, words.slice(0, n).join(" "), `u${i}`)));
        later(t + steps.length * stepMs, () => sayFinal(L, line, Date.now() - pace.lineMs * 0.75, `u${i}`));
        t += pace.lineMs + pace.gapMs;
    }

    // New screens while recording
    const firstShot = prefill > 0 ? Math.min(script.frames.length, Math.max(1, Math.round(prefill / 2))) : 0;
    let ft = prefill > 0 ? 2600 : 900;
    for (let i = firstShot; i < script.frames.length; i++) {
        const image = script.frames[i];
        later(ft, () => captureFrame(L, image, Date.now()));
        ft += pace.frameEveryMs;
    }
    return id;
}

function sayInterim(L: Live, line: { speaker: string }, text: string, utterance: string) {
    emitEvent("live_transcript", {
        text,
        is_final: false,
        confidence: 0.8,
        start: Date.now() / 1000,
        duration: 1,
        speaker: line.speaker,
        utterance_id: `${L.id}-${utterance}`,
    });
}

function sayFinal(L: Live, line: { speaker: string; text: string }, at: number, utterance: string) {
    L.lines.push({ id: String(9000 + L.lines.length), speaker: line.speaker, text: line.text, at });
    emitEvent("live_transcript", {
        text: line.text,
        is_final: true,
        confidence: 0.94,
        start: at / 1000,
        duration: 2,
        speaker: line.speaker,
        utterance_id: `${L.id}-${utterance}`,
    });
}

function captureFrame(L: Live, image: string, at: number) {
    const id = frameId(L.id, image, L.frames.length);
    L.frames.push({ id, image, at });
    emitEvent("frame_captured", {
        state_id: null,
        meeting_id: L.id,
        path: `/frames/${image}-thumb.jpg`,
        source: "display:1",
        label: "Main display",
        timestamp: new Date(at).toISOString(),
        manual: false,
    });
}

function stopLive() {
    if (!live) return;
    live.stopped = true;
    live.timers.forEach((t) => clearTimeout(t));
    const row = meetingById(live.id);
    if (row && !row.ended_at) {
        row.ended_at = new Date().toISOString();
        row.duration_seconds = Math.round((Date.now() - live.startedAt) / 1000);
    }
}

// ── Event bus (listen / unlisten / emit) ────────────────────────────────

const listeners = new Map<string, Set<number>>();

export function emitEvent(event: string, payload: unknown) {
    const internals = (window as any).__TAURI_INTERNALS__;
    for (const handler of [...(listeners.get(event) ?? [])]) {
        if (internals.callbacks?.has?.(handler)) internals.runCallback(handler, { event, id: handler, payload });
        else listeners.get(event)?.delete(handler);
    }
}

// ── Command handlers ────────────────────────────────────────────────────

const CAPABILITIES = {
    // The Mac App Store build: sandboxed, screenshots (no screen video), Pro gating
    flavor: "mas",
    sandboxed: true,
    video_recording: false,
    accessibility_capture: false,
    owner_infra: false,
    storekit: true,
    pro_gating: true,
    apple_intelligence: true,
    apple_intelligence_reason: "",
    version: "3.7.0",
    build: "41",
};

const AI_STATUS = {
    text: { provider: "apple", name: "Apple on-device", model: "Apple Intelligence", local: true, consent: true, state: "ready" },
    vision: null,
    text_ready: true,
    vision_ready: false,
    what_leaves: "Nothing leaves this Mac.",
};

// Settings → AI: Apple on-device in use, the custom endpoint empty (a fresh
// install). The preset cards are the backend's table (ai/providers.rs).
const AI_PROVIDERS = [
    { id: "apple", name: "Apple on-device", preset: null, protocol: "apple", base_url: null, key_url: "", key: "none", local: true, editable_url: false, configured: true, last4: null, consent: true, needs_consent: false, model: "Apple Intelligence", vision_model: null, models: [], active_text: true, active_vision: false },
    { id: "custom", name: "Custom (OpenAI-compatible)", preset: null, protocol: "openai", base_url: null, key_url: "", key: "optional", local: false, editable_url: true, configured: false, last4: null, consent: false, needs_consent: false, model: null, vision_model: null, models: [], active_text: false, active_vision: false },
];
const AI_PRESETS = [
    { id: "openai", name: "ChatGPT (OpenAI)", base_url: "https://api.openai.com/v1", default_model: "gpt-6-luna", model_hint: "gpt-6.1-sol, gpt-6-astra", key_url: "https://platform.openai.com/api-keys", note: "Your OpenAI API key; billed by OpenAI." },
    { id: "anthropic", name: "Anthropic (Claude)", base_url: "https://api.anthropic.com/v1", default_model: "claude-sonnet-5-5", model_hint: "claude-haiku-5-5, claude-opus-5-5", key_url: "https://platform.claude.com/settings/keys", note: "Your Claude API key; billed by Anthropic." },
    { id: "xai", name: "Grok (xAI)", base_url: "https://api.x.ai/v1", default_model: "grok-4.7", model_hint: "grok-4.3", key_url: "https://console.x.ai", note: "Your xAI API key; billed by xAI." },
    { id: "mistral", name: "Mistral", base_url: "https://api.mistral.ai/v1", default_model: "mistral-large-latest", model_hint: "mistral-small-latest", key_url: "https://console.mistral.ai/api-keys", note: "Your Mistral API key; billed by Mistral." },
];

function timelineFor(id: string) {
    const row = meetingById(id);
    const demo = demoById.get(id);
    if (live && live.id === id) {
        return {
            meeting_id: id,
            meeting_title: row?.title ?? "",
            started_at: new Date(live.startedAt).toISOString(),
            duration_seconds: Math.round((Date.now() - live.startedAt) / 1000),
            frames: live.frames.map((f, i) => ({ id: f.id, frame_number: i + 1, timestamp_ms: f.at - live!.startedAt, thumbnail_path: null })),
            transcripts: live.lines.map((l) => ({
                id: l.id,
                timestamp_ms: l.at - live!.startedAt,
                text: l.text,
                speaker: l.speaker,
                is_final: true,
                duration_seconds: 2,
                end_ms: l.at - live!.startedAt + 2000,
            })),
            redactions: [],
        };
    }
    const lines = demo?.lines ?? [];
    const frames = demo?.frames ?? [];
    return {
        meeting_id: id,
        meeting_title: row?.title ?? "",
        started_at: row?.started_at,
        duration_seconds: row?.duration_seconds ?? 0,
        frames: frames.map((f, i) => ({
            id: frameId(id, f.image, i),
            frame_number: i + 1,
            timestamp_ms: ms(f.at),
            thumbnail_path: null,
            end_ms: i + 1 < frames.length ? ms(frames[i + 1].at) : (row?.duration_seconds ?? 0) * 1000,
        })),
        transcripts: lines.map((l, i) => {
            const at = ms(l.at);
            const next = i + 1 < lines.length ? ms(lines[i + 1].at) : at + 6000;
            const dur = Math.min(next - at, Math.max(3000, l.text.split(" ").length * 380));
            return { id: String(100 + i), timestamp_ms: at, text: l.text, speaker: l.speaker, is_final: true, duration_seconds: dur / 1000, end_ms: at + dur };
        }),
        redactions: [],
    };
}

function notesFor(id: string) {
    const n = demoById.get(id)?.notes;
    if (!n) return null;
    const row = meetingById(id)!;
    return {
        id: `notes-${id}`,
        meeting_id: id,
        summary: n.summary,
        key_topics: JSON.stringify(n.key_topics),
        decisions: JSON.stringify(n.decisions),
        action_items: JSON.stringify(n.action_items),
        generated_at: row.ended_at ?? row.started_at,
        model_used: n.model_used,
        stale_after_edit: false,
    };
}

function linksFor(id: string) {
    const row = meetingById(id);
    const links = (demoById.get(id)?.links ?? []).map((l, i) => {
        const u = new URL(l.url);
        const key = `${u.host}${u.pathname}`.replace(/\/$/, "");
        const first = l.first ? ms(l.first) : null;
        return {
            key,
            url: l.url,
            host: u.host.replace(/^www\./, ""),
            path: u.pathname === "/" ? "" : u.pathname,
            title: l.title ?? null,
            note: l.note ?? null,
            sources: l.sources,
            said_count: l.said ?? 0,
            screen_count: l.screen ?? 0,
            first_ms: first,
            first_at: first !== null && row ? new Date(Date.parse(row.started_at) + first).toISOString() : null,
            first_source: l.firstSource ?? null,
            reference_id: l.sources.includes("added") ? `ref-${id}-${i}` : null,
            created_at: row?.ended_at ?? null,
        };
    });
    return { meeting_id: id, started_at: row?.started_at ?? new Date().toISOString(), links, hidden: [] };
}

function studyFor(id: string) {
    const row = meetingById(id);
    const demo = demoById.get(id);
    const materials: Record<string, unknown> = {};
    for (const [kind, data] of Object.entries(demo?.study ?? {})) {
        materials[kind] = { kind, data, created_at: row?.ended_at ?? new Date().toISOString(), stale: false };
    }
    return {
        meeting_id: id,
        title: row?.title ?? "",
        recording_kind: row?.recording_kind ?? "meeting",
        started_at: row?.started_at ?? new Date().toISOString(),
        duration_ms: (row?.duration_seconds ?? 0) * 1000,
        has_transcript: (demo?.lines?.length ?? 0) > 0,
        materials,
        markers: markers.filter((m) => m.meeting_id === id),
    };
}

const never = () => new Promise(() => {});

function handle(cmd: string, args: Args): unknown {
    const a = args ?? {};
    switch (cmd) {
        // Event bus
        case "plugin:event|listen": {
            const set = listeners.get(a.event) ?? new Set<number>();
            set.add(a.handler);
            listeners.set(a.event, set);
            return a.handler;
        }
        case "plugin:event|unlisten":
            listeners.get(a.event)?.delete(a.eventId);
            return null;
        case "plugin:event|emit":
        case "plugin:event|emit_to":
            emitEvent(a.event, a.payload);
            return null;

        // Startup, build, AI, subscription
        case "check_init_status":
            return "Ready";
        case "debug_log":
            return null;
        case "get_build_capabilities":
            return CAPABILITIES;
        case "store_entitlement":
        case "store_restore":
            return entitlement();
        case "store_products":
            return { products: PRODUCTS };
        case "store_purchase":
            isPro = true;
            emitEvent("subscription-changed", entitlement());
            return { status: "purchased", entitlement: entitlement() };
        case "store_manage_subscriptions":
            return null;
        case "ai_status":
            if (AI_OPENAI) {
                return {
                    text: { provider: "custom", name: "ChatGPT (OpenAI)", model: "gpt-6-luna", local: false, consent: openaiConsent, state: openaiConsent ? "ready" : "consent_required" },
                    vision: null,
                    text_ready: openaiConsent,
                    vision_ready: false,
                    what_leaves: "Transcripts and titles go to api.openai.com when you use an AI feature.",
                };
            }
            return AI_NONE ? { text: null, vision: null, text_ready: false, vision_ready: false, what_leaves: "Nothing leaves this Mac." } : AI_STATUS;
        case "ai_grant_consent":
            openaiConsent = true;
            return null;
        case "ai_list_providers":
            if (AI_OPENAI) {
                return AI_PROVIDERS.map((p) =>
                    p.id === "custom"
                        ? { ...p, name: "ChatGPT (OpenAI)", preset: "openai", base_url: "https://api.openai.com/v1", configured: true, last4: "a1b2", consent: openaiConsent, needs_consent: !openaiConsent, model: "gpt-6-luna", active_text: true }
                        : { ...p, active_text: false },
                );
            }
            return AI_NONE ? AI_PROVIDERS.map((p) => ({ ...p, active_text: false })) : AI_PROVIDERS;
        case "ai_list_presets":
            return AI_PRESETS;
        case "ai_test":
            return { ok: true, class: "connected", message: "Connected. The endpoint answered." };
        case "plugin:app|version":
            return CAPABILITIES.version;
        case "get_vault_status":
            return { configured: false, path: null, valid: false, topicCount: 0, totalFiles: 0 };
        case "get_setting":
            return null;
        case "get_ai_automation":
            return { autoReport: true };
        case "set_ai_automation":
            return { autoReport: !!a.autoReport };
        case "get_local_stt_status":
            return {
                ready: !FIRST_RUN,
                resolved_model: "large-v3-turbo-q5_0",
                preferred_model: "large-v3-turbo-q5_0",
                models: [
                    { name: "large-v3-turbo-q5_0", size_mb: 547, description: "Recommended", installed: !FIRST_RUN, active: !FIRST_RUN },
                    { name: "base.en", size_mb: 142, description: "Smaller", installed: false, active: false },
                ],
            };
        case "get_settings":
            return { deepgram_api_key: null, secret_status: {}, selected_microphone: null, selected_monitor: null, auto_start_recording: false, show_notifications: true };
        case "get_feature_flags":
            return {};
        case "get_meeting_end_status":
            return { monitoring: false, pending: null };
        case "get_auto_stop_settings":
            return { enabled: true, silenceMinutes: 10 };
        case "check_permissions":
            return FIRST_RUN
                ? { microphone: false, screen_recording: false, accessibility: false, calendar: false }
                : { microphone: true, screen_recording: true, accessibility: false, calendar: true };
        case "get_microphone_auth_status":
            return FIRST_RUN ? "not_determined" : "authorized";
        case "download_whisper_model":
            // First run: the recommended model is part-way down (a progress line)
            emitEvent("whisper_download_progress", { model: a.model, downloaded_bytes: 212 * 1_048_576, total_bytes: 547 * 1_048_576, done: false, error: null });
            return never();
        case "set_active_provider":
        case "set_local_whisper_model":
            return null;

        // Recordings
        case "get_meetings": {
            const nb = typeof a.notebook === "string" ? a.notebook.toLowerCase() : null;
            const list = meetings
                .filter((m) => !nb || (m.class_name ?? "").toLowerCase() === nb)
                .slice()
                .sort((x, y) => Date.parse(y.started_at) - Date.parse(x.started_at));
            return list.slice(0, a.limit ?? 50);
        }
        case "get_meeting":
            return meetingById(a.meetingId);
        case "list_recent_notebooks":
            return RECENT_NOTEBOOKS;
        case "get_record_prefs":
            return { defaultKind: "meeting", defaultDuration: "none", recentNotebooks: RECENT_NOTEBOOKS };
        case "set_meeting_notebook": {
            const m = meetingById(a.meetingId);
            if (m) m.class_name = a.notebook ?? null;
            return a.notebook ?? null;
        }
        case "set_meeting_recording_kind": {
            const m = meetingById(a.meetingId);
            if (m) m.recording_kind = a.kind;
            return a.kind;
        }
        case "update_meeting_title": {
            const m = meetingById(a.meetingId);
            if (m) m.title = a.title;
            return null;
        }
        case "delete_meeting":
            return null;
        case "match_recording_to_calendar":
            return null;
        case "get_transcripts": {
            const tl = timelineFor(a.meetingId);
            const start = Date.parse(tl.started_at ?? new Date().toISOString());
            return tl.transcripts.map((t) => ({
                id: Number(t.id),
                meeting_id: a.meetingId,
                text: t.text,
                speaker: t.speaker,
                timestamp: new Date(start + t.timestamp_ms).toISOString(),
                is_final: true,
                confidence: 0.95,
            }));
        }
        case "get_synced_timeline":
            return timelineFor(a.meetingId);
        case "get_frame_thumbnail": {
            const image = imageOfFrame(String(a.frameId));
            if (!image) return null;
            return imageBase64(`/frames/${image}${a.thumbnail ? "-thumb" : ""}.jpg`);
        }
        case "get_frame_count":
            return timelineFor(a.meetingId).frames.length;
        case "get_frames":
            return [];
        case "get_meeting_ai_status":
            return { has_notes: !!demoById.get(a.meetingId)?.notes, notes_stale: false, has_study: !!demoById.get(a.meetingId)?.study, study_stale: false };
        case "list_redactions":
        case "list_failed_redactions":
        case "list_video_blank_jobs":
            return [];

        // People / calendar: the strip above a recording stays hidden
        case "get_meeting_people":
            return never();
        case "get_calendar_access_status":
            return FIRST_RUN ? "not_determined" : "authorized";
        case "list_people":
            return [];
        case "get_meeting_attendees":
            return [];

        // Notes, study, links
        case "get_meeting_notes":
            return notesFor(a.meetingId);
        case "get_study_guide":
            return studyFor(a.meetingId);
        case "list_meeting_links":
            return linksFor(a.meetingId);
        case "get_browser_url_capture":
            return false;
        case "open_meeting_link":
            return null;

        // Markers
        case "list_markers":
            return markers.filter((m) => m.meeting_id === a.meetingId).sort((x, y) => x.offset_ms - y.offset_ms);
        case "mark_moment": {
            if (!live || live.stopped) throw "Not recording";
            const off = Date.now() - live.startedAt;
            const m: Marker = {
                id: `${live.id}-mk${markers.length + 1}`,
                meeting_id: live.id,
                ts: new Date().toISOString(),
                kind: a.kind ?? "important",
                note: a.note ?? null,
                created_at: new Date().toISOString(),
                offset_ms: off,
            };
            markers.push(m);
            return m;
        }
        case "add_marker": {
            const row = meetingById(a.meetingId);
            const start = row ? Date.parse(row.started_at) : Date.now();
            const m: Marker = {
                id: `${a.meetingId}-mk${markers.length + 1}`,
                meeting_id: a.meetingId,
                ts: new Date(start + a.offsetMs).toISOString(),
                kind: a.kind ?? "important",
                note: a.note ?? null,
                created_at: new Date().toISOString(),
                offset_ms: a.offsetMs,
            };
            markers.push(m);
            return m;
        }
        case "update_marker": {
            const m = markers.find((x) => x.id === a.id);
            if (!m) throw "Marker not found";
            if (a.kind) m.kind = a.kind;
            if (a.clearNote) m.note = null;
            else if (typeof a.note === "string") m.note = a.note;
            return { ...m };
        }
        case "delete_marker": {
            const i = markers.findIndex((x) => x.id === a.id);
            if (i >= 0) markers.splice(i, 1);
            return null;
        }

        // Recording
        case "start_recording":
            return startFromPlan(a.plan ?? null);
        case "stop_recording":
            stopLive();
            return null;
        case "pause_recording":
        case "resume_recording":
        case "set_accessibility_meeting_id":
        case "set_genie_mode":
            return null;
        case "get_recording_status":
            return {
                is_recording: !!live && !live.stopped,
                duration_seconds: live ? Math.round((Date.now() - live.startedAt) / 1000) : 0,
                video_frames: live?.frames.length ?? 0,
                audio_samples: live ? Math.round((Date.now() - live.startedAt) * 16) : 0,
                audio_warning: null,
            };
        case "get_timed_recording_status":
            return timedStatus();
        case "extend_timed_recording":
            if (live?.deadline) live.deadline += 15 * 60_000;
            return timedStatus();
        case "remove_timed_recording_limit":
            if (live) live.deadline = null;
            return timedStatus();
        case "get_live_insights": {
            if (!live || live.id !== a.meetingId) return [];
            const L = live;
            const elapsed = (Date.now() - L.realStart) / 1000;
            return LIVE_SCRIPTS[L.kind].insights
                .filter((i) => i.at <= elapsed)
                .map((i, n) => ({ type: i.type, id: `${L.id}-ins${n}`, text: i.text, assignee: i.assignee, timestamp_ms: L.realStart + i.at * 1000 }));
        }
        case "get_capture_targets":
            return [];
        case "list_capture_sources":
            return [{ target: { kind: "display", id: 1 }, title: "Main display", app_name: null, width: 3024, height: 1964, is_primary: true, thumbnail: null }];
        case "set_capture_targets":
            return null;
        case "snap_capture_target":
            return { path: "/frames/acme-1-thumb.jpg", label: "Main display" };
        case "get_capture_mode":
            return "Paused";

        // The one search, topics, devices (the Ive pass's Recordings and Settings)
        case "search_recordings":
            return searchRecordings(String(a.query ?? ""));
        case "list_topics":
            return topicIndex();
        case "get_meeting_topics":
            return topicsFor(String(a.meetingId)).map((t, i) => ({
                ...t,
                id: `${a.meetingId}-t${i}`,
                meeting_id: a.meetingId,
                confidence: null,
                source: "ai",
                created_at: new Date().toISOString(),
            }));
        case "get_audio_devices":
            return [{ id: "builtin", name: "MacBook Pro Microphone", is_default: true, is_input: true }];

        // AI (Pro in the Mac App Store build: ai::client::complete → PRO_REQUIRED)
        case "generate_meeting_report":
        case "draft_followup_email":
        case "generate_study_guide":
            if (!isPro) throw "PRO_REQUIRED: noFriction Pro unlocks the AI features.";
            if (cmd === "draft_followup_email") {
                return { subject: "Acme sync: decisions and next steps", body: "Hi all,\n\nThanks for today. We locked the plan names and agreed to test onboarding with five beta teams.\n\nPriya", to: ["dana@example.com", "marcus@example.com"] };
            }
            return null;

        // Chat (chat.rs)
        case "list_chat_threads":
            return chatThreads;
        case "get_chat_thread": {
            const t = chatThreads.find((x) => x.id === a.threadId);
            return { thread: t, messages: chatMessages.filter((m) => m.thread_id === a.threadId) };
        }
        case "delete_chat_thread":
            return null;
        case "chat_scope_summary":
            return scopeSummary(a.scope);
        case "chat_ask":
            if (!isPro) throw "PRO_REQUIRED: noFriction Pro unlocks the AI features.";
            if (AI_OPENAI && !openaiConsent) throw "CONSENT_REQUIRED:custom";
            return chatAnswer(a.threadId ?? null, a.scope, String(a.message ?? ""));

        // Plugins (dialogs, opener)
        case "plugin:dialog|ask":
        case "plugin:dialog|confirm":
            return false;
        case "plugin:dialog|message":
        case "plugin:opener|open_url":
            return null;
    }
    console.warn("[mock] unhandled", cmd, args);
    return null;
}

// ── Chat from the demo data ──────────────────────────────────────────

const chatThreads: any[] = [];
const chatMessages: any[] = [];

function scopeMeetings(scope: any) {
    if (!scope || scope.kind === "all") return meetings;
    if (scope.kind === "notebook") return meetings.filter((m) => (m.class_name ?? "").toLowerCase() === String(scope.value ?? "").toLowerCase());
    if (scope.kind === "meeting") return meetings.filter((m) => m.id === scope.value);
    if (scope.kind === "topic") return meetings.filter((m) => topicsFor(m.id).some((t) => t.key === scope.value));
    return meetings;
}

function scopeLabel(scope: any) {
    if (!scope || scope.kind === "all") return "All recordings";
    if (scope.kind === "notebook") return String(scope.value);
    if (scope.kind === "meeting") return meetingById(scope.value)?.title ?? "This recording";
    return topicIndex().topics.find((t) => t.key === scope.value)?.label ?? "This topic";
}

function scopeSummary(scope: any) {
    const ms_ = scopeMeetings(scope);
    return {
        label: scopeLabel(scope),
        count: ms_.length,
        recent_titles: ms_.slice(0, 3).map((m) => m.title),
        topics: [...new Set(ms_.flatMap((m) => topicsFor(m.id).map((t) => t.label)))].slice(0, 4),
        notebooks: [...new Set(ms_.map((m) => m.class_name).filter(Boolean))],
        kinds: [...new Set(ms_.map((m) => m.recording_kind))],
    };
}

function chatAnswer(threadId: string | null, scope: any, message: string) {
    const now = new Date().toISOString();
    let thread = chatThreads.find((t) => t.id === threadId);
    if (!thread) {
        thread = { id: `thread-${chatThreads.length + 1}`, title: message.slice(0, 60), scope: scope ?? { kind: "all" }, flag: null, created_at: now, updated_at: now };
        chatThreads.unshift(thread);
    }
    const acme = meetingById("m-acme-sync");
    const user = { id: `msg-${chatMessages.length + 1}`, thread_id: thread.id, role: "user", content: message, citations: [], scope_label: null, created_at: now };
    const assistant = {
        id: `msg-${chatMessages.length + 2}`,
        thread_id: thread.id,
        role: "assistant",
        content:
            "You decided to lock the plan names by Friday so billing can start [1], and to send team invites after the first project instead of during setup [2]. The new onboarding will be tested with five beta teams before it's built [3].",
        citations: [
            { n: 1, meeting_id: "m-acme-sync", title: acme?.title ?? "", timestamp_ms: ms("12:40"), excerpt: "Let's lock the plan names by Friday so billing can start.", source: "transcript" },
            { n: 2, meeting_id: "m-acme-sync", title: acme?.title ?? "", timestamp_ms: ms("10:26"), excerpt: "Invites come after the first project, not during setup.", source: "transcript" },
            { n: 3, meeting_id: "m-acme-sync", title: acme?.title ?? "", timestamp_ms: null, excerpt: "Test the new onboarding with five beta teams before building it", source: "notes" },
        ],
        scope_label: scopeLabel(scope),
        created_at: now,
    };
    chatMessages.push(user, assistant);
    return { thread, user, assistant };
}

// ── Search and topics from the demo data ─────────────────────────────

const topicKey = (label: string) => label.toLowerCase().replace(/[^a-z0-9]+/g, "-");

function topicsFor(meetingId: string): { key: string; label: string }[] {
    return (demoById.get(meetingId)?.notes?.key_topics ?? []).slice(0, 3).map((label) => ({ key: topicKey(label), label }));
}

function topicIndex() {
    const counts = new Map<string, { key: string; label: string; count: number }>();
    const by_meeting: Record<string, { key: string; label: string }[]> = {};
    for (const m of meetings) {
        const ts = topicsFor(m.id);
        by_meeting[m.id] = ts;
        for (const t of ts) {
            const c = counts.get(t.key) ?? { ...t, count: 0 };
            c.count += 1;
            counts.set(t.key, c);
        }
    }
    return { topics: [...counts.values()], by_meeting };
}

function searchRecordings(query: string) {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    const hits: { meeting_id: string; kind: string; label: string; ms: number | null }[] = [];
    for (const m of meetings) {
        if (m.title.toLowerCase().includes(q)) hits.push({ meeting_id: m.id, kind: "title", label: m.title, ms: null });
        if (m.class_name?.toLowerCase().includes(q)) hits.push({ meeting_id: m.id, kind: "notebook", label: m.class_name, ms: null });
        for (const t of topicsFor(m.id)) {
            if (t.label.toLowerCase().includes(q)) hits.push({ meeting_id: m.id, kind: "topic", label: t.label, ms: null });
        }
        for (const l of demoById.get(m.id)?.lines ?? []) {
            if (l.text.toLowerCase().includes(q)) hits.push({ meeting_id: m.id, kind: "said", label: l.text, ms: ms(l.at) });
        }
    }
    return hits;
}

/** Install the mocked backend. Call before the app is imported. */
export function installMocks() {
    mockWindows("main");
    mockConvertFileSrc("macos");
    mockIPC((cmd, args) => {
        try {
            return handle(cmd, args as Args);
        } catch (e) {
            return Promise.reject(e);
        }
    });
    // Captured screens are served by the harness: hand the path straight back
    (window as any).__TAURI_INTERNALS__.convertFileSrc = (p: string) => p;
}

let pendingStart: { kind: Kind; prefill?: number; backdateSec?: number } | null = null;

/** start_recording: the Record sheet's plan, or (a tray start from
 *  window.__harness.startLive) the kind and prefill the script asked for. */
function startFromPlan(plan: any) {
    if (pendingStart) {
        const p = pendingStart;
        pendingStart = null;
        return startLive({ ...(plan ?? {}), recordingKind: p.kind }, { prefill: p.prefill, backdateSec: p.backdateSec });
    }
    return startLive(plan);
}

/** Controls for the capture script (window.__harness). */
export const harnessApi = {
    pace,
    emit: emitEvent,
    /** Start a recording as if from the tray (no Record sheet). The app's
     *  own handler calls start_recording; `prefill` lines are already there. */
    startLive(kind: Kind = "meeting", opts: { prefill?: number; backdateSec?: number; duration?: string } = {}) {
        pendingStart = { kind, ...opts };
        emitEvent("tray:start_recording", { duration: opts.duration ?? "60" });
    },
    /** The time-limit warning: five minutes left, as timed_recording.rs sends it. */
    warnLive() {
        if (!live) return;
        live.deadline = Date.now() + 5 * 60_000 - 1000;
        live.warned = true;
        emitEvent("timed-recording-changed", timedStatus());
        emitEvent("timed-recording-warning", timedStatus());
    },
    get liveId() {
        return live?.id ?? null;
    },
};

