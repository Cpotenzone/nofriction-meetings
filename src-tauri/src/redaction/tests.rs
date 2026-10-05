//! Tests for docs/REDACTION.md on the Mac.

use super::*;
use crate::database::DatabaseManager;
use std::sync::Arc;

struct Fixture {
    db: Arc<DatabaseManager>,
    dir: PathBuf,
    env: RedactionEnv,
    db_path: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn setup() -> Fixture {
    let dir = std::env::temp_dir().join(format!("nf-redact-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db_path = data.join("nofriction_meetings.db");
    let db = DatabaseManager::new(&db_path).await.unwrap();
    db.run_migrations().await.unwrap();
    db.create_meeting("m1", "Board sync").await.unwrap();
    let env = RedactionEnv {
        app_data_dir: data,
        cache_dir: dir.join("cache"),
        video_enabled: false,
        recording_meetings: Vec::new(),
    };
    Fixture { db: Arc::new(db), dir, env, db_path }
}

fn utf16(s: &str) -> usize {
    s.encode_utf16().count()
}

/// UTF-16 [start, end) of `needle` inside `hay`
fn span(hay: &str, needle: &str) -> (usize, usize) {
    let b = hay.find(needle).expect("needle in line");
    let s = utf16(&hay[..b]);
    (s, s + utf16(needle))
}

async fn add_line(f: &Fixture, text: &str) -> i64 {
    f.db.add_transcript("m1", text, Some("Alice"), true, 0.9).await.unwrap()
}

async fn line_text(f: &Fixture, id: i64) -> Option<String> {
    sqlx::query_scalar("SELECT text FROM transcripts WHERE id = ?")
        .bind(id)
        .fetch_optional(f.db.pool())
        .await
        .unwrap()
}

async fn fts_hits(f: &Fixture, q: &str) -> usize {
    f.db.search_transcripts(q).await.unwrap().len()
}

fn target(id: i64, (start, end): (usize, usize)) -> WordTarget {
    WordTarget { meeting_id: "m1".into(), transcript_id: id, start, end, expected_text: None, whole_line: false }
}

// ─── Pure text edits ─────────────────────────────────────────────────────

#[test]
fn word_edit_removes_exact_words_and_closes_whitespace() {
    let t = "We should acquire Zenith Labs next quarter.";
    let (s, e) = span(t, "Zenith Labs");
    let ed = apply_word_edit(t, None, s, e, None).unwrap();
    assert_eq!(ed.new_text, "We should acquire next quarter.");
    assert_eq!(ed.removed_plain, "Zenith Labs");
    assert!(!ed.whole_line);

    // first word, last word (with its punctuation), whole line
    let ed = apply_word_edit(t, None, 0, 2, None).unwrap();
    assert_eq!(ed.new_text, "should acquire Zenith Labs next quarter.");
    let (s, e) = span(t, "quarter.");
    assert_eq!(apply_word_edit(t, None, s, e, None).unwrap().new_text, "We should acquire Zenith Labs next");
    let ed = apply_word_edit(t, None, 0, utf16(t), None).unwrap();
    assert_eq!(ed.new_text, "");
    assert!(ed.whole_line);
}

#[test]
fn word_edit_snaps_partial_selection_to_whole_words() {
    let t = "the password is hunter2 okay";
    // select "ssw" inside "password" and "hun" inside "hunter2"
    let s = utf16("the pa");
    let e = utf16("the password is hun");
    let ed = apply_word_edit(t, None, s, e, None).unwrap();
    assert_eq!(ed.new_text, "the okay");
    assert_eq!(ed.removed_plain, "password is hunter2");
}

#[test]
fn word_edit_handles_extra_whitespace_and_unicode() {
    let t = "  café   naïve 😀 secret   end ";
    let (s, e) = span(t, "secret");
    let ed = apply_word_edit(t, None, s, e, None).unwrap();
    // Only the edit site closes up; untouched text is kept verbatim
    assert_eq!(ed.new_text, "  café   naïve 😀 end ");
    // emoji is 2 UTF-16 units; an offset inside the surrogate pair is rejected
    let emoji_at = utf16("  café   naïve ");
    assert!(apply_word_edit(t, None, emoji_at + 1, emoji_at + 2, None).is_err());
    assert!(apply_word_edit(t, None, 3, 3, None).is_err());
    assert!(apply_word_edit(t, None, 0, 999, None).is_err());
    // selecting only whitespace removes nothing
    assert!(apply_word_edit("a    b", None, 2, 4, None).is_err());
}

#[test]
fn strike_splices_one_marker_and_keeps_existing_markers() {
    let id = new_id();
    let m = marker_token(&id);
    let t = "the merger price is 40 million dollars";
    let (s, e) = span(t, "40 million");
    let ed = apply_word_edit(t, None, s, e, Some(&m)).unwrap();
    assert_eq!(ed.new_text, format!("the merger price is {} dollars", m));
    assert_eq!(render_plain(&ed.new_text), "the merger price is [stricken from the record] dollars");
    assert_eq!(marker_ids(&ed.new_text), vec![id.clone()]);

    // A later edit spanning the marker can't remove it
    let t2 = ed.new_text.clone();
    let (s, e) = (0, utf16(&t2));
    let ed2 = apply_word_edit(&t2, None, s, e, None).unwrap();
    assert_eq!(ed2.new_text, m);
    // and selecting only the marker is "already stricken"
    let (s, e) = span(&t2, &m);
    assert!(apply_word_edit(&t2, None, s, e, None).unwrap_err().contains("already stricken"));
}

#[test]
fn word_timings_drop_removed_and_shift_later_words() {
    let t = "alpha beta gamma delta";
    let timings = serde_json::to_string(&vec![
        WordTiming { s: 0, e: 5, t0: 0, t1: 400 },
        WordTiming { s: 6, e: 10, t0: 400, t1: 800 },
        WordTiming { s: 11, e: 16, t0: 800, t1: 1200 },
        WordTiming { s: 17, e: 22, t0: 1200, t1: 1600 },
    ])
    .unwrap();
    let (s, e) = span(t, "beta gamma");
    let ed = apply_word_edit(t, Some(&timings), s, e, None).unwrap();
    assert_eq!(ed.new_text, "alpha delta");
    assert_eq!(ed.removed_ms, Some((400, 1200)));
    let kept: Vec<WordTiming> = serde_json::from_str(ed.new_timings.as_deref().unwrap()).unwrap();
    assert_eq!(kept, vec![
        WordTiming { s: 0, e: 5, t0: 0, t1: 400 },
        WordTiming { s: 6, e: 11, t0: 1200, t1: 1600 },
    ]);
    assert_eq!(&ed.new_text[6..11], "delta");
}

#[test]
fn phrase_regex_is_whole_word_and_case_insensitive() {
    let re = phrase_regex("Plan.").unwrap();
    assert!(re.is_match("the PLAN is set"));
    assert!(!re.is_match("planet earth"));
    let re = phrase_regex("Zenith   Labs").unwrap();
    assert!(re.is_match("buy zenith labs soon"));
    assert!(phrase_regex("...").is_none());
}

#[test]
fn reason_may_not_contain_removed_words() {
    assert_eq!(validate_reason(Some("  privileged "), Some("Zenith Labs")).unwrap(), Some("privileged".into()));
    assert!(validate_reason(Some("about zenith"), Some("Zenith Labs")).is_err());
    assert!(validate_reason(Some(&"x".repeat(200)), None).is_err());
    assert_eq!(validate_reason(Some("   "), None).unwrap(), None);
}

// ─── Schema / pragmas ────────────────────────────────────────────────────

#[tokio::test]
async fn secure_delete_is_on_for_every_pool_connection() {
    let f = setup().await;
    let mut conns = Vec::new();
    for _ in 0..3 {
        conns.push(f.db.pool().acquire().await.unwrap());
    }
    for c in conns.iter_mut() {
        let v: i64 = sqlx::query_scalar("PRAGMA secure_delete").fetch_one(&mut **c).await.unwrap();
        assert_eq!(v, 1);
    }
}

#[tokio::test]
async fn fts_update_trigger_keeps_search_in_sync() {
    let f = setup().await;
    let id = add_line(&f, "kubernetes rollout tomorrow").await;
    assert_eq!(fts_hits(&f, "kubernetes").await, 1);
    sqlx::query("UPDATE transcripts SET text = 'nomad rollout tomorrow' WHERE id = ?")
        .bind(id)
        .execute(f.db.pool())
        .await
        .unwrap();
    assert_eq!(fts_hits(&f, "kubernetes").await, 0);
    assert_eq!(fts_hits(&f, "nomad").await, 1);
    // integrity-check: external-content index matches the table
    sqlx::query("INSERT INTO transcripts_fts(transcripts_fts) VALUES('integrity-check')")
        .execute(f.db.pool())
        .await
        .unwrap();
}

// ─── Delete (undo window) ────────────────────────────────────────────────

#[tokio::test]
async fn delete_words_commits_after_window_and_leaves_no_trace() {
    let f = setup().await;
    let text = "send the wire to Cayman account tonight";
    let id = add_line(&f, text).await;
    let p = request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "Cayman account"))).await.unwrap();
    assert_eq!(p.undo_seconds, UNDO_WINDOW_SECS);
    // Still there during the window
    assert_eq!(line_text(&f, id).await.unwrap(), text);

