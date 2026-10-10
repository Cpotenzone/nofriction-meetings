//! Tests for docs/SYNC.md on the Mac: protocol, auth, merge rules,
//! tombstones through the purge path, a loopback session over real TLS, and
//! the golden fixtures the iOS tests decode byte for byte.

use super::merge::{self, KeepTok};
use super::pairing::{self, Identity, PendingPair};
use super::protocol::{self as p, Item, Msg, Phase};
use super::server::{self, Server};
use super::store;
use crate::database::DatabaseManager;
use crate::redaction::RedactionEnv;
use parking_lot::Mutex;
use sqlx::Row;
use std::path::PathBuf;
use std::sync::Arc;

const SECRET: [u8; 32] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
];
const REC: &str = "7c1e1b2a-3f4d-4e5f-8a9b-0c1d2e3f4a5b";
const PHONE: &str = "0b9a8c7d-6e5f-4a3b-9c2d-1e0f2a3b4c5d";

struct Fx {
    db: Arc<DatabaseManager>,
    dir: PathBuf,
    env: RedactionEnv,
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Fx {
    fn pool(&self) -> &sqlx::Pool<sqlx::Sqlite> {
        self.db.pool()
    }
}

async fn setup() -> Fx {
    let dir = std::env::temp_dir().join(format!("nf-sync-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db = DatabaseManager::new(&data.join("nofriction_meetings.db")).await.unwrap();
    db.run_migrations().await.unwrap();
    let env = RedactionEnv { app_data_dir: data, cache_dir: dir.join("cache"), video_enabled: false, recording_meetings: Vec::new() };
    Fx { db: Arc::new(db), dir, env }
}

fn tk() -> [u8; 32] {
    p::token_key(&SECRET)
}

async fn line_texts(f: &Fx, meeting: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT text FROM transcripts WHERE meeting_id = ? ORDER BY timestamp, id")
        .bind(meeting)
        .fetch_all(f.pool())
        .await
        .unwrap()
}

async fn sync_id_of(f: &Fx, tid: i64) -> String {
    sqlx::query_scalar("SELECT sync_id FROM transcripts WHERE id = ?").bind(tid).fetch_one(f.pool()).await.unwrap()
}

fn rec_item(id: &str, title: &str, modified: i64) -> p::RecordingItem {
    p::RecordingItem {
        id: id.into(),
        title: title.into(),
        started: 1_760_000_000_000,
        ended: Some(1_760_000_600_000),
        kind: "class".into(),
        notebook: Some("BIO 101".into()),
        planned: Some(50),
        cal: Some(p::Calendar { event: Some("evt-1".into()), location: Some("Room 4".into()), ..Default::default() }),
        people: vec![p::PersonEntry { email: "ana@example.com".into(), name: Some("Ana".into()), role: "organizer".into() }],
        modified,
    }
}

fn line_item(id: &str, text: &str, at: i64) -> p::LineItem {
    p::LineItem { id: id.into(), rec: REC.into(), text: text.into(), at, dur: Some(3000), speaker: None, src: None }
}

fn uid(n: u8) -> String {
    format!("00000000-0000-4000-8000-0000000000{:02x}", n)
}

// ═══════════════════════════════════════════════════════════════════════════
// Protocol
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn encode_is_canonical_and_round_trips() {
    let msg = Msg::Batch {
        phase: Phase::Changes,
        items: vec![Item::Line(line_item(&uid(1), "Hello \"there\"\nnext / line ⟦é⟧", 5))],
        last: true,
        upto: Some(42),
    };
    let bytes = p::encode(&msg);
    let s = String::from_utf8(bytes.clone()).unwrap();
    assert!(s.starts_with("{\"items\":[{\"at\":5,\"dur\":3000,\"id\":"), "{}", s);
    assert!(s.ends_with("\"last\":true,\"phase\":\"changes\",\"t\":\"batch\",\"upto\":42,\"v\":1}"), "{}", s);
    assert!(!s.contains("speaker"), "absent optionals are omitted");
    assert!(s.contains("next / line"), "slashes aren't escaped");
    assert_eq!(p::decode(&bytes).unwrap(), msg);
}

#[test]
fn decode_rejects_other_versions_and_garbage() {
    assert_eq!(p::decode(br#"{"t":"done","v":2}"#), Err(p::DecodeError::Version(2)));
    assert!(matches!(p::decode(br#"{"t":"nope","v":1}"#), Err(p::DecodeError::Json(_))));
    assert!(matches!(p::decode(b"not json"), Err(p::DecodeError::Json(_))));
}

#[tokio::test]
async fn framing_round_trips_and_caps_size() {
    let (mut a, mut b) = tokio::io::duplex(1 << 20);
    p::write_msg(&mut a, &Msg::Done {}).await.unwrap();
    assert_eq!(p::read_msg(&mut b).await.unwrap(), Msg::Done {});
    use tokio::io::AsyncWriteExt;
    a.write_all(&((p::MAX_FRAME as u32) + 1).to_be_bytes()).await.unwrap();
    assert!(p::read_msg(&mut b).await.is_err());
}

#[test]
fn ids_and_markers_convert_both_ways() {
    let simple = "0123456789abcdef0123456789abcdef";
    let wire = p::wire_id(simple).unwrap();
    assert_eq!(wire, "01234567-89ab-cdef-0123-456789abcdef");
    assert_eq!(p::simple_id(&wire.to_uppercase()).unwrap(), simple);
    assert_eq!(p::wire_id("not-an-id"), None);
    let mac = format!("one {} two", crate::redaction::marker_token(simple));
    let w = p::text_to_wire(&mac);
    assert_eq!(w, format!("one ⟦stricken:{}⟧ two", wire));
    assert_eq!(p::text_from_wire(&w), mac);
    // iPhone's uppercase UUID spelling is accepted
    assert_eq!(p::text_from_wire(&format!("⟦stricken:{}⟧", wire.to_uppercase())), crate::redaction::marker_token(simple));
}

#[test]
fn batches_split_at_500_and_always_send_one() {
    assert_eq!(p::batches(vec![]).len(), 1);
    let items: Vec<Item> = (0..1001).map(|i| Item::Gone(p::GoneItem { entity: "mark".into(), id: format!("{}", i), rec: None })).collect();
    let b = p::batches(items);
    assert_eq!(b.iter().map(|x| x.len()).collect::<Vec<_>>(), vec![500, 500, 1]);
}

#[test]
fn tokens_split_on_unicode_whitespace_and_find_markers() {
    let m = uid(9);
    let text = format!("a\u{00A0}b  ⟦stricken:{}⟧\tc\u{3000}dé", m);
    let toks = p::tokens(&text);
    let kinds: Vec<String> = toks
        .iter()
        .map(|t| match t {
            p::Tok::Word { text, .. } => text.clone(),
            p::Tok::Marker { id, .. } => format!("M:{}", id),
        })
        .collect();
    assert_eq!(kinds, vec!["a".to_string(), "b".into(), format!("M:{}", m), "c".into(), "dé".into()]);
    // UTF-16 offsets
    assert_eq!((toks[4].start16(), toks[4].end16()), (text.encode_utf16().count() - 2, text.encode_utf16().count()));
}

// ═══════════════════════════════════════════════════════════════════════════
// Auth and pairing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn proofs_differ_by_role_and_secret() {
    let (np, nm) = ([1u8; 32], [2u8; 32]);
    let a = p::mac_proof(&SECRET, &np, &nm);
    assert_ne!(a, p::phone_proof(&SECRET, &nm, &np));
    assert_ne!(a, p::mac_proof(&[9u8; 32], &np, &nm));
    assert!(p::ct_eq(&a, &p::mac_proof(&SECRET, &np, &nm)));
    assert!(!p::ct_eq(&a, &a[..31]));
}

#[test]
fn pairing_codes_are_one_use_expire_and_lock_after_five_misses() {
    let now = std::time::Instant::now();
    let mut slot = Some(PendingPair::new());
    let code = slot.as_ref().unwrap().code.clone();
    assert_eq!(code.len(), 10);
    assert_eq!(pairing::check_code(&mut slot, &code.to_lowercase(), now), pairing::CodeCheck::Ok);
    assert!(slot.is_none(), "one use");
    assert_eq!(pairing::check_code(&mut slot, &code, now), pairing::CodeCheck::Expired);

    let mut slot = Some(PendingPair::new());
    let code = slot.as_ref().unwrap().code.clone();
    for _ in 0..4 {
        assert_eq!(pairing::check_code(&mut slot, "WRONGWRONG", now), pairing::CodeCheck::Wrong);
    }
    assert!(slot.is_some());
    assert_eq!(pairing::check_code(&mut slot, "WRONGWRONG", now), pairing::CodeCheck::Wrong);
    assert!(slot.is_none(), "thrown away after 5 wrong attempts");
    assert_eq!(pairing::check_code(&mut slot, &code, now), pairing::CodeCheck::Expired);

    let mut slot = Some(PendingPair::new());
    let code = slot.as_ref().unwrap().code.clone();
    let later = now + pairing::CODE_TTL + std::time::Duration::from_secs(1);
    assert_eq!(pairing::check_code(&mut slot, &code, later), pairing::CodeCheck::Expired);
}

#[test]
fn pairing_link_and_qr() {
    let link = pairing::pairing_link(REC, "Casey's Mac", &"ab".repeat(32), &["192.168.1.5".into()], 5000, "ABCDEFGHJK");
    assert!(link.starts_with("nfsync:1?id="));
    assert!(link.contains("&n=Casey%27s%20Mac&"));
    assert!(link.contains("&p=5000&c=ABCDEFGHJK"));
    let svg = pairing::qr_svg(&link).unwrap();
    assert!(svg.contains("<svg") && svg.contains("</svg>"));
}

#[test]
fn identity_fingerprint_is_sha256_of_cert() {
    let id = Identity::generate().unwrap();
    assert_eq!(id.fingerprint().len(), 64);
    assert!(id.server_config().is_ok());
}

#[test]
fn local_addresses_only() {
    for ok in ["127.0.0.1", "10.0.0.2", "192.168.1.9", "172.16.0.1", "169.254.1.1", "100.100.1.1", "::1", "fe80::1", "fd00::5", "::ffff:192.168.0.2"] {
        assert!(server::is_local_addr(&ok.parse().unwrap()), "{}", ok);
    }
    for bad in ["8.8.8.8", "100.128.0.1", "2001:4860::1", "::ffff:8.8.8.8"] {
        assert!(!server::is_local_addr(&bad.parse().unwrap()), "{}", bad);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Merge of line edits (pure)
// ═══════════════════════════════════════════════════════════════════════════

/// (receiver's text, sender's text, expected receiver text after merge)
fn merge_cases() -> Vec<(&'static str, String, String, String)> {
    let m1 = uid(0xa1);
    let m2 = uid(0xa2);
    let mk = |id: &str| format!("⟦stricken:{}⟧", id);
    vec![
        ("delete in the middle", "we acquire Zenith Labs next quarter".into(), "we acquire next quarter".into(), "we acquire next quarter".into()),
        (
            "strike with marker",
            "we acquire Zenith Labs next quarter".into(),
            format!("we acquire {} next quarter", mk(&m1)),
            format!("we acquire {} next quarter", mk(&m1)),
        ),
        (
            "both sides removed different words: union",
            "one two three five".into(),  // the receiver deleted "four"
            "one three four five".into(), // the sender deleted "two"
            "one three five".into(),
        ),
        (
            "receiver's own strike stays",
            format!("alpha {} gamma delta", mk(&m2)),
            "alpha beta gamma".into(),
            format!("alpha {} gamma", mk(&m2)),
        ),
        (
            "marker for words already gone here",
            "alpha gamma".into(),
            format!("alpha {} gamma", mk(&m1)),
            format!("alpha {} gamma", mk(&m1)),
        ),
        ("whole line struck", "secret plan here".into(), mk(&m1), mk(&m1)),
        ("repeated words", "yes yes no yes".into(), "yes no yes".into(), "yes no yes".into()),
        ("unicode", "café ünïcödé 日本 語".into(), "café 語".into(), "café 語".into()),
        ("nothing changed", "same words here".into(), "same words here".into(), "same words here".into()),
        (
            "two strikes in one gap",
            "a b c d".into(),
            format!("a {} {} d", mk(&m1), mk(&m2)),
            format!("a {} {} d", mk(&m1), mk(&m2)),
        ),
    ]
}

#[test]
fn merge_cases_hold() {
    let key = tk();
    for (name, local, sender, expected) in merge_cases() {
        let keep = merge::parse_keep(&p::keep_list(&key, &sender)).unwrap();
        let plan = merge::plan(&p::tokens(&local), &keep, &key);
        assert_eq!(merge::apply_to_wire_text(&local, &plan), expected, "case: {}", name);
    }
}

#[test]
fn malformed_keep_is_ignored() {
    assert!(merge::parse_keep(&["w:zz".into()]).is_none());
    assert!(merge::parse_keep(&["x:1".into()]).is_none());
    assert!(merge::parse_keep(&["m:not-a-uuid".into()]).is_none());
    assert_eq!(merge::parse_keep(&[format!("m:{}", uid(1).to_uppercase())]), Some(vec![KeepTok::Marker(uid(1))]));
}

#[test]
fn keep_lists_never_contain_words() {
    let keep = p::keep_list(&tk(), "Zenith Labs acquisition");
    assert_eq!(keep.len(), 3);
    for k in keep {
        assert!(k.starts_with("w:") && k.len() == 18);
        assert!(!k.to_lowercase().contains("zenith"));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Change tracking and outgoing items
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn local_changes_are_collected_once_and_in_order() {
    let f = setup().await;
    f.db.create_meeting(REC, "Biology").await.unwrap();
    f.db.add_transcript(REC, "Cells divide by mitosis", None, true, 0.9).await.unwrap();
    crate::markers::add(f.pool(), REC, chrono::Utc::now(), Some("question"), Some("ask about meiosis"), false).await.unwrap();
    let (items, upto) = store::collect(f.pool(), PHONE, 0, &tk()).await.unwrap();
    let kinds: Vec<u8> = items.iter().map(|i| i.order()).collect();
    assert_eq!(kinds, vec![3, 4, 6], "recording, line, mark: {:?}", items);
    match &items[1] {
        Item::Line(l) => {
            assert_eq!(l.text, "Cells divide by mitosis");
            assert_eq!(l.rec, REC);
        }
        other => panic!("{:?}", other),
    }
    let (again, upto2) = store::collect(f.pool(), PHONE, upto, &tk()).await.unwrap();
    assert!(again.is_empty());
    assert_eq!(upto, upto2);

    // A title change: just the recording
    f.db.update_meeting_title(REC, "Biology 2").await.unwrap();
    let (items, _) = store::collect(f.pool(), PHONE, upto, &tk()).await.unwrap();
    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0], Item::Recording(r) if r.title == "Biology 2"));
}

#[tokio::test]
async fn existing_data_is_backfilled_and_edited_lines_travel_as_edits() {
    let f = setup().await;
    f.db.create_meeting(REC, "Board").await.unwrap();
    let tid = f.db.add_transcript(REC, "We acquire Zenith Labs next quarter", None, true, 0.9).await.unwrap();
    let (_, upto) = store::collect(f.pool(), PHONE, 0, &tk()).await.unwrap();

    let line = "We acquire Zenith Labs next quarter";
    let b = line.find("Zenith Labs").unwrap();
    let target = crate::redaction::WordTarget {
        meeting_id: REC.into(),
        transcript_id: tid,
        start: b,
        end: b + "Zenith Labs".len(),
        expected_text: None,
        whole_line: false,
    };
    let out = crate::redaction::strike_words(f.pool(), &f.env, &target, Some("privileged")).await.unwrap();
    let strike_id = out.record.unwrap().id;
    let (items, _) = store::collect(f.pool(), PHONE, upto, &tk()).await.unwrap();
    // The strike record and an edit; never the struck words
    let encoded = String::from_utf8(p::encode(&Msg::Batch { phase: Phase::Changes, items: items.clone(), last: true, upto: None })).unwrap();
    assert!(!encoded.contains("Zenith"), "struck words never travel: {}", encoded);
    assert!(matches!(&items[0], Item::Strike(s) if s.id == p::wire_id(&strike_id).unwrap() && s.reason.as_deref() == Some("privileged")));
    match &items[1] {
        Item::Edit(e) => {
            assert_eq!(e.keep.len(), 5);
            assert_eq!(e.keep[2], format!("m:{}", p::wire_id(&strike_id).unwrap()));
        }
        other => panic!("{:?}", other),
    }
}

#[tokio::test]
async fn changes_from_a_device_are_not_sent_back_to_it() {
    let f = setup().await;
    let mut screen_line = line_item(&uid(1), "Hello from the phone", 1_760_000_001_000);
    screen_line.src = Some("screen".into());
    let items = vec![Item::Recording(rec_item(REC, "From phone", 10)), Item::Line(screen_line)];
    let r = store::apply(f.pool(), &f.env, PHONE, &tk(), items).await;
    assert_eq!(r.applied, 2, "{:?}", r);
    let (back, _) = store::collect(f.pool(), PHONE, 0, &tk()).await.unwrap();
    assert!(back.is_empty(), "echo: {:?}", back);
    // Another device gets them
    let (other, _) = store::collect(f.pool(), &uid(77), 0, &tk()).await.unwrap();
    assert_eq!(other.len(), 2);
    assert!(matches!(&other[1], Item::Line(l) if l.src.as_deref() == Some("screen")), "the line's source travels on");
}

// ═══════════════════════════════════════════════════════════════════════════
// Applying: merge rules
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn recordings_arrive_with_calendar_and_people_and_last_writer_wins() {
    let f = setup().await;
    let r = store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Recording(rec_item(REC, "Lecture 1", 100))]).await;
    assert_eq!(r.applied, 1);
    let m = f.db.get_meeting(REC).await.unwrap().unwrap();
    assert_eq!((m.title.as_str(), m.recording_kind.as_str(), m.class_name.as_deref(), m.planned_minutes), ("Lecture 1", "class", Some("BIO 101"), Some(50)));
    let people = crate::people::meeting_people(f.pool(), REC).await.unwrap();
    assert_eq!(people.people.len(), 1);
    assert_eq!(people.details.unwrap().location.as_deref(), Some("Room 4"));

    // Older: ignored. Newer: wins.
    store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Recording(rec_item(REC, "Old title", 50))]).await;
    assert_eq!(f.db.get_meeting(REC).await.unwrap().unwrap().title, "Lecture 1");
    store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Recording(rec_item(REC, "New title", 200))]).await;
    assert_eq!(f.db.get_meeting(REC).await.unwrap().unwrap().title, "New title");

    // A local edit after that is newer than the phone's stale copy
    f.db.update_meeting_title(REC, "Mac title").await.unwrap();
    store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Recording(rec_item(REC, "Stale", 300))]).await;
    assert_eq!(f.db.get_meeting(REC).await.unwrap().unwrap().title, "Mac title");
}

