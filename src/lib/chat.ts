// Chat with your recordings: typed wrappers over the Tauri commands in
// src-tauri/src/chat.rs (docs/TOPICS_AND_CHAT.md). Pure logic lives in
// chatLogic.ts.

import { invoke } from "@tauri-apps/api/core";
import { withAiConsent } from "./ai";
import type { Scope, ScopeSummary } from "./chatLogic";

export interface ChatThread {
    id: string;
    title: string;
    scope: Scope;
    /** Set when answers were removed by a purge */
    flag: string | null;
    created_at: string;
    updated_at: string;
}

/** `[n]` in an answer → this passage. */
export interface Citation {
    n: number;
    meeting_id: string;
    title: string;
    /** ms from the recording's start; null for notes */
    timestamp_ms: number | null;
    excerpt: string;
    /** "transcript" | "notes" | "marker" */
    source: string;
}

export interface ChatMessage {
    id: string;
    thread_id: string;
    role: "user" | "assistant";
    /** Markdown (assistant) or the user's text; rendered as text, never HTML */
    content: string;
    citations: Citation[];
    scope_label: string | null;
    created_at: string;
}

export interface ChatTurn {
    thread: ChatThread;
    user: ChatMessage;
    assistant: ChatMessage;
}

export const chatApi = {
    /** Ask in a thread (null starts one). Pro and consent come from the backend's AI client. */
    ask: (threadId: string | null, scope: Scope, message: string) =>
        withAiConsent(() => invoke<ChatTurn>("chat_ask", { threadId, scope, message }), "chat"),
    threads: () => invoke<ChatThread[]>("list_chat_threads"),
    thread: (threadId: string) => invoke<{ thread: ChatThread; messages: ChatMessage[] }>("get_chat_thread", { threadId }),
    remove: (threadId: string) => invoke<void>("delete_chat_thread", { threadId }),
    summary: (scope: Scope) => invoke<ScopeSummary>("chat_scope_summary", { scope }),
};
