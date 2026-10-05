//! Time-range Delete / Strike (docs/REDACTION.md "Time ranges").

use super::*;
use crate::redaction::time_range::{self, line_cut, LineCut, RangeCounts};

fn at(t0: DateTime<Utc>, ms: i64) -> DateTime<Utc> {
    t0 + chrono::Duration::milliseconds(ms)
}

fn timings(words: &[(usize, usize, i64, i64)]) -> String {
    serde_json::to_string(
        &words.iter().map(|&(s, e, t0, t1)| WordTiming { s, e, t0, t1 }).collect::<Vec<_>>(),
    )
    .unwrap()
}

// ─── Pure: which part of a line a range removes ──────────────────────────

#[test]
fn line_cut_splits_by_word_timings() {
    let t0 = Utc::now();
    let text = "delta epsilon zeta eta";
    let tj = timings(&[(0, 5, 0, 500), (6, 13, 600, 1100), (14, 18, 2100, 2600), (19, 22, 2700, 3200)]);
    // Range starts at +2.0 s: zeta and eta go
    assert_eq!(
        line_cut(text, Some(&tj), t0, None, at(t0, 2000), at(t0, 60_000)),
        LineCut::Words { start: 14, end: 22 }
    );
    // A word goes only if the middle of its time is inside (epsilon's middle is 850 ms)
    assert_eq!(
        line_cut(text, Some(&tj), t0, None, at(t0, 900), at(t0, 60_000)),
        LineCut::Words { start: 14, end: 22 }
    );
    assert_eq!(
        line_cut(text, Some(&tj), t0, None, at(t0, 800), at(t0, 60_000)),
        LineCut::Words { start: 6, end: 22 }
    );
    // Range ends inside the line: the head goes
    assert_eq!(
        line_cut(text, Some(&tj), t0, None, at(t0, -5000), at(t0, 1000)),
        LineCut::Words { start: 0, end: 13 }
    );
    assert_eq!(line_cut(text, Some(&tj), t0, None, at(t0, -5000), at(t0, 5000)), LineCut::Whole { estimated: false });
    assert_eq!(line_cut(text, Some(&tj), t0, None, at(t0, 4000), at(t0, 9000)), LineCut::Outside);
}

#[test]
fn line_cut_without_timings_needs_half_the_line_inside() {
    let t0 = Utc::now();
    let text = "one two three four five"; // 5 words → estimated 2.0 s
    assert_eq!(line_cut(text, None, t0, None, at(t0, 900), at(t0, 9000)), LineCut::Whole { estimated: true });
    assert_eq!(line_cut(text, None, t0, None, at(t0, 1000), at(t0, 9000)), LineCut::Whole { estimated: true });
    assert_eq!(line_cut(text, None, t0, None, at(t0, 1100), at(t0, 9000)), LineCut::KeptEstimate);
    assert_eq!(line_cut(text, None, t0, None, at(t0, -9000), at(t0, 1000)), LineCut::Whole { estimated: true });
    assert_eq!(line_cut(text, None, t0, None, at(t0, -9000), at(t0, 900)), LineCut::KeptEstimate);
    assert_eq!(line_cut(text, None, t0, None, at(t0, 3000), at(t0, 9000)), LineCut::Outside);
    // The next line bounds the estimate (here to 1 s)
    assert_eq!(
        line_cut(text, None, t0, Some(at(t0, 1000)), at(t0, 600), at(t0, 9000)),
        LineCut::KeptEstimate
    );
    assert_eq!(
        line_cut(text, None, t0, Some(at(t0, 1000)), at(t0, 500), at(t0, 9000)),
        LineCut::Whole { estimated: true }
    );
    // Garbage timings fall back to the estimate
    assert_eq!(line_cut(text, Some("not json"), t0, None, at(t0, 0), at(t0, 9000)), LineCut::Whole { estimated: true });
}

// ─── Fixture: a meeting with lines, screens and loose screen text ────────

struct Scene {
    t0: DateTime<Utc>,
    l1: i64,
    l2: i64,
    l3: i64,
    l4: i64,
    l5: i64,
    l6: i64,
    s_before: String,
    s_in1: String,
    s_in2: String,
    s_after: String,
}

/// Range under test: [+60 s, +119 s] from the meeting start.
const R0: i64 = 60_000;
const R1: i64 = 119_000;

