// People, calendar sync, and LinkedIn links.

use tauri::{AppHandle, Emitter, Manager, State};

use crate::calendar_client::{CalendarAccessStatus, CalendarClient};
use crate::people::{self, BackfillReport, MeetingPeople, Person};
use crate::AppState;

#[derive(serde::Serialize)]
pub struct CalendarSyncResult {
    /// "authorized" | "denied" | "restricted" | "not_determined"
    pub access: String,
    pub report: Option<BackfillReport>,
}

fn access_label(s: CalendarAccessStatus) -> &'static str {
    match s {
        CalendarAccessStatus::Authorized => "authorized",
        CalendarAccessStatus::Denied => "denied",
        CalendarAccessStatus::Restricted => "restricted",
        CalendarAccessStatus::NotDetermined => "not_determined",
        CalendarAccessStatus::Unknown => "unknown",
    }
}

/// Connect the calendar (asking for access the first time) and link every
/// meeting to its calendar event: title, time, location, notes, people.
#[tauri::command(rename_all = "camelCase")]
pub async fn sync_calendar(
    app: AppHandle,
    relink_all: Option<bool>,
    state: State<'_, AppState>,
) -> Result<CalendarSyncResult, String> {
    let mut status = CalendarClient::check_access();
    if status == CalendarAccessStatus::NotDetermined {
        let _ = CalendarClient::request_access().await;
        status = CalendarClient::check_access();
    }
    if status != CalendarAccessStatus::Authorized {
        return Ok(CalendarSyncResult {
            access: access_label(status).into(),
            report: None,
        });
    }

    let report = people::backfill(
        &state.database.get_pool(),
        &CalendarClient::new(),
        relink_all.unwrap_or(false),
    )
    .await?;
    let _ = app.emit("people_updated", &report);
    Ok(CalendarSyncResult {
        access: "authorized".into(),
        report: Some(report),
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_calendar_access_status() -> Result<String, String> {
    Ok(access_label(CalendarClient::check_access()).into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_meeting_people(
    meeting_id: String,
    state: State<'_, AppState>,
) -> Result<MeetingPeople, String> {
    people::meeting_people(&state.database.get_pool(), &meeting_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_people(state: State<'_, AppState>) -> Result<Vec<Person>, String> {
    people::list_people(&state.database.get_pool())
        .await
        .map_err(|e| e.to_string())
}

/// Save (or clear, with null/empty) a person's LinkedIn profile URL.
/// Returns the normalized URL.
#[tauri::command(rename_all = "camelCase")]
pub async fn set_person_linkedin(
    app: AppHandle,
    person_id: String,
    url: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let saved = people::set_linkedin(&state.database.get_pool(), &person_id, url.as_deref()).await?;
    let _ = app.emit("people_updated", ());
    Ok(saved)
}

/// On launch, quietly link any meetings recorded since the last run —
/// only if calendar access was already granted (never prompts).
pub fn spawn_startup_sync(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if CalendarClient::check_access() != CalendarAccessStatus::Authorized {
            return;
        }
        let Some(state) = app.try_state::<AppState>() else { return };
        let pool = state.database.get_pool();
        match people::backfill(&pool, &CalendarClient::new(), false).await {
            Ok(r) if r.meetings_linked > 0 => {
                let _ = app.emit("people_updated", &r);
            }
            Ok(_) => {}
            Err(e) => log::warn!("Calendar startup sync failed: {}", e),
        }
    });
}