    let out = commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap().unwrap();
    assert!(out.record.is_none());
    assert_eq!(line_text(&f, id).await.unwrap(), "send the wire to tonight");
    assert_eq!(fts_hits(&f, "cayman").await, 0);
    assert_eq!(fts_hits(&f, "wire").await, 1);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM redactions").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(n, 0, "Delete leaves no record");
    // Committing again is a no-op; undo is too late
    assert!(commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap().is_none());
    assert!(undo_delete(f.db.pool(), &p.id).await.is_err());
}

#[tokio::test]
async fn undo_cancels_a_pending_delete() {
    let f = setup().await;
    let text = "keep every word here";
    let id = add_line(&f, text).await;
    let p = request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "every"))).await.unwrap();
    undo_delete(f.db.pool(), &p.id).await.unwrap();
    assert!(commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap().is_none());
    assert_eq!(line_text(&f, id).await.unwrap(), text);
}

#[tokio::test]
async fn pending_delete_is_committed_on_quit_or_next_launch() {
    let f = setup().await;
    let text = "drop this line entirely";
    let id = add_line(&f, text).await;
    request_delete_words(f.db.pool(), &f.env, &target(id, (0, utf16(text)))).await.unwrap();
    // Simulates RunEvent::Exit / startup after a crash
    let errors = commit_all_pending(f.db.pool(), &f.env).await;
    assert!(errors.is_empty(), "{:?}", errors);
    assert!(line_text(&f, id).await.is_none(), "an emptied line is removed");
    assert_eq!(fts_hits(&f, "entirely").await, 0);
}

#[tokio::test]
async fn new_action_flushes_pending_delete_first() {
    let f = setup().await;
    let text = "one two three four";
    let id = add_line(&f, text).await;
    request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "two"))).await.unwrap();
    // Offsets for the second action refer to the text after the first commits
    let after = "one three four";
    request_delete_words(f.db.pool(), &f.env, &target(id, span(after, "three"))).await.unwrap();
    assert_eq!(line_text(&f, id).await.unwrap(), "one three four");
    commit_all_pending(f.db.pool(), &f.env).await;
    assert_eq!(line_text(&f, id).await.unwrap(), "one four");
}

#[tokio::test]
async fn pending_deletes_on_other_lines_keep_their_undo_window() {
    let f = setup().await;
    let a = add_line(&f, "first line goes").await;
    let b = add_line(&f, "second line goes").await;
    let pa = request_delete_words(f.db.pool(), &f.env, &target(a, (0, utf16("first line goes")))).await.unwrap();
    let pb = request_delete_words(f.db.pool(), &f.env, &target(b, (0, utf16("second line goes")))).await.unwrap();
    // Both still undoable (multi-line delete = one Undo)
    undo_delete(f.db.pool(), &pa.id).await.unwrap();
    undo_delete(f.db.pool(), &pb.id).await.unwrap();
    assert_eq!(line_text(&f, a).await.unwrap(), "first line goes");
    assert_eq!(line_text(&f, b).await.unwrap(), "second line goes");
}