async fn scene(f: &Fixture) -> Scene {
    let t0: DateTime<Utc> = parse_ts(
        &sqlx::query_scalar::<_, String>("SELECT started_at FROM meetings WHERE id = 'm1'")
            .fetch_one(f.db.pool())
            .await
            .unwrap(),
    )
    .unwrap();
    let line = |text: &'static str, ms: i64, tj: Option<String>| {
        let db = f.db.clone();
        async move {
            db.add_transcript_full("m1", text, Some("Alice"), true, 0.9, at(t0, ms), tj.as_deref()).await.unwrap()
        }
    };
    let l1 = line("alpha beta gamma", 10_000, Some(timings(&[(0, 5, 0, 400), (6, 10, 500, 900), (11, 16, 1000, 1400)]))).await;
    // Starts before the range; zeta + eta are inside it
    let l2 = line(
        "delta epsilon zeta eta",
        58_000,
        Some(timings(&[(0, 5, 0, 500), (6, 13, 600, 1100), (14, 18, 2100, 2600), (19, 22, 2700, 3200)])),
    )
    .await;
    let l3 = line("inside quokka fully here", 70_000, Some(timings(&[(0, 6, 0, 300), (7, 13, 400, 800), (14, 19, 900, 1200), (20, 24, 1300, 1600)]))).await;
    // No word timings: 4 words ≈ 1.6 s, bounded by the next line at +118.8 s
    let l4 = line("estimated mostly inside line", 118_000, None).await;
    // No timings: 6 words ≈ 2.4 s from +118.8 s, only 0.2 s inside: kept
    let l5 = line("estimated barely inside so kept", 118_800, None).await;
    let l6 = line("after the range entirely", 130_000, None).await;
    let s_before = add_screen_at(f, "before", at(t0, 30_000)).await;
    let s_in1 = add_screen_at(f, "in1", at(t0, 65_000)).await;
    let s_in2 = add_screen_at(f, "in2", at(t0, 100_000)).await;
    let s_after = add_screen_at(f, "after", at(t0, 130_000)).await;
    // Screen text no screen owns, an AI activity summary and a timeline
    // entry, all captured inside the range
    f.db.add_text_snapshot_full("loose", None, None, Some("m1"), at(t0, 90_000), "loose wombat text", "h", 0.9, "accessibility", None, None)
        .await
        .unwrap();
    let activity = crate::database::ActivityLogEntry {
        id: None,
        start_time: at(t0, 95_000),
        end_time: None,
        duration_seconds: None,
        app_name: Some("Notes".into()),
        window_title: None,
        category: "other".into(),
        summary: "User edits the wombat plan".into(),
        focus_area: None,
        visible_files: None,
        confidence: Some(0.5),
        frame_ids: None,
    };
    f.db.add_activity(&activity).await.unwrap();
    sqlx::query("INSERT INTO meeting_timeline_events (event_id, meeting_id, ts, event_type, title) VALUES ('ev-loose', 'm1', ?, 'app', 'Wombat plan')")
        .bind(at(t0, 97_000).to_rfc3339())
        .execute(f.db.pool())
        .await
        .unwrap();
    Scene { t0, l1, l2, l3, l4, l5, l6, s_before, s_in1, s_in2, s_after }
}

async fn screen_left(f: &Fixture, id: &str) -> bool {
    count(f, &format!("SELECT COUNT(*) FROM screen_states WHERE state_id = '{}'", id)).await == 1
}

