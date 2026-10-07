// People & meeting details from the calendar.
//
// Every recording is matched to the calendar event it happened during. The
// event's title, time, location, join link, notes and organizer are kept in
// `meeting_details`; everyone on the invite becomes a `people` row (one per
// email, shared across meetings) joined through `meeting_attendees`.
//
// LinkedIn: there is no public API to look people up, so profiles are
// linked by the user — the UI offers a prefilled LinkedIn search, and the
// chosen profile URL is saved on the person and follows them to every
// meeting they attend.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sqlx::{Pool, Row, Sqlite};

use crate::calendar_client::{CalendarEventNative, CalendarParticipant};

/// Tables owned by this module. Idempotent; runs with the other migrations.
pub async fn ensure_schema(conn: &mut sqlx::SqliteConnection) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS people (
            id TEXT PRIMARY KEY,              -- lowercase email
            email TEXT NOT NULL UNIQUE,
            name TEXT,
            company TEXT,
            company_domain TEXT,
            linkedin_url TEXT,
            notes TEXT,
            is_self INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS meeting_details (
            meeting_id TEXT PRIMARY KEY,
            calendar_event_id TEXT,
            event_title TEXT,
            scheduled_start TEXT,
            scheduled_end TEXT,
            location TEXT,
            meeting_url TEXT,
            notes TEXT,
            calendar_name TEXT,
            organizer_email TEXT,
            linked_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&mut *conn)
    .await?;

    // meeting_attendees predates this module and never had a uniqueness
    // rule, so "INSERT OR IGNORE" duplicated rows. Dedupe, then enforce.
    let _ = sqlx::query(
        "DELETE FROM meeting_attendees WHERE id NOT IN \
         (SELECT MIN(id) FROM meeting_attendees GROUP BY meeting_id, lower(email))",
    )
    .execute(&mut *conn)
    .await;
    let _ = sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_attendees_meeting_email ON meeting_attendees(meeting_id, email)",
    )
    .execute(&mut *conn)
    .await;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct Person {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub company: Option<String>,
    pub company_domain: Option<String>,
    pub linkedin_url: Option<String>,
    pub notes: Option<String>,
    pub is_self: bool,
    /// Role in a specific meeting ("organizer" / "attendee"), when listed
    /// for one
    pub role: Option<String>,
    pub meeting_count: i64,
    pub last_met: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingDetails {
    pub meeting_id: String,
    pub calendar_event_id: Option<String>,
    pub event_title: Option<String>,
    pub scheduled_start: Option<String>,
    pub scheduled_end: Option<String>,
    pub location: Option<String>,
    pub meeting_url: Option<String>,
    pub notes: Option<String>,
    pub calendar_name: Option<String>,
    pub organizer_email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingPeople {
    pub details: Option<MeetingDetails>,
    pub people: Vec<Person>,
}

fn display_name(p: &CalendarParticipant) -> String {
    p.name
        .clone()
        .unwrap_or_else(|| crate::attendee_intel::extract_name_from_email(&p.email))
}

/// Default titles look like "Class — Oct 7" / "BIO 101 — Oct 7" (or
/// "Meeting 2026-09-24 09:01" from earlier builds); anything else was set by
/// the user (or a previous calendar link) and is left alone.
fn is_default_title(title: &str) -> bool {
    crate::recording_kind::is_untitled_title(title)
}

/// Attach a calendar event to a meeting: details, people, and (if the
/// meeting still has its auto-generated title) the event title.
pub async fn link_meeting_to_event(
    pool: &Pool<Sqlite>,
    meeting_id: &str,
    event: &CalendarEventNative,
) -> Result<usize, sqlx::Error> {
    let organizer = event.participants.iter().find(|p| p.is_organizer);

    sqlx::query(
        r#"
        INSERT INTO meeting_details
            (meeting_id, calendar_event_id, event_title, scheduled_start, scheduled_end,
             location, meeting_url, notes, calendar_name, organizer_email)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(meeting_id) DO UPDATE SET
            calendar_event_id = excluded.calendar_event_id,
            event_title = excluded.event_title,
            scheduled_start = excluded.scheduled_start,
            scheduled_end = excluded.scheduled_end,
            location = excluded.location,
            meeting_url = excluded.meeting_url,
            notes = excluded.notes,
            calendar_name = excluded.calendar_name,
            organizer_email = excluded.organizer_email,
            linked_at = datetime('now')
        "#,
    )
    .bind(meeting_id)
    .bind(&event.event_id)
    .bind(&event.title)
    .bind(event.start_time.to_rfc3339())
    .bind(event.end_time.to_rfc3339())
    .bind(&event.location)
    .bind(&event.meeting_url)
    .bind(&event.notes)
    .bind(&event.calendar_name)
    .bind(organizer.map(|o| o.email.clone()))
    .execute(pool)
    .await?;

    let _ = sqlx::query("UPDATE meetings SET calendar_event_id = ? WHERE id = ?")
        .bind(&event.event_id)
        .bind(meeting_id)
        .execute(pool)
        .await;

    if !event.title.trim().is_empty() {
        let current: Option<String> = sqlx::query("SELECT title FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(pool)
            .await?
            .map(|r| r.get("title"));
        if current.as_deref().map(is_default_title).unwrap_or(false) {
            sqlx::query("UPDATE meetings SET title = ? WHERE id = ?")
                .bind(event.title.trim())
                .bind(meeting_id)
                .execute(pool)
                .await?;
        }
    }

    let mut linked = 0;
    for p in &event.participants {
        let name = display_name(p);
        let (domain, company) = crate::attendee_intel::extract_company_from_email(&p.email);
        let company = (company != "Personal").then_some(company);

        // A real name from the invite beats one guessed from the email
        sqlx::query(
            r#"
            INSERT INTO people (id, email, name, company, company_domain, is_self)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = CASE WHEN ? THEN excluded.name ELSE COALESCE(people.name, excluded.name) END,
                company = COALESCE(people.company, excluded.company),
                company_domain = COALESCE(people.company_domain, excluded.company_domain),
                is_self = MAX(people.is_self, excluded.is_self),
                updated_at = datetime('now')
            "#,
        )
        .bind(&p.email)
        .bind(&p.email)
        .bind(&name)
        .bind(&company)
        .bind(&domain)
        .bind(p.is_self as i32)
        .bind(p.name.is_some())
        .execute(pool)
        .await?;

        let role = if p.is_organizer { "organizer" } else { "attendee" };
        sqlx::query(
            r#"
            INSERT INTO meeting_attendees (meeting_id, name, email, company, role)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(meeting_id, email) DO UPDATE SET
                name = excluded.name, company = excluded.company, role = excluded.role
            "#,
        )
        .bind(meeting_id)
        .bind(&name)
        .bind(&p.email)
        .bind(&company)
        .bind(role)
        .execute(pool)
        .await?;
        linked += 1;
    }

    Ok(linked)
}

/// Pick the calendar event a recording belongs to: the timed (not all-day)
/// event with the most overlap, requiring a meaningful overlap so a
/// recording isn't pinned to an adjacent meeting it merely touched.
pub fn best_event_for<'a>(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    events: &'a [CalendarEventNative],
) -> Option<&'a CalendarEventNative> {
    let rec_len = (end - start).num_seconds().max(60);
    events
        .iter()
        .filter(|e| !e.is_all_day && e.end_time > e.start_time)
        .filter_map(|e| {
            // Recordings often start a few minutes early
            let ev_start = e.start_time - Duration::minutes(10);
            let overlap = (end.min(e.end_time) - start.max(ev_start)).num_seconds();
            let ev_len = (e.end_time - e.start_time).num_seconds().max(60);
            let meaningful = overlap >= 300
                || overlap as f64 >= 0.3 * rec_len as f64
                || overlap as f64 >= 0.5 * ev_len as f64;
            (overlap > 0 && meaningful).then_some((overlap, e))
        })
        .max_by_key(|(overlap, _)| *overlap)
        .map(|(_, e)| e)
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct BackfillReport {
    pub meetings_checked: usize,
    pub meetings_linked: usize,
    pub people_linked: usize,
}

/// Match every not-yet-linked meeting to the calendar.
pub async fn backfill(
    pool: &Pool<Sqlite>,
    client: &crate::calendar_client::CalendarClient,
    relink_all: bool,
) -> Result<BackfillReport, String> {
    let rows = sqlx::query(
        r#"
        SELECT m.id, m.started_at, m.ended_at, m.duration_seconds
        FROM meetings m
        LEFT JOIN meeting_details d ON d.meeting_id = m.id
        WHERE ? OR d.meeting_id IS NULL
        "#,
    )
    .bind(relink_all)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let meetings: Vec<(String, DateTime<Utc>, DateTime<Utc>)> = rows
        .iter()
        .filter_map(|r| {
            let id: String = r.get("id");
            let start = DateTime::parse_from_rfc3339(&r.get::<String, _>("started_at"))
                .ok()?
                .with_timezone(&Utc);
            let end = r
                .try_get::<Option<String>, _>("ended_at")
                .ok()
                .flatten()
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc))
                .or_else(|| {
                    r.try_get::<Option<i64>, _>("duration_seconds")
                        .ok()
                        .flatten()
                        .map(|d| start + Duration::seconds(d))
                })
                .unwrap_or(start + Duration::minutes(30));
            Some((id, start, end.max(start + Duration::minutes(1))))
        })
        .collect();

    let mut report = BackfillReport {
        meetings_checked: meetings.len(),
        ..Default::default()
    };
    if meetings.is_empty() {
        return Ok(report);
    }

    // One EventKit query per ~6-month window covering all meetings
    let lo = meetings.iter().map(|m| m.1).min().unwrap() - Duration::days(1);
    let hi = meetings.iter().map(|m| m.2).max().unwrap() + Duration::days(1);
    let mut events = Vec::new();
    let mut cursor = lo;
    while cursor < hi {
        let next = (cursor + Duration::days(180)).min(hi);
        events.extend(client.fetch_events_between(cursor, next)?);
        cursor = next;
    }
    events.sort_by(|a, b| a.event_id.cmp(&b.event_id).then(a.start_time.cmp(&b.start_time)));
    events.dedup_by(|a, b| a.event_id == b.event_id && a.start_time == b.start_time);

    for (id, start, end) in &meetings {
        if let Some(event) = best_event_for(*start, *end, &events) {
            let n = link_meeting_to_event(pool, id, event)
                .await
                .map_err(|e| e.to_string())?;
            report.meetings_linked += 1;
            report.people_linked += n;
        }
    }
    log::info!(
        "📅 Calendar backfill: {}/{} meetings linked, {} attendee links",
        report.meetings_linked,
        report.meetings_checked,
        report.people_linked
    );
    Ok(report)
}