#[tokio::test]
async fn whole_line_target_resolves_after_flushing_pending_delete() {
    let f = setup().await;
    let text = "one two three";
    let id = add_line(&f, text).await;
    request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "two"))).await.unwrap();
    // Whole-line strike: the range is taken after the pending delete commits
    // (the old text's length would be out of range for "one three")
    let t = WordTarget { whole_line: true, ..target(id, (0, 0)) };
    let rec = strike_words(f.db.pool(), &f.env, &t, None).await.unwrap().record.unwrap();
    assert_eq!(rec.kind, "line");
    assert_eq!(line_text(&f, id).await.unwrap(), marker_token(&rec.id));
}

#[tokio::test]
async fn transient_commit_failure_keeps_the_delete_pending_and_retries() {
    let mut f = setup().await;
    let a = add_screen(&f, "a", 10).await;
    let p = request_delete_screens(f.db.pool(), &f.env, "m1", &[a.clone()]).await.unwrap();
    // A recording started in the undo window: the commit can't apply now
    let recording = RedactionEnv { recording_meetings: vec!["m1".into()], ..f.env.clone() };
    let e = commit_pending(f.db.pool(), &recording, &p.id).await.unwrap_err();
    assert_eq!(e, RECORDING_SCREENS_ERROR);
    assert!(is_pending(f.db.pool(), &p.id).await, "not silently dropped");
    let failed = list_failed_deletes(f.db.pool(), "m1").await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].id, p.id);
    assert_eq!(failed[0].failure, RECORDING_SCREENS_ERROR);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM screen_states").await, 1);
    // Retried at the next launch (commit_all_pending) still failing: kept
    assert_eq!(commit_all_pending(f.db.pool(), &recording).await.len(), 1);
    assert!(is_pending(f.db.pool(), &p.id).await);

    // Next commit attempt (another delete) retries it once recording stopped
    f.env.recording_meetings.clear();
    let text = "unrelated words here";
    let line = add_line(&f, text).await;
    let p2 = request_delete_words(f.db.pool(), &f.env, &target(line, span(text, "words"))).await.unwrap();
    commit_pending(f.db.pool(), &f.env, &p2.id).await.unwrap().unwrap();
    assert!(!is_pending(f.db.pool(), &p.id).await);
    assert!(list_failed_deletes(f.db.pool(), "m1").await.unwrap().is_empty());
    assert_eq!(count(&f, "SELECT COUNT(*) FROM screen_states").await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM redactions").await, 0, "Delete leaves no trace");
}

#[tokio::test]
async fn retry_button_applies_failed_deletes() {
    let f = setup().await;
    let a = add_screen(&f, "a", 10).await;
    let p = request_delete_screens(f.db.pool(), &f.env, "m1", &[a]).await.unwrap();
    let recording = RedactionEnv { recording_meetings: vec!["m1".into()], ..f.env.clone() };
    assert!(commit_pending(f.db.pool(), &recording, &p.id).await.is_err());
    assert!(retry_failed_deletes(f.db.pool(), &f.env, "m1").await.is_empty());
    assert!(!is_pending(f.db.pool(), &p.id).await);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM screen_states").await, 0);
}

#[tokio::test]
async fn permanent_mismatch_drops_the_pending_delete_and_reports_it() {
    let f = setup().await;
    let text = "first draft of the line";
    let id = add_line(&f, text).await;
    let p = request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "draft"))).await.unwrap();
    sqlx::query("UPDATE transcripts SET text = 'rewritten by someone else' WHERE id = ?")
        .bind(id)
        .execute(f.db.pool())
        .await
        .unwrap();
    let e = commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap_err();
    assert!(e.contains("changed"), "{}", e);
    assert!(!is_pending(f.db.pool(), &p.id).await, "can never apply: dropped");
    assert!(list_failed_deletes(f.db.pool(), "m1").await.unwrap().is_empty());
    assert_eq!(line_text(&f, id).await.unwrap(), "rewritten by someone else");
}

// ─── Strike ──────────────────────────────────────────────────────────────

/// Every table/column, as text, plus the raw database + WAL bytes.
async fn content_anywhere(f: &Fixture, needle: &str) -> Vec<String> {
    let mut hits = Vec::new();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND sql NOT LIKE 'CREATE VIRTUAL%'",
    )
    .fetch_all(f.db.pool())
    .await
    .unwrap();
    for t in tables {
        let cols: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
            .bind(&t)
            .fetch_all(f.db.pool())
            .await
            .unwrap();
        for c in cols {
            let n: i64 = sqlx::query_scalar(&format!(
                "SELECT COUNT(*) FROM \"{}\" WHERE instr(lower(CAST(\"{}\" AS TEXT)), lower(?)) > 0",
                t, c
            ))
            .bind(needle)
            .fetch_one(f.db.pool())
            .await
            .unwrap();
            if n > 0 {
                hits.push(format!("{}.{}", t, c));
            }
        }
    }
    for suffix in ["", "-wal"] {
        let p = PathBuf::from(format!("{}{}", f.db_path.display(), suffix));
        if let Ok(bytes) = std::fs::read(&p) {
            if bytes.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle.as_bytes())) {
                hits.push(format!("file{}", suffix));
            }
        }
    }
    hits
}

