use super::ai::*;
use super::build::*;
use crate::recording_kind::RecordingKind;
use std::io::Read;
use std::path::PathBuf;

fn line(ms: i64, speaker: Option<&str>, text: &str) -> TranscriptLine {
    TranscriptLine { ms, speaker: speaker.map(str::to_string), text: text.into() }
}

fn tmp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nf-export-{}-{}", tag, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn sample_input(dir: &std::path::Path) -> PackageInput {
    let shot = dir.join("frame.png");
    std::fs::write(&shot, b"\x89PNG fake").unwrap();
    let photo = dir.join("photo.JPG");
    std::fs::write(&photo, b"jpeg fake").unwrap();
    PackageInput {
        title: "Q3 plan: <review> & next steps".into(),
        kind: RecordingKind::Meeting,
        when: "Tuesday 21 October 2026, 14:00".into(),
        duration: Some("42 min".into()),
        notebook: Some("Planning".into()),
        people: vec!["Ana Ruiz".into(), "Ben Ode".into()],
        notes: Some(NotesPart::Structured {
            summary: Some("We agreed to ship *beta* in May.".into()),
            topics: vec!["Budget".into(), " ".into()],
            decisions: vec!["Ship beta in May".into()],
            actions: vec!["Draft the plan (Ana, Friday)".into()],
        }),
        highlights: vec![Highlight { ms: 723_000, label: "Beta date agreed".into(), quote: Some("let's ship the beta in May".into()) }],
        screens: vec![
            ScreenPart { ms: 61_000, source: Source::Screen, caption: Some("The roadmap slide".into()), path: shot },
            ScreenPart { ms: 3_725_000, source: Source::Photo, caption: None, path: photo },
        ],
        marks: vec![MarkPart { ms: 90_000, symbol: "★", label: "Important", note: Some("budget".into()) }],
        links: vec![
            LinkPart { url: "https://example.com/plan".into(), title: Some("The plan".into()), first_ms: Some(120_000) },
            LinkPart { url: "javascript:alert(1)".into(), title: None, first_ms: None },
        ],
        email: Some(EmailPart { subject: "Recap".into(), body: "Hi all,\n\nThanks.".into() }),
        guide_md: Some("# Review guide\n".into()),
        flashcards_csv: Some("front,back\n".into()),
        transcript: vec![line(0, Some("Ana"), "Hello"), line(65_000, None, "[stricken from the record] ok")],
        has_ai: true,
    }
}

#[test]
fn clock_formats_minutes_and_hours() {
    assert_eq!(clock(0), "0:00");
    assert_eq!(clock(61_000), "1:01");
    assert_eq!(clock(3_725_000), "1:02:05");
    assert_eq!(clock(-5), "0:00");
}

#[test]
fn folder_and_image_names_are_safe_and_ordered() {
    assert_eq!(folder_name("2026-10-21", "Q3 plan: review/next"), "2026-10-21 Q3 plan review next");
    assert_eq!(folder_name("", "???"), "Recording");
    let s = ScreenPart { ms: 3_725_000, source: Source::Photo, caption: None, path: "/x/IMG.HEIC".into() };
    assert_eq!(image_name(2, &s), "screens/03-photo-1-02-05.heic");
    let s = ScreenPart { ms: 1000, source: Source::Screen, caption: None, path: "/x/noext".into() };
    assert_eq!(image_name(0, &s), "screens/01-screen-0-01.jpg");
}