#[tokio::test]
async fn marks_refs_topics_and_notes_apply_and_round_trip() {
    let f = setup().await;
    let items = vec![
        Item::Recording(rec_item(REC, "R", 1)),
        Item::Mark(p::MarkItem { id: uid(2), rec: REC.into(), at: 1_760_000_100_000, kind: "test".into(), note: Some("chapter 4".into()), created: 1, modified: 5 }),
        Item::Ref(p::RefItem { id: uid(3), rec: REC.into(), url: "https://example.com/syllabus".into(), title: Some("Syllabus".into()), note: None, created: 1, modified: 5 }),
        Item::Ref(p::RefItem { id: uid(4), rec: REC.into(), url: "javascript:alert(1)".into(), title: None, note: None, created: 1, modified: 5 }),
        Item::Topic(p::TopicItem { id: uid(5), rec: REC.into(), label: "Mitosis".into(), key: "mitosis".into(), conf: 900, source: "user".into(), created: 1 }),
        Item::Notes(p::NotesItem { rec: REC.into(), md: "**Summary**\nCells divide.".into(), made: 1_760_000_700_000, stale: false, modified: 9 }),
    ];
    let r = store::apply(f.pool(), &f.env, PHONE, &tk(), items).await;
    assert_eq!(r.applied, 5, "the javascript: link is refused: {:?}", r);
    let notes = f.db.get_meeting_notes(REC).await.unwrap().unwrap();
    assert_eq!(notes.summary.as_deref(), Some("**Summary**\nCells divide."));
    assert_eq!(notes.model_used.as_deref(), Some(store::SYNCED_NOTES_MODEL));
    let markers = crate::markers::list(f.pool(), REC).await.unwrap();
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].note.as_deref(), Some("chapter 4"));

    // Out to another device, same shapes
    let (out, _) = store::collect(f.pool(), &uid(99), 0, &tk()).await.unwrap();
    assert!(out.iter().any(|i| matches!(i, Item::Notes(n) if n.md == "**Summary**\nCells divide.")));
    assert!(out.iter().any(|i| matches!(i, Item::Topic(t) if t.conf == 900 && t.id == uid(5))));
    assert!(out.iter().any(|i| matches!(i, Item::Mark(m) if m.id == uid(2) && m.kind == "test")));
}