#[tokio::test]
async fn strike_leaves_only_a_marker_and_no_copy_anywhere() {
    let f = setup().await;
    let secret = "xyzzyplugh";
    let text = format!("the codename is {} until launch", secret);
    let id = add_line(&f, &text).await;
    add_line(&f, "unrelated line about launch").await;
    wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    assert!(!content_anywhere(&f, secret).await.is_empty(), "precondition: content is stored");

    let out = strike_words(f.db.pool(), &f.env, &target(id, span(&text, secret)), Some("privileged"))
        .await
        .unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let rec = out.record.unwrap();
    assert_eq!(rec.kind, "words");
    assert_eq!(rec.action, "strike");
    assert_eq!(rec.reason.as_deref(), Some("privileged"));
    assert!(rec.media_start.is_some());

    // Marker in the stored text; plain text for prompts/exports
    let raw = line_text(&f, id).await.unwrap();
    assert_eq!(raw, format!("the codename is {} until launch", marker_token(&rec.id)));
    let plain = f.db.get_transcripts("m1").await.unwrap();
    assert!(plain.iter().any(|t| t.text == "the codename is [stricken from the record] until launch"));
    assert_eq!(fts_hits(&f, secret).await, 0);

    // The record holds when/where/why, never what
    let listed = list_strikes(f.db.pool(), "m1").await.unwrap();
    assert_eq!(listed, vec![rec.clone()]);
    assert!(!serde_json::to_string(&listed).unwrap().contains(secret));

    // No API, table, column, or byte on disk holds it any more
    assert_eq!(content_anywhere(&f, secret).await, Vec::<String>::new());
    let timeline = f.db.get_synced_timeline("m1").await.unwrap().unwrap();
    assert!(!serde_json::to_string(&timeline).unwrap().contains(secret));
    assert_eq!(timeline.redactions.len(), 1);
}

#[tokio::test]
async fn strike_cannot_be_undone_or_edited_but_goes_with_its_meeting() {
    let f = setup().await;
    let text = "confidential settlement figure";
    let id = add_line(&f, text).await;
    let rec = strike_words(f.db.pool(), &f.env, &target(id, span(text, "settlement figure")), None)
        .await
        .unwrap()
        .record
        .unwrap();
    assert!(undo_delete(f.db.pool(), &rec.id).await.unwrap_err().contains("can't be undone"));
    assert!(sqlx::query("UPDATE redactions SET reason = 'x' WHERE id = ?")
        .bind(&rec.id)
        .execute(f.db.pool())
        .await
        .is_err());
    assert!(sqlx::query("DELETE FROM redactions WHERE id = ?")
        .bind(&rec.id)
        .execute(f.db.pool())
        .await
        .is_err());
    // A later edit of the line keeps the marker
    let raw = line_text(&f, id).await.unwrap();
    let p = request_delete_words(f.db.pool(), &f.env, &target(id, (0, utf16(&raw)))).await.unwrap();
    commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    assert_eq!(line_text(&f, id).await.unwrap(), marker_token(&rec.id));

    f.db.delete_meeting("m1").await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM redactions").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn strike_whole_line_is_kind_line_and_uses_word_timings() {
    let f = setup().await;
    let ts = Utc::now();
    let text = "alpha beta";
    let timings = serde_json::to_string(&vec![
        WordTiming { s: 0, e: 5, t0: 100, t1: 500 },
        WordTiming { s: 6, e: 10, t0: 600, t1: 900 },
    ])
    .unwrap();
    let id = f.db.add_transcript_full("m1", text, None, true, 0.9, ts, Some(&timings)).await.unwrap();
    let rec = strike_words(f.db.pool(), &f.env, &target(id, (0, utf16(text))), None)
        .await
        .unwrap()
        .record
        .unwrap();
    assert_eq!(rec.kind, "line");
    let start = parse_ts(rec.media_start.as_deref().unwrap()).unwrap();
    let end = parse_ts(rec.media_end.as_deref().unwrap()).unwrap();
    assert_eq!((start - ts).num_milliseconds(), 100);
    assert_eq!((end - ts).num_milliseconds(), 900);
    let wt: Option<String> = sqlx::query_scalar("SELECT word_timings FROM transcripts WHERE id = ?")
        .bind(id)
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert!(wt.is_none(), "no timings survive for removed words");
}

#[tokio::test]
async fn reason_with_removed_words_is_rejected_and_nothing_changes() {
    let f = setup().await;
    let text = "project bluebird is late";
    let id = add_line(&f, text).await;
    let e = strike_words(f.db.pool(), &f.env, &target(id, span(text, "bluebird")), Some("re: bluebird"))
        .await
        .unwrap_err();
    assert!(e.contains("reason"));
    assert_eq!(line_text(&f, id).await.unwrap(), text);
}

// ─── AI outputs ──────────────────────────────────────────────────────────

