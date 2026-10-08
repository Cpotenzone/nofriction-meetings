//! Chat with your recordings: scope, retrieval, citations and the purge
//! (docs/TOPICS_AND_CHAT.md, docs/REDACTION.md). The AI is always a mock
//! here; no endpoint is called.

use super::prompt::*;
use super::retrieval::*;
use super::*;
use crate::ai::{AiError, Msg, Opts};
use crate::database::DatabaseManager;
use crate::redaction::{self as rd, RedactionEnv, WordTarget};
use parking_lot::Mutex;
use sqlx::{ConnectOptions, Connection};
use std::path::PathBuf;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// Pure pieces
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn citation_numbers_are_read_in_order_of_first_use() {
    assert_eq!(cited_numbers("Ship Friday [2]. Budget agreed [1], see [2] and [1, 3]."), vec![2, 1, 3]);
    assert_eq!(cited_numbers("[12] then [x] and [] and [0]"), vec![12]);
    assert_eq!(cited_numbers("no citations"), Vec::<usize>::new());
    assert_eq!(cited_numbers("[1"), Vec::<usize>::new());
}

#[test]
fn answers_are_cleaned_and_titles_clipped() {
    assert_eq!(clean_answer("<think>plan</think>\n```markdown\n**Yes** [1]\n```"), "**Yes** [1]");
    assert_eq!(clean_answer("  Plain [1].  "), "Plain [1].");
    assert_eq!(clean_answer("```\nnot closed"), "```\nnot closed");
    assert_eq!(title_of("  What did we   decide? "), "What did we decide?");
    let t = title_of(&"word ".repeat(30));
    assert!(t.chars().count() <= 60 && t.ends_with('…'));
}