#[test]
fn mac_notes_become_markdown_by_layout() {
    let md = store::notes_markdown(
        Some("We agreed.".into()),
        Some(r#"["Pricing","Launch"]"#.into()),
        Some(r#"[{"text":"Ship Oct 14","made_by":"Ana"}]"#.into()),
        Some(r#"[{"task":"Send the agreement","assignee":"Alex","due_date":"Friday"}]"#.into()),
        Some("meeting-notes"),
    );
    assert_eq!(
        md,
        "**Summary**\nWe agreed.\n\n**Key topics**\n• Pricing\n• Launch\n\n**Decisions**\n• Ship Oct 14 — Ana\n\n**Action items**\n• Alex: Send the agreement · Friday"
    );
    let lecture = store::notes_markdown(Some("Mitosis.".into()), Some(r#"["Phases"]"#.into()), None, None, Some("lecture-notes"));
    assert_eq!(lecture, "**Lecture summary**\nMitosis.\n\n**Key concepts**\n• Phases");
}

// ═══════════════════════════════════════════════════════════════════════════
// Tombstones through the purge path
// ═══════════════════════════════════════════════════════════════════════════

/// A recording with one line from the phone, notes, a review guide and an
/// AI topic that all mention "Zenith Labs".
async fn phone_recording_with_outputs(f: &Fx) -> (String, i64) {
    let line_id = uid(0x10);
    store::apply(
        f.pool(),
        &f.env,
        PHONE,
        &tk(),
        vec![
            Item::Recording(rec_item(REC, "Board", 1)),
            Item::Line(line_item(&line_id, "We acquire Zenith Labs next quarter", 1_760_000_001_000)),
        ],
    )
    .await;
    let tid: i64 = sqlx::query_scalar("SELECT id FROM transcripts WHERE sync_id = ?")
        .bind(p::simple_id(&line_id).unwrap())
        .fetch_one(f.pool())
        .await
        .unwrap();
    f.db.save_meeting_notes("n1", REC, Some("Plan: acquire Zenith Labs."), None, None, None, None, None).await.unwrap();
    sqlx::query("INSERT INTO study_materials (id, meeting_id, summary) VALUES ('s1', ?, 'Zenith Labs deal')")
        .bind(REC)
        .execute(f.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO meeting_topics (id, meeting_id, topic, topic_key, confidence, source, created_at) VALUES ('t1', ?, 'Zenith deal', 'zenith deal', 0.9, 'ai', '2026-10-10T00:00:00Z')")
        .bind(REC)
        .execute(f.pool())
        .await
        .unwrap();
    (line_id, tid)
}

#[tokio::test]
async fn a_strike_from_the_phone_purges_everywhere_on_the_mac() {
    let f = setup().await;
    let (line_id, tid) = phone_recording_with_outputs(&f).await;
    assert_eq!(f.db.search_transcripts("Zenith").await.unwrap().len(), 1);

    let marker = uid(0x20);
    let phone_text = format!("We acquire ⟦stricken:{}⟧ next quarter", marker);
    let items = vec![
        Item::Strike(p::StrikeItem { id: marker.clone(), rec: REC.into(), target: "words".into(), from: Some(1_760_000_001_000), to: Some(1_760_000_002_000), created: 1_760_000_900_000, reason: Some("privileged".into()), line: Some(line_id.clone()) }),
        Item::Edit(p::EditItem { id: line_id.clone(), rec: REC.into(), keep: p::keep_list(&tk(), &phone_text) }),
    ];
    let r = store::apply(f.pool(), &f.env, PHONE, &tk(), items).await;
    assert_eq!(r.applied, 2, "{:?}", r);

    let text: String = sqlx::query_scalar("SELECT text FROM transcripts WHERE id = ?").bind(tid).fetch_one(f.pool()).await.unwrap();
    assert_eq!(p::text_to_wire(&text), phone_text);
    // Search index, notes, review guide, AI topic
    assert_eq!(f.db.search_transcripts("Zenith").await.unwrap().len(), 0);
    let notes = f.db.get_meeting_notes(REC).await.unwrap().unwrap();
    assert!(!notes.summary.clone().unwrap().contains("Zenith"), "{:?}", notes.summary);
    assert!(notes.stale_after_edit);
    let guides: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM study_materials WHERE meeting_id = ?").bind(REC).fetch_one(f.pool()).await.unwrap();
    assert_eq!(guides, 0);
    let ai_topics: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_topics WHERE source = 'ai'").fetch_one(f.pool()).await.unwrap();
    assert_eq!(ai_topics, 0);
    // The marker record, attached to the line, immutable
    let strikes = crate::redaction::list_strikes(f.pool(), REC).await.unwrap();
    assert_eq!(strikes.len(), 1);
    assert_eq!(strikes[0].transcript_id, Some(tid));
    assert_eq!(strikes[0].reason.as_deref(), Some("privileged"));
    // Applying it again changes nothing
    let r = store::apply(
        f.pool(),
        &f.env,
        PHONE,
        &tk(),
        vec![Item::Edit(p::EditItem { id: line_id, rec: REC.into(), keep: p::keep_list(&tk(), &phone_text) })],
    )
    .await;
    assert_eq!(r.applied, 0);
    // Not echoed back to the phone
    let (_, upto0) = (0, 0);
    let (back, _) = store::collect(f.pool(), PHONE, upto0, &tk()).await.unwrap();
    assert!(back.iter().all(|i| !matches!(i, Item::Edit(_) | Item::Strike(_))), "{:?}", back);
}

#[tokio::test]
async fn a_deleted_line_and_a_deleted_recording_are_gone_and_never_come_back() {
    let f = setup().await;
    let (line_id, _) = phone_recording_with_outputs(&f).await;
    let r = store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Gone(p::GoneItem { entity: "line".into(), id: line_id.clone(), rec: Some(REC.into()) })]).await;
    assert_eq!(r.applied, 1, "{:?}", r);
    assert!(line_texts(&f, REC).await.is_empty());
    assert_eq!(f.db.search_transcripts("Zenith").await.unwrap().len(), 0);
    // A late copy of the line is not re-imported
    store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Line(line_item(&line_id, "We acquire Zenith Labs next quarter", 1))]).await;
    assert!(line_texts(&f, REC).await.is_empty());

    // The recording
    std::fs::create_dir_all(f.env.app_data_dir.join("frames").join(REC)).unwrap();
    let r = store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Gone(p::GoneItem { entity: "recording".into(), id: REC.into(), rec: None })]).await;
    assert_eq!(r.applied, 1);
    assert!(f.db.get_meeting(REC).await.unwrap().is_none());
    assert!(!f.env.app_data_dir.join("frames").join(REC).exists());
    let notes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meeting_notes").fetch_one(f.pool()).await.unwrap();
    assert_eq!(notes, 0);
    store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Recording(rec_item(REC, "Back again?", 999_999))]).await;
    assert!(f.db.get_meeting(REC).await.unwrap().is_none(), "deletions win and are permanent");
}