#[tokio::test]
async fn ai_outputs_are_redacted_and_flagged() {
    let f = setup().await;
    let text = "we will acquire Zenith Labs in May";
    let id = add_line(&f, text).await;
    f.db.save_meeting_notes(
        "n1",
        "m1",
        Some("Team agreed to acquire zenith labs."),
        Some(r#"["Zenith Labs acquisition","Hiring"]"#),
        Some(r#"[{"text":"Buy ZENITH LABS","made_by":"Alice"}]"#),
        Some("[]"),
        Some("[]"),
        Some("test"),
    )
    .await
    .unwrap();
    f.db.save_study_materials("s1", "m1", Some("About Zenith Labs"), None, None, None, None).await.unwrap();
    f.db.add_assistant_conversation(
        "c1",
        &Utc::now().to_rfc3339(),
        "what about zenith labs?",
        "You said you'd acquire Zenith Labs.",
        "x",
        &[format!("transcript-m1-{}", id)],
    )
    .await
    .unwrap();
    f.db.add_assistant_conversation("c2", &Utc::now().to_rfc3339(), "zenith labs elsewhere", "n/a", "x", &[])
        .await
        .unwrap();
    f.db.add_meeting_comment("k1", "m1", "follow up on Zenith Labs", None, None, None).await.unwrap();

    strike_words(f.db.pool(), &f.env, &target(id, span(text, "Zenith Labs")), None).await.unwrap();

    let notes = f.db.get_meeting_notes("m1").await.unwrap().unwrap();
    assert_eq!(notes.summary.as_deref(), Some("Team agreed to acquire [stricken from the record]."));
    assert_eq!(notes.key_topics.as_deref(), Some(r#"["[stricken from the record] acquisition","Hiring"]"#));
    assert!(notes.decisions.unwrap().contains("Buy [stricken from the record]"));
    assert!(notes.stale_after_edit);
    let (sum, stale): (String, i64) =
        sqlx::query_as("SELECT summary, stale_after_edit FROM study_materials WHERE id = 's1'")
            .fetch_one(f.db.pool())
            .await
            .unwrap();
    assert_eq!(sum, "About [stricken from the record]");
    assert_eq!(stale, 1);
    let convs = f.db.list_assistant_conversations(10).await.unwrap();
    let c1 = convs.iter().find(|c| c.0 == "c1").unwrap();
    assert_eq!(c1.2, "what about [stricken from the record]?");
    assert!(!c1.3.to_lowercase().contains("zenith"));
    // A chat that never used this meeting is left alone
    assert!(convs.iter().find(|c| c.0 == "c2").unwrap().2.contains("zenith"));
    // User-authored comments are never rewritten
    let comments = f.db.get_meeting_comments("m1").await.unwrap();
    assert_eq!(comments[0].comment, "follow up on Zenith Labs");

    // Regenerating (INSERT OR REPLACE of the row) clears the flag
    f.db.save_meeting_notes("n1", "m1", Some("fresh"), None, None, None, None, None).await.unwrap();
    assert!(!f.db.get_meeting_notes("m1").await.unwrap().unwrap().stale_after_edit);
}

#[tokio::test]
async fn delete_redacts_ai_outputs_without_a_strike_marker() {
    let f = setup().await;
    let text = "ping Roberta about invoices";
    let id = add_line(&f, text).await;
    f.db.save_meeting_notes("n1", "m1", Some("Ping Roberta about invoices"), None, None, None, None, None)
        .await
        .unwrap();
    let p = request_delete_words(f.db.pool(), &f.env, &target(id, span(text, "Roberta"))).await.unwrap();
    commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap();
    let notes = f.db.get_meeting_notes("m1").await.unwrap().unwrap();
    assert_eq!(notes.summary.as_deref(), Some("Ping [removed] about invoices"));
    assert!(notes.stale_after_edit);
}

#[test]
fn only_distinctive_removals_propagate() {
    assert!(!is_distinctive("plan"));
    assert!(!is_distinctive("Plan."));
    assert!(!is_distinctive("because"), "common long word");
    assert!(!is_distinctive("Really,"));
    assert!(!is_distinctive("..."));
    assert!(is_distinctive("Marguerite"));
    assert!(is_distinctive("acmecorp"));
    assert!(is_distinctive("the plan"), "two words");
    assert!(is_distinctive("Zenith Labs"));
    assert!(distinctive_phrase_regex("team").is_none());
    assert!(distinctive_phrase_regex("Zenith Labs").is_some());
}

#[tokio::test]
async fn common_word_edit_only_flags_ai_outputs_and_leaves_comments_and_logs() {
    let f = setup().await;
    let logs = f.env.app_data_dir.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("app.log"), "INFO the plan for Q3 is ready\n").unwrap();
    let text = "the plan is late";
    let id = add_line(&f, text).await;
    add_line(&f, "we need a plan for hiring").await;
    f.db.save_meeting_notes("n1", "m1", Some("The plan for hiring is approved."), None, None, None, None, None)
        .await
        .unwrap();
    f.db.add_meeting_comment("k1", "m1", "my plan: call them", None, None, None).await.unwrap();

    strike_words(f.db.pool(), &f.env, &target(id, span(text, "plan")), None).await.unwrap();

    // The line itself is edited...
    assert_eq!(render_plain(&line_text(&f, id).await.unwrap()), "the [stricken from the record] is late");
    // ...but every other "plan" in the meeting's outputs is left alone, and
    // the outputs are only flagged "made before an edit"
    let notes = f.db.get_meeting_notes("m1").await.unwrap().unwrap();
    assert_eq!(notes.summary.as_deref(), Some("The plan for hiring is approved."));
    assert!(notes.stale_after_edit);
    assert_eq!(f.db.get_meeting_comments("m1").await.unwrap()[0].comment, "my plan: call them");
    assert_eq!(std::fs::read_to_string(logs.join("app.log")).unwrap(), "INFO the plan for Q3 is ready\n");
    assert_eq!(fts_hits(&f, "hiring").await, 1, "other lines untouched");
}

#[tokio::test]
async fn common_word_edit_still_purges_the_line_in_backups_but_keeps_them() {
    let f = setup().await;
    let text = "the plan is late";
    let id = add_line(&f, text).await;
    add_line(&f, "another plan entirely").await;
    wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    let bdir = f.env.app_data_dir.join("backups");
    std::fs::create_dir_all(&bdir).unwrap();
    let copy = bdir.join("copy.db");
    std::fs::copy(&f.db_path, &copy).unwrap();
    let out = strike_words(f.db.pool(), &f.env, &target(id, span(text, "plan")), None).await.unwrap();
    assert_eq!((out.backups_purged, out.backups_deleted), (1, 0), "{:?}", out.warnings);
    let mut c = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&copy)).await.unwrap();
    let t: String = sqlx::query_scalar("SELECT text FROM transcripts WHERE id = ?").bind(id).fetch_one(&mut c).await.unwrap();
    assert_eq!(render_plain(&t), "the [stricken from the record] is late");
}

// ─── Exports and prompts ─────────────────────────────────────────────────

#[tokio::test]
async fn exports_and_prompts_render_placeholders() {
    let f = setup().await;
    let text = "the vendor is Acmecorp for now";
    let id = add_line(&f, text).await;
    strike_words(f.db.pool(), &f.env, &target(id, span(text, "Acmecorp")), None).await.unwrap();
    let state_id = add_screen(&f, "s-export", 30).await;
    strike_screens(f.db.pool(), &f.env, "m1", &[state_id], None).await.unwrap();

    // AI prompt text
    let prompt = crate::meeting_notes::transcript_for_prompt(&f.db.get_transcripts("m1").await.unwrap());
    assert!(prompt.contains("the vendor is [stricken from the record] for now"));
    assert!(!prompt.contains("strickenid"));
    assert!(!prompt.to_lowercase().contains("acmecorp"));
    // Even a caller holding marked rows can't leak the marker id
    let marked = crate::meeting_notes::transcript_for_prompt(&f.db.get_transcripts_marked("m1").await.unwrap());
    assert!(!marked.contains("strickenid"));

    // Obsidian/Markdown export
    let vault_dir = f.dir.join("vault");
    std::fs::create_dir_all(&vault_dir).unwrap();
    let vault = Arc::new(crate::obsidian_vault::VaultManager::new());
    vault.set_vault_path(vault_dir.to_string_lossy().to_string());
    crate::commands::vault::internal_export_meeting(f.db.clone(), vault, "Topic".into(), "m1".into())
        .await
        .unwrap();
    let mut md = String::new();
    let mut stack = vec![vault_dir.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            } else if e.path().file_name().map_or(false, |n| n == "transcript.md") {
                md = std::fs::read_to_string(e.path()).unwrap();
            }
        }
    }
    assert!(md.contains("the vendor is [stricken from the record] for now"), "{}", md);
    assert!(md.contains(SCREEN_STRICKEN_PLACEHOLDER), "{}", md);
    assert!(!md.contains("strickenid"));
}

