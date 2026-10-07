// noFriction Meetings - Links & References: backend commands
// (src-tauri/src/meeting_links.rs; docs/LINKS.md)

import { invoke } from "@tauri-apps/api/core";
import { isOpenableUrl, type MeetingLinks, type MeetingReference } from "./meetingLinks";

export const listMeetingLinks = (meetingId: string) => invoke<MeetingLinks>("list_meeting_links", { meetingId });

export const addMeetingReference = (meetingId: string, url: string, title: string | null, note: string | null) =>
    invoke<MeetingReference>("add_meeting_reference", { meetingId, url, title, note });

export const updateMeetingReference = (id: string, url: string, title: string | null, note: string | null) =>
    invoke<MeetingReference>("update_meeting_reference", { id, url, title, note });

export const deleteMeetingReference = (id: string) => invoke<void>("delete_meeting_reference", { id });

/** Hide a detected link (`hidden` false shows it again). Stores only a hash. */
export const hideMeetingLink = (meetingId: string, key: string, hidden = true) =>
    invoke<void>("hide_meeting_link", { meetingId, key, hidden });

/** Opens in the default browser. http/https only (checked here and again in Rust). */
export async function openMeetingLink(url: string): Promise<void> {
    if (!isOpenableUrl(url)) throw new Error("Only web links (http and https) can be opened.");
    await invoke<void>("open_meeting_link", { url });
}

/** DMG build: record the frontmost browser's address while recording. */
export const getBrowserUrlCapture = () => invoke<boolean>("get_browser_url_capture");
export const setBrowserUrlCapture = (enabled: boolean) => invoke<void>("set_browser_url_capture", { enabled });