#[tokio::test]
async fn a_recording_being_recorded_is_not_deleted_until_it_stops() {
    let mut f = setup().await;
    f.db.create_meeting(REC, "Live").await.unwrap();
    f.env.recording_meetings = vec![REC.into()];
    let r = store::apply(f.pool(), &f.env, PHONE, &tk(), vec![Item::Gone(p::GoneItem { entity: "recording".into(), id: REC.into(), rec: None })]).await;
    assert_eq!(r.retry, vec![REC.to_string()]);
    assert!(f.db.get_meeting(REC).await.unwrap().is_some());
}

#[tokio::test]
async fn mac_deletions_travel_as_gones() {
    let f = setup().await;
    f.db.create_meeting(REC, "R").await.unwrap();
    let tid = f.db.add_transcript(REC, "short line", None, true, 0.9).await.unwrap();
    let sid = sync_id_of(&f, tid).await;
    let mark = crate::markers::add(f.pool(), REC, chrono::Utc::now(), None, None, false).await.unwrap();
    let (_, upto) = store::collect(f.pool(), PHONE, 0, &tk()).await.unwrap();
    crate::markers::delete(f.pool(), &mark.id).await.unwrap();
    let pending = crate::redaction::request_delete_words(
        f.pool(),
        &f.env,
        &crate::redaction::WordTarget { meeting_id: REC.into(), transcript_id: tid, start: 0, end: 0, expected_text: None, whole_line: true },
    )
    .await
    .unwrap();
    crate::redaction::commit_pending(f.pool(), &f.env, &pending.id).await.unwrap();
    let (items, upto2) = store::collect(f.pool(), PHONE, upto, &tk()).await.unwrap();
    let gones: Vec<(String, String)> = items
        .iter()
        .filter_map(|i| match i {
            Item::Gone(g) => Some((g.entity.clone(), g.id.clone())),
            _ => None,
        })
        .collect();
    assert!(gones.contains(&("mark".into(), p::wire_id(&mark.id).unwrap())), "{:?}", items);
    assert!(gones.contains(&("line".into(), p::wire_id(&sid).unwrap())), "{:?}", items);

    // Deleting the recording: one gone, not one per line
    f.db.delete_meeting(REC).await.unwrap();
    let (items, _) = store::collect(f.pool(), PHONE, upto2, &tk()).await.unwrap();
    assert_eq!(items, vec![Item::Gone(p::GoneItem { entity: "recording".into(), id: REC.into(), rec: Some(REC.into()) })]);
}