// ─── Screens ─────────────────────────────────────────────────────────────

/// A screen state with a keyframe file, OCR snapshot, VLM queue + activity
/// (+ entity), a timeline event and an episode. Returns its state_id.
async fn add_screen(f: &Fixture, name: &str, secs: i64) -> String {
    add_screen_at(f, name, Utc::now() - chrono::Duration::seconds(600 - secs)).await
}

async fn add_screen_at(f: &Fixture, name: &str, ts: DateTime<Utc>) -> String {
    let frames = f.env.app_data_dir.join("frames").join("m1");
    std::fs::create_dir_all(&frames).unwrap();
    let state_id = format!("{}-{}", name, uuid::Uuid::new_v4());
    let path = frames.join(format!("state_{}.jpg", state_id));
    std::fs::write(&path, b"jpeg").unwrap();
    let p = path.to_string_lossy().to_string();
    f.db.add_screen_state(&state_id, "m1", ts, Some(ts), "", 0.0, Some(&p), "other", "{}").await.unwrap();
    let ep = format!("ep-{}", state_id);
    f.db.create_episode(&ep, "m1", ts, Some("Mail"), Some("Secret deal memo")).await.unwrap();
    f.db.link_state_to_episode(&ep, &state_id, 0).await.unwrap();
    f.db.add_text_snapshot_full(
        &format!("snap-{}", state_id),
        Some(&ep),
        Some(&state_id),
        Some("m1"),
        ts,
        "OCR: wire 2M to account 4471",
        "h",
        0.9,
        "ocr",
        Some("Mail"),
        Some("Secret deal memo"),
    )
    .await
    .unwrap();
    let qid = f.db.queue_frame(None, &p, ts).await.unwrap();
    let activity = crate::database::ActivityLogEntry {
        id: None,
        start_time: ts,
        end_time: None,
        duration_seconds: None,
        app_name: Some("Mail".into()),
        window_title: Some("Secret deal memo".into()),
        category: "other".into(),
        summary: "User reads the 4471 wire memo".into(),
        focus_area: None,
        visible_files: None,
        confidence: Some(0.5),
        frame_ids: Some(qid.to_string()),
    };
    let aid = f.db.add_activity(&activity).await.unwrap();
    f.db.add_entity(aid, "account", "4471", None, 0.5, None).await.unwrap();
    sqlx::query("INSERT INTO meeting_timeline_events (event_id, meeting_id, ts, event_type, title, state_id) VALUES (?, 'm1', ?, 'screen', 'Memo 4471', ?)")
        .bind(format!("ev-{}", state_id))
        .bind(ts.to_rfc3339())
        .bind(&state_id)
        .execute(f.db.pool())
        .await
        .unwrap();
    state_id
}

async fn count(f: &Fixture, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(f.db.pool()).await.unwrap()
}

#[tokio::test]
async fn screen_delete_removes_files_derived_files_and_rows() {
    let f = setup().await;
    let a = add_screen(&f, "a", 10).await;
    let b = add_screen(&f, "b", 20).await;
    let legacy_file = f.env.app_data_dir.join("frames").join("m1").join("frame_1.jpg");
    std::fs::write(&legacy_file, b"jpeg").unwrap();
    let legacy = f.db.add_frame("m1", Utc::now(), Some(&legacy_file.to_string_lossy()), Some("ocr 4471")).await.unwrap();
    let cache = f.env.cache_dir.join("m1").join("thumbnails");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("thumb_1.000.jpg"), b"x").unwrap();

    let p = request_delete_screens(f.db.pool(), &f.env, "m1", &[a.clone(), legacy.to_string()]).await.unwrap();
    // nothing removed during the undo window
    assert!(legacy_file.exists());
    commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap().unwrap();

    assert!(!legacy_file.exists());
    assert!(!f.env.app_data_dir.join("frames/m1").join(format!("state_{}.jpg", a)).exists());
    assert!(f.env.app_data_dir.join("frames/m1").join(format!("state_{}.jpg", b)).exists(), "unselected screen kept");
    assert!(!cache.exists(), "cached thumbnails removed");
    assert_eq!(count(&f, &format!("SELECT COUNT(*) FROM screen_states WHERE state_id = '{}'", a)).await, 0);
    assert_eq!(count(&f, &format!("SELECT COUNT(*) FROM text_snapshots WHERE state_id = '{}'", a)).await, 0);
    assert_eq!(count(&f, &format!("SELECT COUNT(*) FROM document_episodes WHERE episode_id = 'ep-{}'", a)).await, 0);
    assert_eq!(count(&f, &format!("SELECT COUNT(*) FROM meeting_timeline_events WHERE state_id = '{}'", a)).await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM frames").await, 0);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM screen_states").await, 1);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM frame_queue").await, 1, "only b's VLM queue row left");
    assert_eq!(count(&f, "SELECT COUNT(*) FROM activity_log").await, 1);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM entities").await, 1);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM redactions").await, 0, "Delete leaves no trace");
}