#[test]
fn scopes_parse_and_fall_back_to_all() {
    let s: Scope = serde_json::from_str(r#"{"kind": "notebook", "value": "BIO 101"}"#).unwrap();
    assert_eq!(s, Scope::notebook("BIO 101"));
    let s: Scope = serde_json::from_str(r#"{"kind": "all"}"#).unwrap();
    assert_eq!(s, Scope::all());
    assert!(serde_json::from_str::<Scope>(r#"{"kind": "galaxy"}"#).is_err());
    assert_eq!(Scope::from_stored("galaxy", Some("x".into())), Scope::all());
    assert_eq!(Scope::from_stored("meeting", Some(" ".into())).value, None);
    assert_eq!(Scope::from_stored("topic", Some("q4 roadmap".into())), Scope::topic("q4 roadmap"));
    assert_eq!(Filter::Notebook("x".into()).clause("t.meeting_id"), " AND t.meeting_id IN (SELECT id FROM meetings WHERE class_name = ? COLLATE NOCASE)");
    assert_eq!(Filter::All.clause("id"), "");
}

#[test]
fn packing_ranks_caps_per_recording_and_fits_the_budget() {
    let p = |m: &str, text: &str, score: f64| Passage {
        n: 0,
        meeting_id: m.into(),
        title: m.into(),
        started_at: "2026-10-06T10:00:00Z".into(),
        kind: "meeting".into(),
        notebook: None,
        source: "transcript",
        timestamp_ms: Some(0),
        excerpt: text.into(),
        score,
    };
    let all = vec![p("a", "x1", 0.1), p("a", "x2", 0.9), p("a", "x3", 0.8), p("b", "y1", 0.5), p("a", "x2", 0.9)];
    let got = pack(all.clone(), 10_000, 12, 2);
    assert_eq!(got.iter().map(|g| g.excerpt.as_str()).collect::<Vec<_>>(), vec!["x2", "x3", "y1"], "ranked, duplicate and third 'a' passage dropped");
    assert_eq!(got.iter().map(|g| g.n).collect::<Vec<_>>(), vec![1, 2, 3]);
    let got = pack(all.clone(), 200, 12, 12);
    assert_eq!(got.len(), 2, "two passages of ~92 chars fit in 200");
    assert_eq!(pack(all, 10, 12, 12).len(), 1, "the best passage always fits");
    assert_eq!(term_score("The Q4 Roadmap is set", &["q4".into(), "roadmap".into(), "budget".into()]), 2.0 / 3.0);
    assert_eq!(clip("a  b\n c", 10), "a b c");
    assert_eq!(clip("abcdefghij", 5), "abcd…");
}

#[test]
fn history_is_clipped_for_small_windows() {
    let h: Vec<(String, String)> = (0..8).map(|i| ((if i % 2 == 0 { "user" } else { "assistant" }).to_string(), format!("turn {} {}", i, "x".repeat(900)))).collect();
    let big = history_messages(&h, 32_768);
    assert_eq!(big.len(), 8);
    assert_eq!(big[1].role, crate::ai::Role::Assistant);
    let small = history_messages(&h, 4_096);
    assert_eq!(small.len(), 4);
    assert!(small[0].text().starts_with("turn 4"));
    assert!(small.iter().all(|m| m.text().chars().count() <= 500));
    assert!(max_tokens(4_096) < max_tokens(32_768));
}

// ═══════════════════════════════════════════════════════════════════════════
// Database fixture
// ═══════════════════════════════════════════════════════════════════════════

struct Mock {
    ctx: usize,
    calls: Mutex<Vec<Vec<Msg>>>,
    reply: Mutex<String>,
    error: Option<AiError>,
}

impl Mock {
    fn new(reply: &str) -> Self {
        Self { ctx: 32_768, calls: Mutex::new(Vec::new()), reply: Mutex::new(reply.into()), error: None }
    }
    fn last_prompt(&self) -> String {
        self.calls.lock().last().unwrap().iter().map(|m| m.text()).collect::<Vec<_>>().join("\n")
    }
}

#[async_trait::async_trait]
impl Completer for Mock {
    async fn complete(&self, msgs: Vec<Msg>, _opts: Opts) -> Result<String, AiError> {
        self.calls.lock().push(msgs);
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        Ok(self.reply.lock().clone())
    }
    fn context_tokens(&self) -> usize {
        self.ctx
    }
}

struct Fx {
    db: Arc<DatabaseManager>,
    dir: PathBuf,
    env: RedactionEnv,
    db_path: PathBuf,
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn start_of(f: &Fx, meeting: &str) -> chrono::DateTime<chrono::Utc> {
    let s: String = sqlx::query_scalar("SELECT started_at FROM meetings WHERE id = ?").bind(meeting).fetch_one(f.db.pool()).await.unwrap();
    chrono::DateTime::parse_from_rfc3339(&s).unwrap().with_timezone(&chrono::Utc)
}

async fn line_at(f: &Fx, meeting: &str, secs: i64, text: &str) -> i64 {
    let t0 = start_of(f, meeting).await;
    f.db.add_transcript_at(meeting, text, None, true, 0.9, t0 + chrono::Duration::seconds(secs)).await.unwrap()
}

/// Three recordings: m1 (class, Notebook BIO 101, topic "cell membrane"),
/// m2 (meeting, Notebook Acme, with notes), m3 (personal, topic "cell membrane").
async fn fx() -> Fx {
    let dir = std::env::temp_dir().join(format!("nf-chat-{}", uuid::Uuid::new_v4()));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db_path = data.join("nofriction_meetings.db");
    let db = DatabaseManager::new(&db_path).await.unwrap();
    db.run_migrations().await.unwrap();
    db.create_meeting("m1", "Biology 101").await.unwrap();
    db.create_meeting("m2", "Acme sync").await.unwrap();
    db.create_meeting("m3", "Dentist").await.unwrap();
    db.set_meeting_notebook("m1", Some("BIO 101")).await.unwrap();
    db.set_meeting_notebook("m2", Some("Acme")).await.unwrap();
    db.set_meeting_kind("m1", crate::recording_kind::RecordingKind::Class).await.unwrap();
    db.set_meeting_kind("m3", crate::recording_kind::RecordingKind::Personal).await.unwrap();
    let f = Fx {
        env: RedactionEnv { app_data_dir: data, cache_dir: dir.join("cache"), video_enabled: false, recording_meetings: Vec::new() },
        db: Arc::new(db),
        dir,
        db_path,
    };
    line_at(&f, "m1", 10, "the cell membrane is selectively permeable").await;
    line_at(&f, "m1", 20, "mitochondria make ATP for the cell").await;
    line_at(&f, "m1", 30, "osmosis moves water across the membrane").await;
    line_at(&f, "m2", 20, "we decided to ship the launch on Friday").await;
    line_at(&f, "m2", 40, "the budget stays at ten thousand").await;
    line_at(&f, "m3", 5, "the dentist said floss every night").await;
    f.db
        .save_meeting_notes("n2", "m2", Some("Launch planning sync."), Some("[\"launch\"]"), Some("[{\"text\": \"Ship the launch Friday\", \"made_by\": \"Ana\"}]"), Some("[{\"task\": \"Send the budget\", \"assignee\": \"Bo\"}]"), None, Some("default"))
        .await
        .unwrap();
    crate::markers::add_at_offset(f.db.pool(), "m2", 40_000, Some("test"), Some("budget follow up")).await.unwrap();
    crate::topics::set_for_meeting(f.db.pool(), "m1", &["Cell membranes".into()]).await.unwrap();
    crate::topics::set_for_meeting(f.db.pool(), "m3", &["cell membrane".into(), "Flossing".into()]).await.unwrap();
    f
}

fn meetings_of(p: &[Passage]) -> Vec<&str> {
    let mut v: Vec<&str> = p.iter().map(|x| x.meeting_id.as_str()).collect();
    v.dedup();
    v.sort();
    v.dedup();
    v
}

// ═══════════════════════════════════════════════════════════════════════════
// Retrieval
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn retrieval_stays_inside_the_scope_and_the_budget() {
    let f = fx().await;
    let mut conn = f.db.pool().acquire().await.unwrap();
    // All: the transcript line about mitochondria ranks first, with its time
    let info = resolve(&mut conn, &Scope::all()).await.unwrap();
    assert_eq!((info.label.as_str(), info.count), ("All recordings", 3));
    let p = retrieve(&mut conn, &info, "what makes ATP in the cell?", 4_000, 12).await.unwrap();
    assert_eq!(p[0].meeting_id, "m1");
    assert_eq!(p[0].timestamp_ms, Some(20_000));
    assert_eq!((p[0].n, p[0].source, p[0].kind.as_str(), p[0].notebook.as_deref()), (1, "transcript", "class", Some("BIO 101")));
    assert!(p[0].excerpt.contains("mitochondria"));
    // Notebook Acme: nothing about ATP there, so its own recent passages come instead
    let info = resolve(&mut conn, &Scope::notebook("acme")).await.unwrap();
    assert_eq!((info.label.as_str(), info.count), ("Notebook · acme", 1));
    let p = retrieve(&mut conn, &info, "what makes ATP in the cell?", 4_000, 12).await.unwrap();
    assert_eq!(meetings_of(&p), vec!["m2"]);
    assert!(p.iter().any(|x| x.source == "notes" && x.excerpt.starts_with("Summary:")), "{:?}", p);
    // Notes and marker notes match by term, with the marker's time
    let p = retrieve(&mut conn, &info, "who sends the budget?", 4_000, 12).await.unwrap();
    assert!(p.iter().any(|x| x.source == "notes" && x.excerpt.contains("Send the budget (Bo)")), "{:?}", p);
    assert!(p.iter().any(|x| x.source == "marker" && x.timestamp_ms == Some(40_000) && x.excerpt.contains("Follow up: budget")), "{:?}", p);
    assert!(p.iter().any(|x| x.source == "transcript" && x.timestamp_ms == Some(40_000)));
    // Topic: m1 and m3 share "cell membrane"; m2 never appears
    let info = resolve(&mut conn, &Scope::topic("cell membrane")).await.unwrap();
    assert_eq!((info.label.as_str(), info.count), ("Topic · Cell membranes", 2));
    let p = retrieve(&mut conn, &info, "membrane dentist budget launch", 4_000, 12).await.unwrap();
    assert_eq!(meetings_of(&p), vec!["m1", "m3"]);
    // One recording: only it, and a question with no matching terms samples its lines
    let info = resolve(&mut conn, &Scope::meeting("m2")).await.unwrap();
    assert_eq!(info.label, "Recording · Acme sync");
    let p = retrieve(&mut conn, &info, "summarize this", 4_000, 12).await.unwrap();
    assert_eq!(meetings_of(&p), vec!["m2"]);
    assert!(p.iter().any(|x| x.source == "transcript") && p.iter().any(|x| x.source == "notes"));
    // The budget bounds the total; a tiny window still gets one passage
    let info = resolve(&mut conn, &Scope::all()).await.unwrap();
    let p = retrieve(&mut conn, &info, "membrane cell water ATP osmosis", 150, 12).await.unwrap();
    assert_eq!(p.len(), 1);
    let p = retrieve(&mut conn, &info, "membrane cell water ATP osmosis", 600, 12).await.unwrap();
    assert!(p.len() >= 2 && p.iter().map(|x| x.cost()).sum::<usize>() <= 600, "{:?}", p);
    // Unknown scopes are refused
    assert!(resolve(&mut conn, &Scope::meeting("nope")).await.is_err());
    assert!(resolve(&mut conn, &Scope { kind: ScopeKind::Notebook, value: None }).await.is_err());
    // The summary feeds the suggested questions
    let s = summary(&mut conn, &Scope::all()).await.unwrap();
    assert_eq!(s.count, 3);
    assert_eq!(s.recent_titles.len(), 3);
    assert_eq!(s.topics[0], "Cell membranes");
    assert!(s.notebooks.contains(&"BIO 101".to_string()) && s.notebooks.contains(&"Acme".to_string()));
    assert_eq!(s.kinds.len(), 3);
    let s = summary(&mut conn, &Scope::notebook("BIO 101")).await.unwrap();
    assert_eq!((s.count, s.recent_titles[0].as_str(), s.kinds.as_slice()), (1, "Biology 101", &["class".to_string()][..]));
}

// ═══════════════════════════════════════════════════════════════════════════
// Asking, memory, citations
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn an_answer_carries_its_scope_and_citations_and_the_thread_remembers() {
    let f = fx().await;
    let m = Mock::new("They decided to **ship Friday** [1].\n\n- Budget: ten thousand [2]");
    let turn = ask(f.db.pool(), &m, None, &Scope::notebook("Acme"), "What did we decide about the launch?").await.unwrap();
    assert_eq!(turn.thread.title, "What did we decide about the launch?");
    assert_eq!(turn.thread.scope, Scope::notebook("Acme"));
    assert_eq!(turn.user.content, "What did we decide about the launch?");
    assert_eq!(turn.assistant.scope_label.as_deref(), Some("Notebook · Acme"));
    assert!(turn.assistant.content.starts_with("They decided"));
    let c1 = turn.assistant.citations.iter().find(|c| c.n == 1).unwrap();
    assert_eq!((c1.meeting_id.as_str(), c1.title.as_str(), c1.timestamp_ms), ("m2", "Acme sync", Some(20_000)));
    assert!(c1.excerpt.contains("ship the launch on Friday"));
    assert_eq!(cited_numbers(&turn.assistant.content), vec![1, 2]);
    assert!(turn.assistant.citations.iter().all(|c| c.meeting_id == "m2"));
    // The prompt: system rules, scope, numbered sources with recording facts, the question
    let p = m.last_prompt();
    assert!(p.starts_with(&system_prompt()));
    assert!(p.contains("SCOPE: Notebook · Acme (1 recording)\n"), "{}", p);
    assert!(p.contains("[1] Acme sync — ") && p.contains(", Meeting, Notebook Acme — said at 0:20\nwe decided to ship the launch on Friday"), "{}", p);
    assert!(p.ends_with("QUESTION: What did we decide about the launch?"));
    // A second question in the thread carries the first exchange
    let m2 = Mock::new("Friday [1].");
    let t2 = ask(f.db.pool(), &m2, Some(&turn.thread.id), &Scope::all(), "And when exactly?").await.unwrap();
    let msgs = m2.calls.lock()[0].clone();
    assert_eq!(msgs.len(), 4, "system, user, assistant, user");
    assert_eq!(msgs[1].text(), "What did we decide about the launch?");
    assert!(msgs[2].text().starts_with("They decided"));
    assert_eq!(t2.thread.id, turn.thread.id);
    assert_eq!(t2.thread.scope, Scope::all(), "the thread follows the latest scope");
    assert_eq!(t2.assistant.scope_label.as_deref(), Some("All recordings"));
    let detail = get_thread(f.db.pool(), &turn.thread.id).await.unwrap();
    assert_eq!(detail.messages.iter().map(|x| x.role.as_str()).collect::<Vec<_>>(), vec!["user", "assistant", "user", "assistant"]);
    assert_eq!(detail.messages[1].citations.len(), turn.assistant.citations.len());
    assert_eq!(list_threads(f.db.pool()).await.unwrap().len(), 1);
    // AI access errors come back as they are, and nothing is stored
    let mut bad = Mock::new("");
    bad.error = Some(AiError::ProRequired);
    let e = ask(f.db.pool(), &bad, None, &Scope::all(), "anything").await.unwrap_err();
    assert!(e.starts_with("PRO_REQUIRED"));
    assert_eq!(list_threads(f.db.pool()).await.unwrap().len(), 1);
    assert!(ask(f.db.pool(), &m, None, &Scope::all(), "   ").await.is_err());
    // Delete
    delete_thread(f.db.pool(), &turn.thread.id).await.unwrap();
    assert!(list_threads(f.db.pool()).await.unwrap().is_empty());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_message_sources").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn stricken_words_never_reach_the_prompt() {
    let f = fx().await;
    let text = "the secret code is Zanzibar";
    let id = line_at(&f, "m1", 50, text).await;
    let b = text.find("Zanzibar").unwrap();
    let s = text[..b].encode_utf16().count();
    rd::strike_words(f.db.pool(), &f.env, &WordTarget { meeting_id: "m1".into(), transcript_id: id, start: s, end: s + 8, expected_text: None, whole_line: false }, None)
        .await
        .unwrap();
    let m = Mock::new("No idea.");
    ask(f.db.pool(), &m, None, &Scope::meeting("m1"), "What is the secret code?").await.unwrap();
    let p = m.last_prompt();
    assert!(!p.contains("Zanzibar") && !p.contains("strickenid"), "{}", p);
    assert!(p.contains("[stricken from the record]"), "{}", p);
}

// ═══════════════════════════════════════════════════════════════════════════
// Purge
// ═══════════════════════════════════════════════════════════════════════════

async fn counts(pool: &Pool<Sqlite>, thread: &str) -> (i64, i64) {
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE thread_id = ? AND role = 'user'").bind(thread).fetch_one(pool).await.unwrap();
    let answers: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE thread_id = ? AND role = 'assistant'").bind(thread).fetch_one(pool).await.unwrap();
    (users, answers)
}

#[tokio::test]
async fn transcript_edits_and_meeting_delete_remove_the_answers_that_used_it() {
    let f = fx().await;
    let m = Mock::new("ATP [1].");
    let bio = ask(f.db.pool(), &m, None, &Scope::meeting("m1"), "what makes ATP?").await.unwrap();
    let acme = ask(f.db.pool(), &Mock::new("Friday [1]."), None, &Scope::notebook("Acme"), "when do we launch?").await.unwrap();
    // Answers drew on the recording even when the text doesn't cite it
    let uncited = ask(f.db.pool(), &Mock::new("Not sure."), Some(&bio.thread.id), &Scope::all(), "and osmosis?").await.unwrap();
    assert!(uncited.assistant.citations.iter().any(|c| c.meeting_id == "m1"));
    // A backup made now
    rd::wal_checkpoint_truncate(f.db.pool()).await.unwrap();
    let bdir = f.env.app_data_dir.join("backups");
    std::fs::create_dir_all(&bdir).unwrap();
    let copy = bdir.join("copy.db");
    std::fs::copy(&f.db_path, &copy).unwrap();
    // Striking words in m1: the preview says so; both answers of the Bio thread go, the questions stay, the thread is flagged
    let text = "mitochondria make ATP for the cell";
    let id: i64 = sqlx::query_scalar("SELECT id FROM transcripts WHERE meeting_id = 'm1' AND text = ?").bind(text).fetch_one(f.db.pool()).await.unwrap();
    let target = WordTarget { meeting_id: "m1".into(), transcript_id: id, start: 0, end: 12, expected_text: None, whole_line: false };
    let preview = rd::preview_words(f.db.pool(), &f.env, &target).await.unwrap();
    assert!(preview.iter().any(|i| i.contains("2 CHAT answers that drew on this recording")), "{:?}", preview);
    let out = rd::strike_words(f.db.pool(), &f.env, &target, None).await.unwrap();
    assert_eq!(counts(f.db.pool(), &bio.thread.id).await, (2, 0));
    assert_eq!(counts(f.db.pool(), &acme.thread.id).await, (1, 1), "the other thread is untouched");
    let t = get_thread(f.db.pool(), &bio.thread.id).await.unwrap().thread;
    assert_eq!(t.flag.as_deref(), Some(FLAG_REMOVED));
    assert!(get_thread(f.db.pool(), &acme.thread.id).await.unwrap().thread.flag.is_none());
    assert_eq!((out.backups_purged, out.backups_deleted), (1, 0), "{:?}", out.warnings);
    let mut c = sqlx::sqlite::SqliteConnectOptions::new().filename(&copy).connect().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_messages WHERE role = 'assistant' AND thread_id = ?").bind(&bio.thread.id).fetch_one(&mut c).await.unwrap();
    assert_eq!(n, 0, "purged from the backup too");
    // An older backup without the chat tables is left alone
    sqlx::query("DROP TABLE chat_message_sources").execute(&mut c).await.unwrap();
    assert_eq!(purge_for_meeting(&mut c, "m2").await.unwrap(), 0);
    assert!(rd::redact_ai_outputs(&mut c, "m2", "launch Friday", "[stricken from the record]").await.is_ok());
    c.close().await.unwrap();
    // Deleting m2 removes the Acme answer and flags that thread
    f.db.delete_meeting("m2").await.unwrap();
    assert_eq!(counts(f.db.pool(), &acme.thread.id).await, (1, 0));
    assert_eq!(get_thread(f.db.pool(), &acme.thread.id).await.unwrap().thread.flag.as_deref(), Some(FLAG_REMOVED));
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM chat_message_sources WHERE meeting_id = 'm2'").fetch_one(f.db.pool()).await.unwrap();
    assert_eq!(n, 0);
    // A thread scoped to a deleted recording is flagged even without answers
    let scoped = ask(f.db.pool(), &Mock::new("Floss [1]."), None, &Scope::meeting("m3"), "what did the dentist say?").await.unwrap();
    f.db.delete_meeting("m3").await.unwrap();
    assert_eq!(get_thread(f.db.pool(), &scoped.thread.id).await.unwrap().thread.flag.as_deref(), Some(FLAG_REMOVED));
}
