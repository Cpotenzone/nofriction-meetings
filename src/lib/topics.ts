// Topics: typed wrappers over the Tauri commands in src-tauri/src/topics.rs
// (docs/TOPICS_AND_CHAT.md). Pure logic lives in topicsLogic.ts.

import { invoke } from "@tauri-apps/api/core";
import type { MeetingTopic, TopicIndex } from "./topicsLogic";

/** Fired in the window after a recording's topics change, so open lists refresh. */
export const TOPICS_CHANGED_EVENT = "nf:topics-changed";
export function notifyTopicsChanged(meetingId: string): void {
    window.dispatchEvent(new CustomEvent(TOPICS_CHANGED_EVENT, { detail: { meetingId } }));
}

export const topicsApi = {
    /** Every topic with its recording count, and each recording's topics */
    index: () => invoke<TopicIndex>("list_topics"),
    get: (meetingId: string) => invoke<MeetingTopic[]>("get_meeting_topics", { meetingId }),
    /** The user's own list for a recording (rename, remove, add); [] clears it */
    set: (meetingId: string, labels: string[]) => invoke<MeetingTopic[]>("set_meeting_topics", { meetingId, labels }),
    /** Find (again) with the user's AI; user topics are kept */
    find: (meetingId: string) => invoke<MeetingTopic[]>("find_topics", { meetingId }),
};