// ═══════════════════════════════════════════════════════════════════════════
// Loopback: a full session over TLS against a test client
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug)]
struct Pin(String);

impl rustls::client::danger::ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        end_entity: &rustls::pki_types::CertificateDer<'_>,
        _: &[rustls::pki_types::CertificateDer<'_>],
        _: &rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        use sha2::Digest;
        if pairing::hex(&sha2::Sha256::digest(end_entity.as_ref())) == self.0 {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("certificate doesn't match the pin".into()))
        }
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::aws_lc_rs::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

struct Loop {
    f: Fx,
    port: u16,
    server: Arc<Server>,
    fingerprint: String,
    _stop: tokio::sync::watch::Sender<bool>,
}

async fn start_server(pro: bool) -> Loop {
    let f = setup().await;
    let identity = Identity::generate().unwrap();
    let fingerprint = identity.fingerprint();
    let env = f.env.clone();
    let server = Arc::new(Server {
        pool: f.pool().clone(),
        identity,
        mac_id: uid(0xee),
        mac_name: "Test Mac".into(),
        pairing: Mutex::new(None),
        pro: Arc::new(move || Box::pin(async move { pro })),
        env: Arc::new(move || env.clone()),
        notify: None,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = tokio::sync::watch::channel(false);
    tokio::spawn(server.clone().serve(listener, rx));
    Loop { f, port, server, fingerprint, _stop: tx }
}

type Tls = tokio_rustls::client::TlsStream<tokio::net::TcpStream>;

async fn connect(port: u16, pin: &str) -> std::io::Result<Tls> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pin(pin.to_string())))
        .with_no_client_auth();
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    let name = rustls::pki_types::ServerName::try_from("nofriction-sync.local").unwrap();
    tokio_rustls::TlsConnector::from(Arc::new(config)).connect(name, tcp).await
}