#[test]
fn markdown_has_every_part_escaped_and_in_order() {
    let dir = tmp_dir("md");
    let md = markdown(&sample_input(&dir));
    assert!(md.starts_with("# Q3 plan: \\<review\\> & next steps\n"));
    assert!(md.contains("Meeting · Tuesday 21 October 2026, 14:00 · 42 min"));
    assert!(md.contains("People: Ana Ruiz, Ben Ode"));
    assert!(md.contains("We agreed to ship \\*beta\\* in May."));
    let order = ["## Summary", "## Highlights", "## Key topics", "## Decisions", "## Action items", "## Screens and photos", "## Marked moments", "## Links", "## Follow-up email", "## Also in this package"];
    let pos: Vec<usize> = order.iter().map(|h| md.find(h).unwrap_or_else(|| panic!("{} missing", h))).collect();
    assert!(pos.windows(2).all(|w| w[0] < w[1]), "sections out of order");
    assert!(md.contains("- **12:03** Beta date agreed: “let's ship the beta in May”"));
    assert!(md.contains("![The roadmap slide](screens/01-screen-1-01.png)"));
    assert!(md.contains("![photo at 1:02:05](screens/02-photo-1-02-05.jpg)"));
    assert!(md.contains("- [The plan](<https://example.com/plan>) (2:00)"));
    assert!(md.contains("[Transcript](transcript.txt)"));
    assert!(md.contains("Parts were written by AI"));
    // Blank topics are dropped
    assert!(!md.contains("- \n"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn summary_keeps_its_paragraphs_and_ai_markdown_is_inert() {
    let mut p = PackageInput { title: "T".into(), ..Default::default() };
    p.notes = Some(NotesPart::Structured { summary: Some("First para.\nSecond line.\n\nNext para.".into()), topics: vec![], decisions: vec![], actions: vec![] });
    assert!(markdown(&p).contains("## Summary\n\nFirst para.  \nSecond line.\n\nNext para.\n\n"));
    p.notes = Some(NotesPart::Markdown("## Plan\n<img src=\"https://x.example/t.png\">\n![pixel](https://x.example/p.png)\n- see [the doc](https://example.com)".into()));
    let md = markdown(&p);
    assert!(md.contains("\\<img src=\"https://x.example/t.png\"\\>"), "{}", md);
    assert!(md.contains("\\![pixel](https://x.example/p.png)"));
    assert!(md.contains("- see [the doc](https://example.com)"), "links stay links");
    assert!(!md.contains("\n<img"));
}

#[test]
fn synced_iphone_notes_render_under_notes() {
    let mut p = PackageInput { title: "Walk".into(), ..Default::default() };
    p.notes = Some(NotesPart::Markdown("## Plan\n- one\n**Next:**\nCall **Ana**".into()));
    let md = markdown(&p);
    assert!(md.contains("## Notes\n\n#### Plan\n- one\n"));
    let html = html(&p);
    assert!(html.contains("<h3>Plan</h3>"));
    assert!(html.contains("<li>one</li>"));
    assert!(html.contains("<h3>Next</h3>"));
    assert!(html.contains("<p>Call <strong>Ana</strong></p>"));
}

#[test]
fn html_escapes_everything_and_only_links_web_urls() {
    let dir = tmp_dir("html");
    let h = html(&sample_input(&dir));
    assert!(h.contains("<title>Q3 plan: &lt;review&gt; &amp; next steps</title>"));
    assert!(!h.contains("<review>"));
    assert!(h.contains("<a href=\"https://example.com/plan\">The plan</a>"));
    assert!(!h.contains("href=\"javascript"), "only http(s) links become anchors");
    assert!(h.contains("<img src=\"screens/01-screen-1-01.png\" alt=\"The roadmap slide\">"));
    assert!(!h.contains("<script"));
    assert!(!h.contains("http://") || h.contains("https://example.com"), "no outside requests");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn simple_markdown_escapes_html() {
    let out = simple_markdown_html("<img src=x onerror=alert(1)>\n- **<b>**");
    assert!(out.contains("&lt;img src=x onerror=alert(1)&gt;"));
    assert!(out.contains("<li><strong>&lt;b&gt;</strong></li>"));
}

#[test]
fn transcript_text_has_times_and_speakers() {
    let dir = tmp_dir("tx");
    let t = transcript_text(&sample_input(&dir));
    assert!(t.contains("[0:00] Ana: Hello\n"));
    assert!(t.contains("[1:05] [stricken from the record] ok\n"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn package_lists_files_and_skips_empty_parts() {
    let dir = tmp_dir("pkg");
    let p = sample_input(&dir);
    let pkg = package(&p, "2026-10-21");
    assert_eq!(pkg.folder, "2026-10-21 Q3 plan review next steps");
    let names: Vec<&str> = pkg.files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "Q3 plan review next steps.md",
            "Q3 plan review next steps.html",
            "screens/01-screen-1-01.png",
            "screens/02-photo-1-02-05.jpg",
            "transcript.txt",
            "Review guide.md",
            "flashcards.csv",
        ]
    );
    let bare = PackageInput { title: "Solo".into(), ..Default::default() };
    let names: Vec<String> = package(&bare, "").files.into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, vec!["Solo.md".to_string(), "Solo.html".to_string()]);
    assert!(!markdown(&bare).contains("## Also in this package"));
    assert!(markdown(&bare).ends_with("_Made with noFriction._\n"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn writes_a_folder_and_a_zip_with_the_same_files() {
    let dir = tmp_dir("write");
    let p = sample_input(&dir);
    let pkg = package(&p, "2026-10-21");

    let out = unique_child(&dir, &pkg.folder, "");
    write_folder(&pkg, &out).unwrap();
    assert_eq!(std::fs::read(out.join("screens/01-screen-1-01.png")).unwrap(), b"\x89PNG fake");
    assert!(std::fs::read_to_string(out.join("transcript.txt")).unwrap().contains("Hello"));
    // A second export doesn't overwrite the first
    let again = unique_child(&dir, &pkg.folder, "");
    assert_eq!(again.file_name().unwrap().to_string_lossy(), format!("{} 2", pkg.folder));

    let buf = write_zip(&pkg, std::io::Cursor::new(Vec::new())).unwrap().into_inner();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(buf)).unwrap();
    assert_eq!(zip.len(), pkg.files.len());
    let mut names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().to_string()).collect();
    names.sort();
    assert!(names.iter().all(|n| n.starts_with("2026-10-21 Q3 plan review next steps/")));
    let mut s = String::new();
    zip.by_name("2026-10-21 Q3 plan review next steps/flashcards.csv").unwrap().read_to_string(&mut s).unwrap();
    assert_eq!(s, "front,back\n");
    let img = zip.by_name("2026-10-21 Q3 plan review next steps/screens/02-photo-1-02-05.jpg").unwrap();
    assert_eq!(img.compression(), zip::CompressionMethod::Stored);
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn write_refuses_paths_that_escape_the_folder() {
    let dir = tmp_dir("escape");
    let pkg = Package { folder: "x".into(), files: vec![("../evil.txt".into(), FileData::Text("no".into()))] };
    assert!(write_folder(&pkg, &dir.join("x")).is_err());
    assert!(write_zip(&pkg, std::io::Cursor::new(Vec::new())).is_err());
    assert!(!dir.join("evil.txt").exists());
    std::fs::remove_dir_all(dir).ok();
}

// ─── AI ──────────────────────────────────────────────────────────────────

#[test]
fn parts_group_lines_and_cap_their_count() {
    let lines: Vec<TranscriptLine> = (0..10).map(|i| line(i * 1000, Some("A"), "short words here")).collect();
    let ps = parts(&lines);
    assert_eq!(ps.len(), 1);
    assert!(ps[0].text.starts_with("A: short words here A: short"));

    let long: Vec<TranscriptLine> = (0..5000).map(|i| line(i * 1000, None, &"word ".repeat(30))).collect();
    let ps = parts(&long);
    assert!(ps.len() <= MAX_PARTS, "{} parts", ps.len());
    assert!(ps.windows(2).all(|w| w[0].ms < w[1].ms));
    assert!(parts(&[line(0, None, "  ")]).is_empty());
    // A struck line or screen is a marker, not something anyone said
    assert!(parts(&[line(0, None, "[stricken from the record]"), line(1, None, " [screen stricken from the record] ")]).is_empty());
    assert_eq!(parts(&[line(0, None, "it was stricken from the record yesterday")]).len(), 1);

    let prompt = highlights_prompt("T", &ps);
    assert!(prompt.chars().count() < HIGHLIGHT_BUDGET + 20 * MAX_PARTS + 200);
    assert!(prompt.contains("[1] (0:00) "));
}

#[test]
fn highlights_keep_only_real_quotes_and_valid_parts() {
    let ps = vec![
        Part { ms: 0, text: "Ana: Let's ship the beta in May, everyone agrees.".into() },
        Part { ms: 60_000, text: "Ben: The budget is [stricken from the record] for now.".into() },
        Part { ms: 120_000, text: "Ana: I'll draft the plan by Friday.".into() },
    ];
    let raw = r#"Sure! ```json
    {"highlights":[
      {"part":3,"label":"Ana owns the plan","quote":"I'll draft the plan by Friday"},
      {"part":1,"label":"- Beta date","quote":"“let’s ship the beta in may”"},
      {"part":1,"label":"duplicate","quote":"x"},
      {"part":2,"label":"Budget","quote":"The budget is [stricken from the record]"},
      {"part":9,"label":"out of range"},
      {"part":"2","label":"Invented quote","quote":"the budget is ten million dollars"},
    ]}
    ```"#;
    let hs = parse_highlights(raw, &ps).unwrap();
    assert_eq!(hs.len(), 3);
    assert_eq!(hs[0].ms, 0);
    assert_eq!(hs[0].label, "Beta date");
    assert_eq!(hs[0].quote.as_deref(), Some("let’s ship the beta in may"), "the model's quote marks are dropped (the package adds its own)");
    assert_eq!(hs[1].ms, 60_000);
    assert_eq!(hs[1].quote, None, "a stricken quote is dropped");
    assert_eq!(hs[2].quote.as_deref(), Some("I'll draft the plan by Friday"));
    assert!(parse_highlights(r#"{"highlights":[{"part":7,"label":"x"}]}"#, &ps).is_err());
    assert!(parse_highlights("no json", &ps).is_err());
}

#[test]
fn said_around_uses_the_time_window() {
    let lines = vec![line(0, None, "too early"), line(50_000, None, "intro"), line(90_000, None, "the slide"), line(400_000, None, "too late")];
    let p = Picture { ms: 80_000, end_ms: Some(100_000), source: "screen", app: Some("Keynote".into()), window: Some("Q3".into()) };
    assert_eq!(said_around(&p, &lines, 500), "intro the slide");
    assert_eq!(said_around(&p, &lines, 5), "intro");
    let quiet = Picture { ms: 900_000, end_ms: None, source: "photo", app: None, window: None };
    assert_eq!(said_around(&quiet, &lines, 500), "(nothing was said)");
    let prompt = pictures_prompt("T", &[p, quiet], &lines, 500);
    assert!(prompt.contains("[1] screen at 1:20, Keynote — Q3\nSaid around then: intro the slide"));
    assert!(prompt.contains("[2] photo at 15:00\n"));
}

#[test]
fn sample_spreads_evenly() {
    assert_eq!(sample(3, 10), vec![0, 1, 2]);
    assert_eq!(sample(100, 5), vec![0, 24, 49, 74, 99]);
    assert_eq!(sample(10, 1), vec![0]);
}

#[test]
fn captions_parse_numbers_once_each() {
    let raw = r#"{"captions":[{"picture":2,"caption":"The roadmap"},{"picture":"1","caption":"Team photo"},{"picture":2,"caption":"dup"},{"picture":5,"caption":"nope"},{"picture":3,"caption":""}]}"#;
    assert_eq!(parse_captions(raw, 3, "captions").unwrap(), vec![(0, "Team photo".to_string()), (1, "The roadmap".to_string())]);
    let picks = r#"{"picks":[{"picture":3,"caption":"Budget chart"}]}"#;
    assert_eq!(parse_captions(picks, 3, "picks").unwrap(), vec![(2, "Budget chart".to_string())]);
    assert!(parse_captions(r#"{"captions":[]}"#, 3, "captions").is_err());
    assert!(picks_system(4).contains("at most 4 pictures"));
}

// ─── From the database ───────────────────────────────────────────────────

mod db {
    use super::super::build::{self, FileData};
    use super::super::commands::{draft, package_input, EmailDto, ExportRequest, HighlightDto, Include, PickedItem};
    use crate::database::DatabaseManager;
    use std::io::Read;

    struct Fx {
        db: DatabaseManager,
        dir: std::path::PathBuf,
    }
    impl Drop for Fx {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    async fn fx() -> Fx {
        let dir = std::env::temp_dir().join(format!("nf-export-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = DatabaseManager::new(&dir.join("t.db")).await.unwrap();
        db.run_migrations().await.unwrap();
        db.create_meeting("m1", "Weekly sync").await.unwrap();
        Fx { db, dir }
    }

    async fn screen(f: &Fx, id: &str, secs: i64, kind: &str, file: Option<&str>, app: Option<&str>) {
        let t0 = f.db.get_meeting("m1").await.unwrap().unwrap().started_at;
        let path = file.map(|n| {
            let p = f.dir.join(n);
            std::fs::write(&p, format!("image {}", n)).unwrap();
            p.display().to_string()
        });
        sqlx::query(
            "INSERT INTO screen_states (state_id, meeting_id, start_ts, end_ts, app_name, window_title, phash, keyframe_path, state_type) VALUES (?, 'm1', ?, NULL, ?, ?, 'x', ?, ?)",
        )
        .bind(id)
        .bind((t0 + chrono::Duration::seconds(secs)).to_rfc3339())
        .bind(app)
        .bind(app.map(|_| "Q3 plan"))
        .bind(path)
        .bind(kind)
        .execute(f.db.pool())
        .await
        .unwrap();
    }

    fn all() -> Include {
        Include { notes: true, highlights: true, screens: true, marks: true, links: true, email: true, guide: true, transcript: true }
    }

    #[tokio::test]
    async fn draft_lists_pictures_with_files_in_time_order() {
        let f = fx().await;
        let t0 = f.db.get_meeting("m1").await.unwrap().unwrap().started_at;
        f.db.add_transcript_at("m1", "hello there", Some("Ana"), true, 0.9, t0 + chrono::Duration::seconds(5)).await.unwrap();
        screen(&f, "s-late", 120, "slide", Some("late.png"), Some("Keynote")).await;
        screen(&f, "p-early", 30, "photo", Some("early.jpg"), None).await;
        screen(&f, "s-nofile", 60, "slide", None, None).await;
        screen(&f, "s-missing", 90, "slide", Some("gone.png"), None).await;
        std::fs::remove_file(f.dir.join("gone.png")).unwrap();

        let d = draft(&f.db, "m1").await.unwrap();
        assert_eq!(d.title, "Weekly sync");
        assert_eq!(d.kind, "meeting");
        assert_eq!(d.transcript_lines, 1);
        assert!(d.notes.is_none());
        let ids: Vec<&str> = d.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["p-early", "s-late"], "only pictures with a file, in time order");
        assert_eq!(d.items[0].source, "photo");
        assert_eq!(d.items[1].label.as_deref(), Some("Keynote — Q3 plan"));
        assert!(d.folder.ends_with(" Weekly sync"));
        assert!(draft(&f.db, "nope").await.is_err());
    }

    #[tokio::test]
    async fn package_reads_fresh_data_and_renders_stricken_text_as_the_marker() {
        let f = fx().await;
        let t0 = f.db.get_meeting("m1").await.unwrap().unwrap().started_at;
        f.db.add_transcript_at("m1", "we ship in May", Some("Ana"), true, 0.9, t0 + chrono::Duration::seconds(5)).await.unwrap();
        // A line with a strike marker (redaction.rs token) exports as the placeholder
        let marked = "the code is ⟦strickenid0123456789abcdef0123456789abcdef⟧";
        f.db.add_transcript_at("m1", marked, None, true, 0.9, t0 + chrono::Duration::seconds(65)).await.unwrap();
        screen(&f, "s1", 120, "slide", Some("one.png"), Some("Keynote")).await;
        screen(&f, "p1", 30, "photo", Some("two.jpg"), None).await;
        f.db.save_meeting_notes("n1", "m1", Some("We agreed on May."), Some(r#"["Launch"]"#), Some(r#"[{"text":"Ship in May"}]"#), Some(r#"[{"task":"Draft plan","assignee":"Ana"}]"#), None, Some("default"))
            .await
            .unwrap();

        let req = ExportRequest {
            include: all(),
            // The user's order is kept; an id from another recording is ignored
            items: vec![
                PickedItem { id: "s1".into(), caption: Some("  The plan  ".into()), ai: true },
                PickedItem { id: "p1".into(), caption: Some("".into()), ai: false },
                PickedItem { id: "elsewhere".into(), caption: None, ai: false },
            ],
            highlights: vec![HighlightDto { ms: 5000, label: "Date set".into(), quote: Some("we ship in May".into()) }, HighlightDto { ms: 1, label: "  ".into(), quote: None }],
            email: Some(EmailDto { subject: "Recap".into(), body: "Thanks all.".into() }),
            has_ai: false,
            basis: Some(draft(&f.db, "m1").await.unwrap().fingerprint),
        };
        let (input, date) = package_input(&f.db, "m1", &req).await.unwrap();
        assert!(input.has_ai, "saved notes are AI output");
        assert_eq!(input.screens.len(), 2);
        assert_eq!(input.screens[0].caption.as_deref(), Some("The plan"));
        assert_eq!(input.screens[1].caption, None);
        assert_eq!(input.highlights.len(), 1, "blank highlights are dropped");
        assert_eq!(input.transcript.len(), 2);
        assert!(!input.transcript[1].text.contains("strickenid"), "{}", input.transcript[1].text);
        assert!(input.transcript[1].text.contains(crate::redaction::STRICKEN_PLACEHOLDER));
        match input.notes.as_ref().unwrap() {
            build::NotesPart::Structured { summary, topics, decisions, actions } => {
                assert_eq!(summary.as_deref(), Some("We agreed on May."));
                assert_eq!(topics, &vec!["Launch".to_string()]);
                assert_eq!(decisions, &vec!["Ship in May".to_string()]);
                assert_eq!(actions, &vec!["Draft plan (Ana)".to_string()]);
            }
            other => panic!("{:?}", other),
        }

        let pkg = build::package(&input, &date);
        let buf = build::write_zip(&pkg, std::io::Cursor::new(Vec::new())).unwrap().into_inner();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(buf)).unwrap();
        let root = format!("{}/", pkg.folder);
        let mut md = String::new();
        zip.by_name(&format!("{}Weekly sync.md", root)).unwrap().read_to_string(&mut md).unwrap();
        assert!(md.contains("## Summary\n\nWe agreed on May."));
        assert!(md.contains("![The plan](screens/01-screen-2-00.png)"));
        assert!(md.contains("screens/02-photo-0-30.jpg"));
        assert!(md.contains("## Follow-up email"));
        let mut img = Vec::new();
        zip.by_name(&format!("{}screens/01-screen-2-00.png", root)).unwrap().read_to_end(&mut img).unwrap();
        assert_eq!(img, b"image one.png");
        let mut tx = String::new();
        zip.by_name(&format!("{}transcript.txt", root)).unwrap().read_to_string(&mut tx).unwrap();
        assert!(tx.contains("[0:05] Ana: we ship in May"));
        assert!(!tx.contains("strickenid"));

        // Nothing ticked: just the document and the page
        let none = ExportRequest { include: Include::default(), items: req.items.clone(), highlights: vec![], email: None, has_ai: false, basis: None };
        let (bare, _) = package_input(&f.db, "m1", &none).await.unwrap();
        assert!(!bare.has_ai);
        let files: Vec<String> = build::package(&bare, &date).files.into_iter().filter(|(_, d)| matches!(d, FileData::Copy(_))).map(|(n, _)| n).collect();
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn ai_parts_from_an_older_transcript_are_refused() {
        let f = fx().await;
        let t0 = f.db.get_meeting("m1").await.unwrap().unwrap().started_at;
        f.db.add_transcript_at("m1", "the budget is four million", None, true, 0.9, t0 + chrono::Duration::seconds(5)).await.unwrap();
        let before = draft(&f.db, "m1").await.unwrap().fingerprint;
        let req = |basis: Option<String>, highlights: Vec<HighlightDto>| ExportRequest {
            include: all(),
            items: vec![],
            highlights,
            email: None,
            has_ai: false,
            basis,
        };
        let h = || vec![HighlightDto { ms: 5000, label: "Budget".into(), quote: Some("the budget is four million".into()) }];
        assert!(package_input(&f.db, "m1", &req(Some(before.clone()), h())).await.is_ok());

        // A Strike (here or over Sync) rewrites the line: the old highlight would quote it
        sqlx::query("UPDATE transcripts SET text = 'the budget is ⟦strickenid0123456789abcdef0123456789abcdef⟧' WHERE meeting_id = 'm1'")
            .execute(f.db.pool())
            .await
            .unwrap();
        let err = package_input(&f.db, "m1", &req(Some(before.clone()), h())).await.unwrap_err();
        assert!(err.starts_with("TRANSCRIPT_CHANGED:"), "{}", err);
        // Without AI parts there's nothing stale to refuse
        assert!(package_input(&f.db, "m1", &req(Some(before), vec![])).await.is_ok());
        let after = draft(&f.db, "m1").await.unwrap().fingerprint;
        assert!(package_input(&f.db, "m1", &req(Some(after), h())).await.is_ok());

        // A Delete still in its undo window: the line is hidden in the app
        // but still in the table, so nothing is exported or sent until it lands
        sqlx::query("INSERT INTO redactions (id, meeting_id, kind, action, created_at, pending_payload) VALUES ('r9', 'm1', 'words', 'delete', datetime('now'), '{}')")
            .execute(f.db.pool())
            .await
            .unwrap();
        let err = package_input(&f.db, "m1", &req(None, vec![])).await.unwrap_err();
        assert_eq!(err, crate::study::PENDING_EDIT);
    }
}