#[tokio::test]
async fn screen_strike_leaves_only_a_marker() {
    let f = setup().await;
    let a = add_screen(&f, "a", 10).await;
    let b = add_screen(&f, "b", 40).await;
    let out = strike_screens(f.db.pool(), &f.env, "m1", &[a.clone(), b.clone()], Some("client data"))
        .await
        .unwrap();
    let rec = out.record.unwrap();
    assert_eq!(rec.kind, "screen");
    assert_eq!(rec.item_count, 2);
    assert!(rec.media_start <= rec.media_end);
    for t in ["screen_states", "text_snapshots", "frame_queue", "activity_log", "entities", "meeting_timeline_events", "document_episodes", "episode_states"] {
        assert_eq!(count(&f, &format!("SELECT COUNT(*) FROM {}", t)).await, 0, "{}", t);
    }
    assert!(std::fs::read_dir(f.env.app_data_dir.join("frames/m1")).unwrap().next().is_none());
    assert_eq!(content_anywhere(&f, "4471").await, Vec::<String>::new());
    let tl = f.db.get_synced_timeline("m1").await.unwrap().unwrap();
    assert!(tl.frames.is_empty());
    assert_eq!(tl.redactions, vec![rec]);
    // Unknown ids fail without changing anything
    assert!(strike_screens(f.db.pool(), &f.env, "m1", &["nope".into()], None).await.is_err());
}

#[tokio::test]
async fn screens_of_the_recording_meeting_cannot_be_edited() {
    let mut f = setup().await;
    let a = add_screen(&f, "a", 10).await;
    f.env.recording_meetings = vec!["m1".into()];
    let e = strike_screens(f.db.pool(), &f.env, "m1", &[a.clone()], None).await.unwrap_err();
    assert_eq!(e, RECORDING_SCREENS_ERROR);
    assert_eq!(request_delete_screens(f.db.pool(), &f.env, "m1", &[a.clone()]).await.unwrap_err(), RECORDING_SCREENS_ERROR);
    assert!(preview_screens(f.db.pool(), &f.env, "m1", &[a.clone()]).await.is_err());
    assert_eq!(count(&f, "SELECT COUNT(*) FROM screen_states").await, 1);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM redactions").await, 0);
    // Another meeting's recording doesn't block this one
    f.env.recording_meetings = vec!["m2".into()];
    strike_screens(f.db.pool(), &f.env, "m1", &[a], None).await.unwrap();
}

#[tokio::test]
async fn screen_purge_removes_edit_history_of_its_text() {
    let f = setup().await;
    let a = add_screen(&f, "a", 10).await;
    let b = add_screen(&f, "b", 20).await;
    let snap_rowid: String = sqlx::query_scalar("SELECT CAST(rowid AS TEXT) FROM text_snapshots WHERE snapshot_id = ?")
        .bind(format!("snap-{}", a))
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    for (t, id) in [
        ("text_snapshot", format!("snap-{}", a)),
        ("text_snapshot", snap_rowid),
        ("episode", format!("ep-{}", a)),
        ("text_snapshot", format!("snap-{}", b)),
    ] {
        sqlx::query("INSERT INTO data_versions (entity_type, entity_id, field_name, previous_value, new_value) VALUES (?, ?, 'content', 'wire 4471', 'x')")
            .bind(t)
            .bind(id)
            .execute(f.db.pool())
            .await
            .unwrap();
    }
    strike_screens(f.db.pool(), &f.env, "m1", &[a], None).await.unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT entity_id FROM data_versions").fetch_all(f.db.pool()).await.unwrap();
    assert_eq!(left, vec![format!("snap-{}", b)], "only the unselected screen's history is kept");
}

// ─── Backups and logs ────────────────────────────────────────────────────

#[tokio::test]
async fn strike_purges_app_backups_and_deletes_unreadable_ones() {
    let f = setup().await;
    let secret = "quokkasecret";
    let text = format!("remember {} please", secret);
    let id = add_line(&f, &text).await;
    wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    let bdir = f.env.app_data_dir.join("backups").join("pre-migration-x");
    std::fs::create_dir_all(&bdir).unwrap();
    let good = bdir.join("copy.db");
    std::fs::copy(&f.db_path, &good).unwrap();
    let junk = bdir.join("broken.db");
    let mut bytes = b"SQLite format 3\0".to_vec();
    bytes.extend(std::iter::repeat(7u8).take(4096));
    std::fs::write(&junk, &bytes).unwrap();
    let other = bdir.join("notes.txt");
    std::fs::write(&other, "not a database").unwrap();

    let out = strike_words(f.db.pool(), &f.env, &target(id, span(&text, secret)), None).await.unwrap();
    assert_eq!(out.backups_purged, 1);
    assert_eq!(out.backups_deleted, 1);
    assert!(good.exists());
    assert!(!junk.exists());
    assert!(other.exists());
    let b = std::fs::read(&good).unwrap();
    assert!(!b.windows(secret.len()).any(|w| w == secret.as_bytes()), "backup bytes still hold the words");
    // The purged backup still opens and has the edited line
    let mut c = SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(&good)).await.unwrap();
    let t: String = sqlx::query_scalar("SELECT text FROM transcripts WHERE id = ?").bind(id).fetch_one(&mut c).await.unwrap();
    assert_eq!(render_plain(&t), "remember [stricken from the record] please");
}

#[tokio::test]
async fn strike_redacts_app_logs() {
    let f = setup().await;
    let logs = f.env.app_data_dir.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(logs.join("app.log"), "INFO 📝 Whisper [1.0s]: call Marguerite at home\nINFO other\n").unwrap();
    let text = "call Marguerite at home";
    let id = add_line(&f, text).await;
    strike_words(f.db.pool(), &f.env, &target(id, span(text, "Marguerite")), None).await.unwrap();
    let log = std::fs::read_to_string(logs.join("app.log")).unwrap();
    assert!(!log.contains("Marguerite"));
    assert!(log.contains("call [stricken from the record] at home"));
}

// ─── Screen video (DMG) ──────────────────────────────────────────────────