async fn pair(l: &Loop, code: &str, device: &str) -> Msg {
    let mut s = connect(l.port, &l.fingerprint).await.unwrap();
    p::write_msg(&mut s, &Msg::Pair { code: code.into(), device_id: device.into(), name: "Test iPhone".into() }).await.unwrap();
    p::read_msg(&mut s).await.unwrap()
}

/// The phone's side of a session: auth, push removals, pull, push changes.
async fn phone_session(l: &Loop, device: &str, secret: &[u8], since: i64, removals: Vec<Item>, changes: Vec<Item>) -> Result<(Vec<Item>, i64), Msg> {
    let mut s = connect(l.port, &l.fingerprint).await.unwrap();
    let nonce_p = [7u8; 32];
    p::write_msg(&mut s, &Msg::Hello { device_id: device.into(), nonce: p::b64(&nonce_p) }).await.unwrap();
    let (nonce, proof) = match p::read_msg(&mut s).await.unwrap() {
        Msg::Challenge { nonce, proof } => (nonce, proof),
        other => return Err(other),
    };
    let nonce_m = p::unb64(&nonce).unwrap();
    if p::unb64(&proof).unwrap() != p::mac_proof(secret, &nonce_p, &nonce_m) {
        return Err(Msg::error("bad_mac_proof", ""));
    }
    p::write_msg(&mut s, &Msg::Auth { proof: p::b64(&p::phone_proof(secret, &nonce_m, &nonce_p)) }).await.unwrap();
    match p::read_msg(&mut s).await.unwrap() {
        Msg::Welcome { .. } => {}
        other => return Err(other),
    }
    p::write_msg(&mut s, &Msg::Batch { phase: Phase::Removals, items: removals, last: true, upto: None }).await.unwrap();
    assert!(matches!(p::read_msg(&mut s).await.unwrap(), Msg::Applied { .. }));
    p::write_msg(&mut s, &Msg::Pull { since }).await.unwrap();
    let mut got = Vec::new();
    let upto = loop {
        match p::read_msg(&mut s).await.unwrap() {
            Msg::Batch { items, last, upto, .. } => {
                got.extend(items);
                if last {
                    break upto.unwrap();
                }
            }
            other => panic!("{:?}", other),
        }
    };
    p::write_msg(&mut s, &Msg::Batch { phase: Phase::Changes, items: changes, last: true, upto: None }).await.unwrap();
    assert!(matches!(p::read_msg(&mut s).await.unwrap(), Msg::Applied { .. }));
    p::write_msg(&mut s, &Msg::Done {}).await.unwrap();
    Ok((got, upto))
}