fn row_to_person(r: &sqlx::sqlite::SqliteRow) -> Person {
    Person {
        id: r.get("id"),
        email: r.get("email"),
        name: r.try_get("name").ok().flatten(),
        company: r.try_get("company").ok().flatten(),
        company_domain: r.try_get("company_domain").ok().flatten(),
        linkedin_url: r.try_get("linkedin_url").ok().flatten(),
        notes: r.try_get("notes").ok().flatten(),
        is_self: r.try_get::<i64, _>("is_self").unwrap_or(0) != 0,
        role: r.try_get("role").ok().flatten(),
        meeting_count: r.try_get("meeting_count").unwrap_or(0),
        last_met: r.try_get("last_met").ok().flatten(),
    }
}

pub async fn meeting_people(pool: &Pool<Sqlite>, meeting_id: &str) -> Result<MeetingPeople, sqlx::Error> {
    let details = sqlx::query("SELECT * FROM meeting_details WHERE meeting_id = ?")
        .bind(meeting_id)
        .fetch_optional(pool)
        .await?
        .map(|r| MeetingDetails {
            meeting_id: r.get("meeting_id"),
            calendar_event_id: r.try_get("calendar_event_id").ok().flatten(),
            event_title: r.try_get("event_title").ok().flatten(),
            scheduled_start: r.try_get("scheduled_start").ok().flatten(),
            scheduled_end: r.try_get("scheduled_end").ok().flatten(),
            location: r.try_get("location").ok().flatten(),
            meeting_url: r.try_get("meeting_url").ok().flatten(),
            notes: r.try_get("notes").ok().flatten(),
            calendar_name: r.try_get("calendar_name").ok().flatten(),
            organizer_email: r.try_get("organizer_email").ok().flatten(),
        });

    let people = sqlx::query(
        r#"
        SELECT p.*, a.role,
               (SELECT COUNT(*) FROM meeting_attendees x WHERE lower(x.email) = p.id) AS meeting_count,
               (SELECT MAX(m.started_at) FROM meeting_attendees x JOIN meetings m ON m.id = x.meeting_id
                 WHERE lower(x.email) = p.id) AS last_met
        FROM meeting_attendees a
        JOIN people p ON p.id = lower(a.email)
        WHERE a.meeting_id = ?
        ORDER BY p.is_self ASC, (a.role = 'organizer') DESC, p.name COLLATE NOCASE
        "#,
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(row_to_person)
    .collect();

    Ok(MeetingPeople { details, people })
}

pub async fn list_people(pool: &Pool<Sqlite>) -> Result<Vec<Person>, sqlx::Error> {
    Ok(sqlx::query(
        r#"
        SELECT p.*, NULL AS role,
               COUNT(a.id) AS meeting_count,
               MAX(m.started_at) AS last_met
        FROM people p
        LEFT JOIN meeting_attendees a ON lower(a.email) = p.id
        LEFT JOIN meetings m ON m.id = a.meeting_id
        WHERE p.is_self = 0
        GROUP BY p.id
        ORDER BY last_met DESC
        "#,
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(row_to_person)
    .collect())
}

/// Accepts "linkedin.com/in/…" in any common form; returns the canonical
/// https URL, or an error for anything that isn't a LinkedIn profile.
pub fn normalize_linkedin_url(input: &str) -> Result<String, String> {
    let t = input.trim().trim_end_matches('/');
    let without_scheme = t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
        .unwrap_or(t);
    let without_www = without_scheme
        .strip_prefix("www.")
        .or_else(|| without_scheme.strip_prefix("m."))
        .unwrap_or(without_scheme);
    let path = without_www
        .strip_prefix("linkedin.com/")
        .ok_or("That isn't a LinkedIn link — paste a profile URL like linkedin.com/in/jane-doe")?;
    let path = path.split(['?', '#']).next().unwrap_or("");
    let mut parts = path.split('/').filter(|s| !s.is_empty());
    match (parts.next(), parts.next()) {
        (Some("in"), Some(handle)) if !handle.is_empty() => {
            Ok(format!("https://www.linkedin.com/in/{}", handle))
        }
        _ => Err("Paste a LinkedIn profile URL (linkedin.com/in/…)".into()),
    }
}

pub async fn set_linkedin(pool: &Pool<Sqlite>, person_id: &str, url: Option<&str>) -> Result<Option<String>, String> {
    let normalized = match url.map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => Some(normalize_linkedin_url(u)?),
        None => None,
    };
    let res = sqlx::query("UPDATE people SET linkedin_url = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&normalized)
        .bind(person_id.to_lowercase())
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    if res.rows_affected() == 0 {
        return Err("Person not found".into());
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, start_min: i64, len_min: i64) -> CalendarEventNative {
        let base = DateTime::parse_from_rfc3339("2026-09-24T16:00:00Z").unwrap().with_timezone(&Utc);
        CalendarEventNative {
            event_id: id.into(),
            title: id.into(),
            start_time: base + Duration::minutes(start_min),
            end_time: base + Duration::minutes(start_min + len_min),
            location: None,
            attendees: vec![],
            calendar_name: "Work".into(),
            is_all_day: false,
            meeting_url: None,
            notes: None,
            participants: vec![],
        }
    }

    #[test]
    fn picks_event_with_most_overlap() {
        let base = DateTime::parse_from_rfc3339("2026-09-24T16:00:00Z").unwrap().with_timezone(&Utc);
        let events = vec![ev("standup", 0, 15), ev("sync", 15, 60)];
        // Recording 16:12 → 17:05: mostly the sync
        let got = best_event_for(base + Duration::minutes(12), base + Duration::minutes(65), &events);
        assert_eq!(got.unwrap().event_id, "sync");
    }

    #[test]
    fn early_start_still_matches() {
        let base = DateTime::parse_from_rfc3339("2026-09-24T16:00:00Z").unwrap().with_timezone(&Utc);
        let events = vec![ev("sync", 0, 30)];
        let got = best_event_for(base - Duration::minutes(4), base + Duration::minutes(25), &events);
        assert!(got.is_some());
    }

    #[test]
    fn ignores_trivial_overlap_and_all_day() {
        let base = DateTime::parse_from_rfc3339("2026-09-24T16:00:00Z").unwrap().with_timezone(&Utc);
        let mut all_day = ev("offsite", -600, 1440);
        all_day.is_all_day = true;
        let events = vec![ev("earlier", -60, 61), all_day];
        // Recording 16:00 → 17:00 touches "earlier" for 1 minute only
        assert!(best_event_for(base, base + Duration::minutes(60), &events).is_none());
    }

    #[test]
    fn linkedin_urls_normalize() {
        assert_eq!(
            normalize_linkedin_url("linkedin.com/in/jane-doe/").unwrap(),
            "https://www.linkedin.com/in/jane-doe"
        );
        assert_eq!(
            normalize_linkedin_url("https://www.linkedin.com/in/jane-doe?utm_source=x").unwrap(),
            "https://www.linkedin.com/in/jane-doe"
        );
        assert!(normalize_linkedin_url("https://example.com/in/jane").is_err());
        assert!(normalize_linkedin_url("https://www.linkedin.com/company/acme").is_err());
    }

    #[test]
    fn default_titles_detected() {
        assert!(is_default_title("Meeting 2026-09-24 09:01"));
        assert!(is_default_title("Meeting — Oct 7"));
        assert!(is_default_title("Personal — Dec 25"));
        assert!(!is_default_title("Weekly sync"));
    }
}