#[tokio::test]
async fn preview_counts_exactly_what_a_range_delete_removes() {
    let f = setup().await;
    let sc = scene(&f).await;
    let p = time_range::preview_time_range(f.db.pool(), &f.env, "m1", R0, R1).await.unwrap();
    assert_eq!(p.counts, RangeCounts { screens: 2, lines_whole: 2, lines_split: 1 });
    assert_eq!(p.words_removed, 2 + 4 + 4);
    assert_eq!(p.estimated_included, 1);
    assert_eq!(p.estimated_excluded, 1);
    assert_eq!(p.screen_text_snapshots, 1, "only the loose one; screens' own text goes with them");
    assert_eq!(p.activity_summaries, 1);
    assert_eq!(p.timeline_entries, 1);
    assert_eq!(p.start_ms, R0);
    assert_eq!(p.end_ms, R1);
    let mut ids = p.screen_ids.clone();
    ids.sort();
    let mut want = vec![sc.s_in1.clone(), sc.s_in2.clone()];
    want.sort();
    assert_eq!(ids, want);
    assert!(p.items.iter().any(|i| i.contains("at least half")), "{:?}", p.items);
    assert!(p.items.iter().any(|i| i.contains("less than half")), "{:?}", p.items);
    // Preview changes nothing
    assert_eq!(count(&f, "SELECT COUNT(*) FROM transcripts").await, 6);

    // The delete refuses if the range no longer matches what was shown
    let stale = RangeCounts { screens: 3, ..p.counts };
    assert!(time_range::request_delete_time_range(f.db.pool(), &f.env, "m1", R0, R1, Some(stale)).await.is_err());
    assert_eq!(count(&f, "SELECT COUNT(*) FROM redactions").await, 0);

    let pending = time_range::request_delete_time_range(f.db.pool(), &f.env, "m1", R0, R1, Some(p.counts)).await.unwrap();
    // Nothing removed during the undo window
    assert_eq!(count(&f, "SELECT COUNT(*) FROM transcripts").await, 6);
    assert!(screen_left(&f, &sc.s_in1).await);
    commit_pending(f.db.pool(), &f.env, &pending.id).await.unwrap().unwrap();

    assert_eq!(line_text(&f, sc.l1).await.unwrap(), "alpha beta gamma");
    assert_eq!(line_text(&f, sc.l2).await.unwrap(), "delta epsilon", "split by word timings");
    let tj: String = sqlx::query_scalar("SELECT word_timings FROM transcripts WHERE id = ?")
        .bind(sc.l2)
        .fetch_one(f.db.pool())
        .await
        .unwrap();
    assert_eq!(serde_json::from_str::<Vec<WordTiming>>(&tj).unwrap().len(), 2);
    assert_eq!(line_text(&f, sc.l3).await, None);
    assert_eq!(line_text(&f, sc.l4).await, None, "≥ 50% inside: included");
    assert_eq!(line_text(&f, sc.l5).await.unwrap(), "estimated barely inside so kept");
    assert_eq!(line_text(&f, sc.l6).await.unwrap(), "after the range entirely");
    assert!(screen_left(&f, &sc.s_before).await);
    assert!(!screen_left(&f, &sc.s_in1).await);
    assert!(!screen_left(&f, &sc.s_in2).await);
    assert!(screen_left(&f, &sc.s_after).await);
    assert_eq!(fts_hits(&f, "zeta").await, 0);
    assert_eq!(fts_hits(&f, "quokka").await, 0);
    assert_eq!(content_anywhere(&f, "wombat").await, Vec::<String>::new(), "loose screen text, activity, timeline");
    assert_eq!(count(&f, "SELECT COUNT(*) FROM redactions").await, 0, "Delete leaves no trace");
    // Files of removed screens are gone, others kept
    let frames = f.env.app_data_dir.join("frames/m1");
    assert!(!frames.join(format!("state_{}.jpg", sc.s_in1)).exists());
    assert!(frames.join(format!("state_{}.jpg", sc.s_before)).exists());
    let _ = sc.t0;
}

#[tokio::test]
async fn range_delete_has_undo_and_is_dropped_if_the_range_changed() {
    let f = setup().await;
    let sc = scene(&f).await;
    let p = time_range::request_delete_time_range(f.db.pool(), &f.env, "m1", R0, R1, None).await.unwrap();
    undo_delete(f.db.pool(), &p.id).await.unwrap();
    assert_eq!(count(&f, "SELECT COUNT(*) FROM transcripts").await, 6);
    assert_eq!(count(&f, "SELECT COUNT(*) FROM redactions").await, 0);

    let p = time_range::request_delete_time_range(f.db.pool(), &f.env, "m1", R0, R1, None).await.unwrap();
    sqlx::query("UPDATE transcripts SET text = 'someone edited this' WHERE id = ?")
        .bind(sc.l3)
        .execute(f.db.pool())
        .await
        .unwrap();
    let e = commit_pending(f.db.pool(), &f.env, &p.id).await.unwrap_err();
    assert!(e.contains("changed"), "{}", e);
    assert!(!is_pending(f.db.pool(), &p.id).await, "can never apply: dropped");
    assert!(screen_left(&f, &sc.s_in1).await, "nothing applied");
}

