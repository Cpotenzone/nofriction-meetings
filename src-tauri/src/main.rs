// noFriction Meetings
// Single-binary macOS meeting transcription app

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nofriction_meetings_lib::run()
}