/// Mean luma of one frame at `t` seconds
#[cfg(not(feature = "mas"))]
fn luma_at(path: &Path, t: f64) -> Option<f64> {
    let ffmpeg = crate::video_recorder::find_tool("ffmpeg")?;
    let out = std::process::Command::new(ffmpeg)
        .args(["-v", "error", "-ss", &format!("{:.2}", t), "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
        .output()
        .ok()?;
    if out.stdout.is_empty() {
        return None;
    }
    Some(out.stdout.iter().map(|&b| b as f64).sum::<f64>() / out.stdout.len() as f64)
}

#[cfg(not(feature = "mas"))]
#[tokio::test]
async fn screen_strike_blanks_the_screen_video_range() {
    let Some(ffmpeg) = crate::video_recorder::find_tool("ffmpeg") else {
        eprintln!("ffmpeg not installed; skipping video blanking test");
        return;
    };
    if crate::video_recorder::find_tool("ffprobe").is_none() {
        return;
    }
    let mut f = setup().await;
    f.env.video_enabled = true;
    let vdir = video_blank::video_dir(&f.env, "m1");
    std::fs::create_dir_all(&vdir).unwrap();
    let chunk = vdir.join("chunk_001.mov");
    let ok = std::process::Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=white:s=64x64:d=8:r=10", "-pix_fmt", "yuv420p", "-c:v", "mpeg4", "-q:v", "2"])
        .arg(&chunk)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("ffmpeg can't encode a test clip here; skipping");
        return;
    }
    let chunk_start = Utc::now() - chrono::Duration::seconds(60);
    video_blank::set_chunk_start(&vdir, &chunk, chunk_start).unwrap();
    // A screen at chunk +4s
    let state_id = "vid-state".to_string();
    let ts = chunk_start + chrono::Duration::seconds(4);
    f.db.add_screen_state(&state_id, "m1", ts, Some(ts), "", 0.0, None, "other", "{}").await.unwrap();

    let before = luma_at(&chunk, 4.5).unwrap();
    assert!(before > 200.0);
    let out = strike_screens(f.db.pool(), &f.env, "m1", &[state_id], None).await.unwrap();
    // The database content is gone at once; the video is queued
    assert_eq!(out.video_jobs_queued, 1);
    assert!(out.record.as_ref().unwrap().video_pending, "the marker says the video is pending");
    assert_eq!(count(&f, "SELECT COUNT(*) FROM screen_states").await, 0);
    assert!(luma_at(&chunk, 4.5).unwrap() > 200.0, "not blanked inline (outside the action)");
    let tl = f.db.get_synced_timeline("m1").await.unwrap().unwrap();
    assert!(tl.redactions[0].video_pending);
    // The background job blanks it (mpeg4 source: whole-chunk fallback)
    let s = run_jobs(&f).await;
    assert_eq!(s.jobs_done, 1, "{:?} {:?}", s, video_jobs::list(f.db.pool(), None).await);
    assert!(luma_at(&chunk, 4.5).unwrap() < 30.0, "covered moment is black");
    let tl = f.db.get_synced_timeline("m1").await.unwrap().unwrap();
    assert!(!tl.redactions[0].video_pending, "marker no longer pending");
    assert!(luma_at(&chunk, 0.5).unwrap() > 200.0, "outside the range is untouched");
    assert!(luma_at(&chunk, 7.5).unwrap() > 200.0);
    let dur = video_blank::probe_duration(&chunk).unwrap();
    assert!((dur - 8.0).abs() < 0.6, "duration unchanged: {}", dur);
}

/// Run the screen video jobs due now with real ffmpeg (DMG).
#[cfg(not(feature = "mas"))]
async fn run_jobs(f: &Fixture) -> video_jobs::RunSummary {
    video_jobs::run_due(
        f.db.pool(),
        &f.env,
        std::sync::Arc::new(video_blank::FfmpegOps),
        Utc::now(),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::sync::Arc::new(|_| {}),
    )
    .await
    .unwrap()
}

/// The screen video can't be read: the screens are still removed from the
/// database at once (never left behind because of the video), and the
/// video job fails loudly, stays queued and is retried with backoff.
#[cfg(not(feature = "mas"))]
#[tokio::test]
async fn unreadable_video_still_purges_screens_and_keeps_a_failed_video_job() {
    let mut f = setup().await;
    f.env.video_enabled = true;
    let vdir = video_blank::video_dir(&f.env, "m1");
    std::fs::create_dir_all(&vdir).unwrap();
    // Not a real video: ffprobe can't read its length
    std::fs::write(vdir.join("chunk_001.mov"), b"garbage").unwrap();
    let a = add_screen(&f, "a", 10).await;
    let out = strike_screens(f.db.pool(), &f.env, "m1", &[a.clone()], None).await.unwrap();
    assert_eq!(out.video_jobs_queued, 1);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM screen_states").await, 0);
    assert!(!f.env.app_data_dir.join("frames/m1").join(format!("state_{}.jpg", a)).exists());
    if crate::video_recorder::find_tool("ffprobe").is_none() {
        return;
    }
    let s = run_jobs(&f).await;
    assert_eq!(s.meetings_failed, 1);
    let jobs = video_jobs::list(f.db.pool(), Some("m1")).await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].status, "failed");
    assert!(jobs[0].last_error.as_deref().map_or(false, |e| !e.is_empty()));
    assert!(jobs[0].next_attempt_at.is_some(), "retried later, with backoff");
    // Not due again immediately: no tight retry loop
    assert_eq!(run_jobs(&f).await, video_jobs::RunSummary::default());
    assert_eq!(std::fs::read(vdir.join("chunk_001.mov")).unwrap(), b"garbage", "left as it was");
}

// ─── Audio ───────────────────────────────────────────────────────────────

/// The Mac app never writes meeting audio to disk (transcription is
/// streamed; screen video is recorded with no audio track), so there is no
/// recording to silence. This guards that assumption: if a recorder starts
/// saving audio, the silence step in docs/REDACTION.md must be built.
#[test]
fn mac_stores_no_meeting_audio() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src];
    let mut offenders = Vec::new();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().map_or(false, |x| x == "rs") && !p.ends_with("redaction/tests.rs") {
                let s = std::fs::read_to_string(&p).unwrap();
                for pat in ["WavWriter", "hound::", ".wav\"", ".m4a\"", ".caf\"", "AVAudioFile"] {
                    if s.contains(pat) {
                        offenders.push(format!("{}: {}", p.display(), pat));
                    }
                }
            }
        }
    }
    assert!(offenders.is_empty(), "audio is written to disk; implement the silence step: {:?}", offenders);
}

mod range_tests;
mod video_tests;