#[tokio::test]
async fn range_strike_leaves_one_marker_in_the_transcript_and_one_in_the_screens() {
    let f = setup().await;
    let sc = scene(&f).await;
    let out = time_range::strike_time_range(f.db.pool(), &f.env, "m1", R0, R1, Some("privileged"), None)
        .await
        .unwrap();
    assert_eq!(out.records.len(), 2);
    let line_rec = out.records.iter().find(|r| r.kind != "screen").unwrap();
    let screen_rec = out.records.iter().find(|r| r.kind == "screen").unwrap();
    assert_eq!(line_rec.kind, "words", "one line was split");
    assert_eq!(line_rec.item_count, 3);
    assert_eq!(line_rec.transcript_id, Some(sc.l2));
    assert_eq!(screen_rec.item_count, 2);
    for r in &out.records {
        assert_eq!(r.reason.as_deref(), Some("privileged"));
        assert_eq!(r.media_start.as_deref().and_then(parse_ts), Some(at(sc.t0, R0)));
        assert_eq!(r.media_end.as_deref().and_then(parse_ts), Some(at(sc.t0, R1)));
    }
    // One marker for the span, where it starts; the rest closes up
    assert_eq!(line_text(&f, sc.l2).await.unwrap(), format!("delta epsilon {}", marker_token(&line_rec.id)));
    assert_eq!(line_text(&f, sc.l3).await, None);
    assert_eq!(line_text(&f, sc.l4).await, None);
    assert!(line_text(&f, sc.l5).await.is_some());
    for needle in ["zeta", "quokka", "wombat", "4471"] {
        if needle == "4471" {
            // The screens outside the range still hold their (test) text
            continue;
        }
        assert_eq!(content_anywhere(&f, needle).await, Vec::<String>::new(), "{}", needle);
    }
    let tl = f.db.get_synced_timeline("m1").await.unwrap().unwrap();
    assert_eq!(tl.redactions.len(), 2);
    assert_eq!(tl.frames.len(), 2, "only the screens outside the range are left");
    // Strikes can't be undone
    assert!(undo_delete(f.db.pool(), &line_rec.id).await.is_err());
    // A reason quoting the removed words is refused (nothing changes)
    let f2 = setup().await;
    let _ = scene(&f2).await;
    let e = time_range::strike_time_range(f2.db.pool(), &f2.env, "m1", R0, R1, Some("about quokka"), None)
        .await
        .unwrap_err();
    assert!(e.contains("reason"), "{}", e);
    assert_eq!(count(&f2, "SELECT COUNT(*) FROM transcripts").await, 6);
}

#[tokio::test]
async fn empty_or_backwards_ranges_are_refused() {
    let f = setup().await;
    let _ = scene(&f).await;
    assert!(time_range::preview_time_range(f.db.pool(), &f.env, "m1", R1, R0).await.is_err());
    let p = time_range::preview_time_range(f.db.pool(), &f.env, "m1", 200_000, 300_000).await.unwrap();
    assert!(p.nothing);
    assert!(time_range::request_delete_time_range(f.db.pool(), &f.env, "m1", 200_000, 300_000, None).await.is_err());
    // Recording meetings can't be edited
    let rec = RedactionEnv { recording_meetings: vec!["m1".into()], ..f.env.clone() };
    assert_eq!(
        time_range::request_delete_time_range(f.db.pool(), &rec, "m1", R0, R1, None).await.unwrap_err(),
        RECORDING_SCREENS_ERROR
    );
}

#[tokio::test]
async fn range_delete_flushes_overlapping_pending_deletes_first() {
    let f = setup().await;
    let sc = scene(&f).await;
    // A word delete on a line the range also touches is still in its undo window
    let p1 = request_delete_words(f.db.pool(), &f.env, &target(sc.l2, (0, 5))).await.unwrap();
    let p2 = time_range::request_delete_time_range(f.db.pool(), &f.env, "m1", R0, R1, None).await.unwrap();
    assert!(!is_pending(f.db.pool(), &p1.id).await, "committed first");
    commit_pending(f.db.pool(), &f.env, &p2.id).await.unwrap().unwrap();
    assert_eq!(line_text(&f, sc.l2).await.unwrap(), "epsilon");
}