#[tokio::test]
async fn loopback_pair_then_full_sync_session() {
    let l = start_server(true).await;
    let device = uid(0x51);

    // Pair with the code shown on the Mac
    *l.server.pairing.lock() = Some(PendingPair::new());
    let code = l.server.pairing.lock().as_ref().unwrap().code.clone();
    let Msg::Paired { device_id, name, secret } = pair(&l, &code, &device).await else { panic!("not paired") };
    assert_eq!((device_id.as_str(), name.as_str()), (uid(0xee).as_str(), "Test Mac"));
    let secret = p::unb64(&secret).unwrap();
    assert_eq!(secret.len(), 32);
    assert_eq!(pairing::load_secret(&device).unwrap(), secret);
    assert_eq!(store::list_devices(l.f.pool()).await.unwrap()[0].name, "Test iPhone");
    // The code was one use
    assert!(matches!(pair(&l, &code, &uid(0x52)).await, Msg::Error { code, .. } if code == "expired_code"));

    // Mac content before the first sync
    l.f.db.create_meeting(REC, "Mac lecture").await.unwrap();
    let mac_tid = l.f.db.add_transcript(REC, "The Krebs cycle makes ATP", None, true, 0.9).await.unwrap();
    let mac_line = p::wire_id(&sync_id_of(&l.f, mac_tid).await).unwrap();

    // Session 1: the phone sends a new line and a mark; gets the Mac's
    let phone_line = uid(0x61);
    let changes = vec![
        Item::Line(line_item(&phone_line, "Glycolysis comes first", 1_760_000_000_500)),
        Item::Mark(p::MarkItem { id: uid(0x62), rec: REC.into(), at: 1_760_000_000_600, kind: "important".into(), note: None, created: 1, modified: 2 }),
    ];
    let (got, upto) = phone_session(&l, &device, &secret, 0, vec![], changes).await.unwrap();
    assert!(got.iter().any(|i| matches!(i, Item::Recording(r) if r.title == "Mac lecture")));
    assert!(got.iter().any(|i| matches!(i, Item::Line(x) if x.id == mac_line)));
    assert_eq!(line_texts(&l.f, REC).await.len(), 2);
    assert_eq!(crate::markers::list(l.f.pool(), REC).await.unwrap().len(), 1);
    assert!(store::list_devices(l.f.pool()).await.unwrap()[0].last_sync_at.is_some());

    // Session 2: the phone struck "Krebs cycle" in the Mac's line; the Mac
    // deleted the phone's line. Removals cross before content.
    let m = uid(0x70);
    let struck = format!("The ⟦stricken:{}⟧ makes ATP", m);
    let removals = vec![
        Item::Strike(p::StrikeItem { id: m.clone(), rec: REC.into(), target: "words".into(), from: None, to: None, created: 5, reason: None, line: Some(mac_line.clone()) }),
        Item::Edit(p::EditItem { id: mac_line.clone(), rec: REC.into(), keep: p::keep_list(&p::token_key(&secret), &struck) }),
    ];
    let phone_tid: i64 = sqlx::query_scalar("SELECT id FROM transcripts WHERE sync_id = ?")
        .bind(p::simple_id(&phone_line).unwrap())
        .fetch_one(l.f.pool())
        .await
        .unwrap();
    let pending = crate::redaction::request_delete_words(
        l.f.pool(),
        &l.f.env,
        &crate::redaction::WordTarget { meeting_id: REC.into(), transcript_id: phone_tid, start: 0, end: 0, expected_text: None, whole_line: true },
    )
    .await
    .unwrap();
    crate::redaction::commit_pending(l.f.pool(), &l.f.env, &pending.id).await.unwrap();

    let (got, _) = phone_session(&l, &device, &secret, upto, removals, vec![]).await.unwrap();
    assert_eq!(got, vec![Item::Gone(p::GoneItem { entity: "line".into(), id: phone_line, rec: Some(REC.into()) })]);
    let texts = line_texts(&l.f, REC).await;
    assert_eq!(texts.len(), 1);
    assert_eq!(p::text_to_wire(&texts[0]), struck);
    assert_eq!(l.f.db.search_transcripts("Krebs").await.unwrap().len(), 0);
}

