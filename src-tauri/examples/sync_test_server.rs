//! A Mac sync server for end-to-end tests against the iOS Simulator
//! (docs/SYNC.md "Testing"). Everything lives in a temp directory and an
//! in-memory secret store: it never touches the app's data or the Keychain.
//!
//!   cargo run --example sync_test_server -- <dir>
//!
//! Writes `<dir>/link.txt` (a pairing link for 127.0.0.1), then:
//! - after the first session, deletes the word "Tuesday" from the iPhone's
//!   line on the Mac (a Mac → iPhone removal);
//! - after the second session, writes `<dir>/result.json` and exits.

use nofriction_meetings_lib::database::DatabaseManager;
use nofriction_meetings_lib::redaction::{self, RedactionEnv, WordTarget};
use nofriction_meetings_lib::sync::{pairing, protocol as p, server, store};
use std::sync::Arc;

const MAC_REC: &str = "5a7d2c1e-0b3f-4c8d-9e6a-1f2b3c4d5e6f";

#[tokio::main]
async fn main() {
    // Never the real Keychain
    nofriction_meetings_lib::secrets::set_store(Box::new(nofriction_meetings_lib::secrets::MemoryStore::default()));
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: sync_test_server <dir>"));
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    let db = Arc::new(DatabaseManager::new(&data.join("nofriction_meetings.db")).await.unwrap());
    db.run_migrations().await.unwrap();
    let pool = db.pool().clone();

    // A Mac recording with notes
    db.create_meeting(MAC_REC, "Mac planning meeting").await.unwrap();
    db.add_transcript(MAC_REC, "We acquire Zenith Labs next quarter", None, true, 0.9).await.unwrap();
    db.add_transcript(MAC_REC, "Budget review is on Friday", None, true, 0.9).await.unwrap();
    db.save_meeting_notes("n1", MAC_REC, Some("Plan: acquire Zenith Labs."), None, None, None, None, Some("meeting-notes"))
        .await
        .unwrap();

    let env = RedactionEnv { app_data_dir: data.clone(), cache_dir: dir.join("cache"), video_enabled: false, recording_meetings: vec![] };
    let identity = pairing::Identity::generate().unwrap();
    let fingerprint = identity.fingerprint();
    let env2 = env.clone();
    let srv = Arc::new(server::Server {
        pool: pool.clone(),
        identity,
        mac_id: "00000000-0000-4000-8000-00000000e2e0".into(),
        mac_name: "Test Mac".into(),
        pairing: parking_lot::Mutex::new(Some(pairing::PendingPair::new())),
        pro: Arc::new(|| Box::pin(async { true })),
        env: Arc::new(move || env2.clone()),
        notify: None,
    });
    let code = srv.pairing.lock().as_ref().unwrap().code.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (_tx, rx) = tokio::sync::watch::channel(false);
    tokio::spawn(srv.clone().serve(listener, rx));
    let link = pairing::pairing_link(&srv.mac_id, &srv.mac_name, &fingerprint, &["127.0.0.1".to_string()], port, &code);
    std::fs::write(dir.join("link.txt"), &link).unwrap();
    println!("listening on 127.0.0.1:{}", port);

    // Wait for sessions
    let mut seen = 0usize;
    let mut last: Option<String> = None;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let devices = store::list_devices(&pool).await.unwrap();
        let Some(d) = devices.first() else { continue };
        if d.last_sync_at.is_some() && d.last_sync_at != last {
            last = d.last_sync_at.clone();
            seen += 1;
            println!("session {} done", seen);
            if seen == 1 {
                // Mac → iPhone removal: delete "Tuesday" from the iPhone's line
                let row: Option<(i64, String, String)> =
                    sqlx::query_as("SELECT id, meeting_id, text FROM transcripts WHERE text LIKE '%Tuesday%' LIMIT 1")
                        .fetch_optional(&pool)
                        .await
                        .unwrap();
                if let Some((tid, mid, text)) = row {
                    let b = text.find("Tuesday").unwrap();
                    let s = text[..b].encode_utf16().count();
                    let target = WordTarget { meeting_id: mid, transcript_id: tid, start: s, end: s + 7, expected_text: None, whole_line: false };
                    let pending = redaction::request_delete_words(&pool, &env, &target).await.unwrap();
                    redaction::commit_pending(&pool, &env, &pending.id).await.unwrap();
                    println!("deleted a word on the Mac");
                }
            }
            if seen >= 2 {
                break;
            }
        }
    }

    let count = |sql: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&pool).await.unwrap() }
    };
    let result = serde_json::json!({
        "sessions": seen,
        "recordings": count("SELECT COUNT(*) FROM meetings").await,
        "lines": count("SELECT COUNT(*) FROM transcripts").await,
        "zenith_in_search": db.search_transcripts("Zenith").await.unwrap().len(),
        "zenith_in_notes": count("SELECT COUNT(*) FROM meeting_notes WHERE summary LIKE '%Zenith%'").await,
        "strikes": count("SELECT COUNT(*) FROM redactions WHERE action = 'strike'").await,
        "phone_lines": count("SELECT COUNT(*) FROM transcripts WHERE meeting_id != '5a7d2c1e-0b3f-4c8d-9e6a-1f2b3c4d5e6f'").await,
        "deleted_phone_line_present": count("SELECT COUNT(*) FROM transcripts WHERE text LIKE '%delete me%'").await,
        "marks": count("SELECT COUNT(*) FROM meeting_markers").await,
        "phone_notes": count("SELECT COUNT(*) FROM meeting_notes WHERE model_used = 'synced-markdown'").await,
        "texts": sqlx::query_scalar::<_, String>("SELECT text FROM transcripts ORDER BY timestamp").fetch_all(&pool).await.unwrap()
            .iter().map(|t| p::text_to_wire(t)).collect::<Vec<_>>(),
    });
    std::fs::write(dir.join("result.json"), serde_json::to_string_pretty(&result).unwrap()).unwrap();
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
}