#[tokio::test]
async fn loopback_rejects_wrong_code_wrong_secret_wrong_pin_unknown_device_and_no_pro() {
    let l = start_server(true).await;
    // Wrong code
    *l.server.pairing.lock() = Some(PendingPair::new());
    assert!(matches!(pair(&l, "AAAAAAAAAA", &uid(0x81)).await, Msg::Error { code, .. } if code == "bad_code"));
    assert!(pairing::load_secret(&uid(0x81)).is_none());

    // Pair properly, then use the wrong secret
    let code = l.server.pairing.lock().as_ref().unwrap().code.clone();
    let Msg::Paired { secret, .. } = pair(&l, &code, &uid(0x82)).await else { panic!() };
    let secret = p::unb64(&secret).unwrap();
    // The Mac's proof doesn't verify with a wrong secret; the Mac refuses a wrong phone proof
    assert!(phone_session(&l, &uid(0x82), &[3u8; 32], 0, vec![], vec![]).await.is_err());
    {
        let mut s = connect(l.port, &l.fingerprint).await.unwrap();
        p::write_msg(&mut s, &Msg::Hello { device_id: uid(0x82), nonce: p::b64(&[1u8; 32]) }).await.unwrap();
        let Msg::Challenge { .. } = p::read_msg(&mut s).await.unwrap() else { panic!() };
        p::write_msg(&mut s, &Msg::Auth { proof: p::b64(&[0u8; 32]) }).await.unwrap();
        assert!(matches!(p::read_msg(&mut s).await.unwrap(), Msg::Error { code, .. } if code == "bad_proof"));
    }
    // The right secret works
    assert!(phone_session(&l, &uid(0x82), &secret, 0, vec![], vec![]).await.is_ok());

    // Unknown device
    assert!(matches!(phone_session(&l, &uid(0x83), &secret, 0, vec![], vec![]).await, Err(Msg::Error { code, .. }) if code == "unknown_device"));

    // Wrong fingerprint: the TLS handshake fails before any message
    assert!(connect(l.port, &"00".repeat(32)).await.is_err());

    // Forget: the device can't sync any more
    pairing::delete_secret(&uid(0x82)).unwrap();
    store::remove_device(l.f.pool(), &uid(0x82)).await.unwrap();
    assert!(matches!(phone_session(&l, &uid(0x82), &secret, 0, vec![], vec![]).await, Err(Msg::Error { code, .. }) if code == "unknown_device"));

    // No Pro on the Mac
    let l2 = start_server(false).await;
    *l2.server.pairing.lock() = Some(PendingPair::new());
    let code = l2.server.pairing.lock().as_ref().unwrap().code.clone();
    assert!(matches!(pair(&l2, &code, &uid(0x84)).await, Msg::Error { code, .. } if code == "pro_required"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Golden fixtures for the iOS tests (byte-compatible encoding)
// ═══════════════════════════════════════════════════════════════════════════

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ios/NoFrictionTests/SyncFixtures")
}

fn golden_messages() -> Vec<(&'static str, Msg)> {
    let m = uid(0xa1);
    vec![
        ("pair", Msg::Pair { code: "ABCDEFGHJK".into(), device_id: PHONE.into(), name: "Casey's iPhone".into() }),
        ("paired", Msg::Paired { device_id: uid(0xee), name: "Casey's Mac".into(), secret: p::b64(&SECRET) }),
        ("hello", Msg::Hello { device_id: PHONE.into(), nonce: p::b64(&[1u8; 32]) }),
        ("challenge", Msg::Challenge { nonce: p::b64(&[2u8; 32]), proof: p::b64(&p::mac_proof(&SECRET, &[1u8; 32], &[2u8; 32])) }),
        ("auth", Msg::Auth { proof: p::b64(&p::phone_proof(&SECRET, &[2u8; 32], &[1u8; 32])) }),
        ("welcome", Msg::Welcome { device_id: uid(0xee), name: "Casey's Mac".into() }),
        ("pull", Msg::Pull { since: 1234 }),
        ("applied", Msg::Applied { retry: vec![] }),
        ("applied_retry", Msg::Applied { retry: vec![REC.into()] }),
        ("done", Msg::Done {}),
        ("error", Msg::error("bad_proof", "Authentication failed. Pair again.")),
        (
            "batch_removals",
            Msg::Batch {
                phase: Phase::Removals,
                items: vec![
                    Item::Strike(p::StrikeItem { id: m.clone(), rec: REC.into(), target: "words".into(), from: Some(1_760_000_001_000), to: Some(1_760_000_002_000), created: 1_760_000_900_000, reason: Some("privileged".into()), line: Some(uid(0x10)) }),
                    Item::Edit(p::EditItem { id: uid(0x10), rec: REC.into(), keep: p::keep_list(&tk(), &format!("We acquire ⟦stricken:{}⟧ next quarter.", m)) }),
                    Item::Gone(p::GoneItem { entity: "line".into(), id: uid(0x11), rec: Some(REC.into()) }),
                    Item::Gone(p::GoneItem { entity: "recording".into(), id: uid(0x12), rec: None }),
                ],
                last: true,
                upto: None,
            },
        ),
        (
            "batch_changes",
            Msg::Batch {
                phase: Phase::Changes,
                items: vec![
                    Item::Recording(rec_item(REC, "Lecture — \"Cells\" / 1", 1_760_000_999_000)),
                    Item::Recording(p::RecordingItem { id: uid(0x13), title: "Untitled".into(), started: 1, ended: None, kind: "meeting".into(), notebook: None, planned: None, cal: None, people: vec![], modified: 2 }),
                    Item::Line(p::LineItem { id: uid(0x14), rec: REC.into(), text: "Café ünïcödé 日本語 \"quoted\"\ttab".into(), at: 1_760_000_001_000, dur: Some(2500), speaker: Some("Ana".into()), src: Some("screen".into()) }),
                    Item::Notes(p::NotesItem { rec: REC.into(), md: "**Summary**\nCells divide.\n\n**Action items**\n• Read ch. 4".into(), made: 1_760_000_700_000, stale: true, modified: 1_760_000_700_001 }),
                    Item::Mark(p::MarkItem { id: uid(0x15), rec: REC.into(), at: 1_760_000_100_000, kind: "test".into(), note: Some("on the exam".into()), created: 1_760_000_100_001, modified: 1_760_000_100_002 }),
                    Item::Ref(p::RefItem { id: uid(0x16), rec: REC.into(), url: "https://example.com/a?b=1&c=2".into(), title: Some("Syllabus".into()), note: None, created: 3, modified: 4 }),
                    Item::Topic(p::TopicItem { id: uid(0x17), rec: REC.into(), label: "Mitosis".into(), key: "mitosis".into(), conf: 875, source: "ai".into(), created: 5 }),
                ],
                last: true,
                upto: Some(77),
            },
        ),
    ]
}

/// Golden files: one canonical message per file, plus hash vectors and the
/// merge cases. `NF_WRITE_SYNC_FIXTURES=1 cargo test --lib sync::tests::golden`
/// rewrites them; otherwise they must match exactly.
#[test]
fn golden_fixtures_match() {
    let dir = fixtures_dir();
    let write = std::env::var("NF_WRITE_SYNC_FIXTURES").is_ok();
    if write {
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut files: Vec<(String, Vec<u8>)> = golden_messages().into_iter().map(|(n, m)| (format!("{}.json", n), p::encode(&m))).collect();

    // Auth and token hash vectors
    let key = tk();
    let words = ["We", "acquire", "next", "quarter.", "Café", "日本語", "\"quoted\""];
    let vectors = serde_json::json!({
        "secret": p::b64(&SECRET),
        "token_key": pairing::hex(&key),
        "nonce_phone": p::b64(&[1u8; 32]),
        "nonce_mac": p::b64(&[2u8; 32]),
        "mac_proof": p::b64(&p::mac_proof(&SECRET, &[1u8; 32], &[2u8; 32])),
        "phone_proof": p::b64(&p::phone_proof(&SECRET, &[2u8; 32], &[1u8; 32])),
        "words": words.iter().map(|w| serde_json::json!({"word": w, "hash": p::word_hash(&key, w)})).collect::<Vec<_>>(),
    });
    files.push(("vectors.json".into(), format!("{}\n", serde_json::to_string_pretty(&vectors).unwrap()).into_bytes()));

    // Merge cases: receiver text, sender keep list, expected result
    let cases: Vec<serde_json::Value> = merge_cases()
        .into_iter()
        .map(|(name, local, sender, expected)| {
            serde_json::json!({"name": name, "local": local, "keep": p::keep_list(&key, &sender), "expected": expected})
        })
        .collect();
    files.push(("merge_cases.json".into(), format!("{}\n", serde_json::to_string_pretty(&cases).unwrap()).into_bytes()));

    for (name, bytes) in files {
        let path = dir.join(&name);
        if write {
            std::fs::write(&path, &bytes).unwrap();
        } else {
            let on_disk = std::fs::read(&path).unwrap_or_else(|_| panic!("missing fixture {:?}; run with NF_WRITE_SYNC_FIXTURES=1", path));
            assert_eq!(String::from_utf8_lossy(&on_disk), String::from_utf8_lossy(&bytes), "fixture {} is out of date", name);
        }
    }
    // Every golden message decodes back to itself
    for (_, m) in golden_messages() {
        assert_eq!(p::decode(&p::encode(&m)).unwrap(), m);
    }
}

#[tokio::test]
async fn migration_is_idempotent_and_backfills_old_rows_once() {
    let f = setup().await;
    f.db.create_meeting(REC, "R").await.unwrap();
    f.db.add_transcript(REC, "one", None, true, 0.9).await.unwrap();
    f.db.run_migrations().await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_meta").fetch_one(f.pool()).await.unwrap();
    assert_eq!(rows, 2);
    let null_ids: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM transcripts WHERE sync_id IS NULL").fetch_one(f.pool()).await.unwrap();
    assert_eq!(null_ids, 0);
    let r = sqlx::query("SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync_%'").fetch_one(f.pool()).await.unwrap();
    assert_eq!(r.get::<i64, _>("n"), 27);
}
