-- Schema of a real database created by an older noFriction build (taken
-- with sqlite3 .schema; DDL only, no data). text_snapshots here has no
-- meeting_id / app_name / window_title, which newer code relies on.
-- Used by schema_drift_tests.rs. sqlite_sequence is created by SQLite.
CREATE TABLE meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER
            , calendar_event_id TEXT);
CREATE TABLE transcripts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                text TEXT NOT NULL,
                speaker TEXT,
                timestamp TEXT NOT NULL,
                is_final INTEGER NOT NULL DEFAULT 1,
                confidence REAL NOT NULL DEFAULT 0.0
            , text_hash TEXT, word_timings TEXT);
CREATE TABLE frames (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
                frame_number INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL,
                file_path TEXT,
                ocr_text TEXT
            );
CREATE VIRTUAL TABLE transcripts_fts 
            USING fts5(text, meeting_id, content='transcripts', content_rowid='id')
/* transcripts_fts(text,meeting_id) */;
CREATE TABLE IF NOT EXISTS 'transcripts_fts_data'(id INTEGER PRIMARY KEY, block BLOB);
CREATE TABLE IF NOT EXISTS 'transcripts_fts_idx'(segid, term, pgno, PRIMARY KEY(segid, term)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS 'transcripts_fts_docsize'(id INTEGER PRIMARY KEY, sz BLOB);
CREATE TABLE IF NOT EXISTS 'transcripts_fts_config'(k PRIMARY KEY, v) WITHOUT ROWID;
CREATE TRIGGER transcripts_ai AFTER INSERT ON transcripts BEGIN
                INSERT INTO transcripts_fts(rowid, text, meeting_id) 
                VALUES (new.id, new.text, new.meeting_id);
            END;
CREATE TRIGGER transcripts_ad AFTER DELETE ON transcripts BEGIN
                INSERT INTO transcripts_fts(transcripts_fts, rowid, text, meeting_id) 
                VALUES ('delete', old.id, old.text, old.meeting_id);
            END;
CREATE INDEX idx_transcripts_meeting ON transcripts(meeting_id);
CREATE INDEX idx_meetings_started ON meetings(started_at);
CREATE TABLE frame_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                frame_id INTEGER REFERENCES frames(id) ON DELETE CASCADE,
                frame_path TEXT NOT NULL,
                captured_at TEXT NOT NULL,
                analyzed INTEGER NOT NULL DEFAULT 0,
                synced INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
CREATE TABLE activity_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time TEXT NOT NULL,
                end_time TEXT,
                duration_seconds INTEGER,
                app_name TEXT,
                window_title TEXT,
                category TEXT NOT NULL DEFAULT 'other',
                summary TEXT NOT NULL,
                focus_area TEXT,
                visible_files TEXT,
                confidence REAL DEFAULT 0.0,
                frame_ids TEXT,
                pinecone_id TEXT,
                supabase_id TEXT,
                synced_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
CREATE INDEX idx_frame_queue_analyzed ON frame_queue(analyzed);
CREATE INDEX idx_frame_queue_synced ON frame_queue(synced);
CREATE INDEX idx_activity_log_start ON activity_log(start_time);
CREATE INDEX idx_activity_log_category ON activity_log(category);
CREATE TABLE theme_sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                theme TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_seconds INTEGER,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
CREATE INDEX idx_theme_sessions_theme ON theme_sessions(theme);
CREATE INDEX idx_theme_sessions_started ON theme_sessions(started_at);
CREATE TABLE entities (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                activity_id INTEGER NOT NULL,
                entity_type TEXT NOT NULL,
                name TEXT NOT NULL,
                metadata TEXT,
                confidence REAL DEFAULT 0.5,
                theme TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (activity_id) REFERENCES activity_log(id) ON DELETE CASCADE
            );
CREATE INDEX idx_entities_type ON entities(entity_type);
CREATE INDEX idx_entities_theme ON entities(theme);
CREATE INDEX idx_entities_activity ON entities(activity_id);
CREATE TABLE screen_states (
                state_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                start_ts TEXT NOT NULL,
                end_ts TEXT,
                app_name TEXT,
                window_title TEXT,
                phash TEXT NOT NULL,
                delta_score REAL DEFAULT 0.0,
                keyframe_path TEXT,
                state_type TEXT DEFAULT 'other',
                flags TEXT DEFAULT '{}',
                created_at TEXT DEFAULT (datetime('now')), source_key TEXT,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
CREATE INDEX idx_screen_states_meeting ON screen_states(meeting_id);
CREATE INDEX idx_screen_states_start ON screen_states(start_ts);
CREATE TABLE document_episodes (
                episode_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                start_ts TEXT NOT NULL,
                end_ts TEXT,
                app_name TEXT,
                window_title TEXT,
                document_fingerprint TEXT,
                state_count INTEGER DEFAULT 0,
                total_duration_ms INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
CREATE INDEX idx_episodes_meeting ON document_episodes(meeting_id);
CREATE INDEX idx_episodes_app ON document_episodes(app_name);
CREATE TABLE episode_states (
                episode_id TEXT NOT NULL,
                state_id TEXT NOT NULL,
                sequence_num INTEGER DEFAULT 0,
                PRIMARY KEY (episode_id, state_id),
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE CASCADE,
                FOREIGN KEY (state_id) REFERENCES screen_states(state_id) ON DELETE CASCADE
            );
CREATE TABLE text_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                episode_id TEXT,
                state_id TEXT,
                ts TEXT NOT NULL,
                text TEXT NOT NULL,
                text_hash TEXT NOT NULL,
                quality_score REAL DEFAULT 0.0,
                source TEXT DEFAULT 'ocr',
                word_count INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE CASCADE,
                FOREIGN KEY (state_id) REFERENCES screen_states(state_id) ON DELETE CASCADE
            );
CREATE INDEX idx_snapshots_episode ON text_snapshots(episode_id);
CREATE INDEX idx_snapshots_hash ON text_snapshots(text_hash);
CREATE TABLE text_patches (
                patch_id TEXT PRIMARY KEY,
                episode_id TEXT NOT NULL,
                from_snapshot_id TEXT,
                to_snapshot_id TEXT,
                from_text_hash TEXT NOT NULL,
                to_text_hash TEXT NOT NULL,
                ts TEXT NOT NULL,
                unified_diff TEXT NOT NULL,
                lines_added INTEGER DEFAULT 0,
                lines_removed INTEGER DEFAULT 0,
                change_summary TEXT,
                change_type TEXT DEFAULT 'content_changed',
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE CASCADE
            );
CREATE INDEX idx_patches_episode ON text_patches(episode_id);
CREATE TABLE meeting_timeline_events (
                event_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                ts TEXT NOT NULL,
                event_type TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT,
                app_name TEXT,
                window_title TEXT,
                duration_ms INTEGER,
                episode_id TEXT,
                state_id TEXT,
                topic TEXT,
                importance REAL DEFAULT 0.5,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
                FOREIGN KEY (episode_id) REFERENCES document_episodes(episode_id) ON DELETE SET NULL
            );
CREATE INDEX idx_timeline_meeting ON meeting_timeline_events(meeting_id);
CREATE INDEX idx_timeline_ts ON meeting_timeline_events(ts);
CREATE INDEX idx_timeline_topic ON meeting_timeline_events(topic);
CREATE TABLE topic_clusters (
                topic_id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT,
                start_ts TEXT NOT NULL,
                end_ts TEXT,
                event_count INTEGER DEFAULT 0,
                total_duration_ms INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
CREATE INDEX idx_topics_meeting ON topic_clusters(meeting_id);
CREATE TABLE audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                target_type TEXT NOT NULL,
                target_id TEXT NOT NULL,
                details TEXT,
                bytes_affected INTEGER DEFAULT 0,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );
CREATE INDEX idx_audit_action ON audit_log(action);
CREATE INDEX idx_audit_timestamp ON audit_log(timestamp);
CREATE TABLE data_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                field_name TEXT NOT NULL,
                previous_value TEXT,
                new_value TEXT,
                diff TEXT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );
CREATE INDEX idx_versions_entity ON data_versions(entity_type, entity_id);
CREATE TABLE settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT DEFAULT CURRENT_TIMESTAMP
            );
CREATE TABLE prompt_library (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                category TEXT NOT NULL DEFAULT 'general',
                system_prompt TEXT NOT NULL,
                user_prompt_template TEXT,
                model_id TEXT,
                temperature REAL NOT NULL DEFAULT 0.5,
                max_tokens INTEGER,
                theme TEXT,
                version INTEGER NOT NULL DEFAULT 1,
                is_builtin BOOLEAN NOT NULL DEFAULT 0,
                is_active BOOLEAN NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
CREATE INDEX idx_prompts_theme ON prompt_library(theme);
CREATE INDEX idx_prompts_version ON prompt_library(name, version DESC);
CREATE TABLE model_configurations (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                model_type TEXT NOT NULL DEFAULT 'llm',
                base_url TEXT NOT NULL DEFAULT 'http://localhost:8080',
                capabilities TEXT,
                default_temperature REAL NOT NULL DEFAULT 0.5,
                default_max_tokens INTEGER NOT NULL DEFAULT 2048,
                is_available BOOLEAN NOT NULL DEFAULT 0,
                last_health_check TEXT,
                created_at TEXT NOT NULL
            );
CREATE TABLE use_case_mappings (
                id TEXT PRIMARY KEY,
                use_case TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                description TEXT,
                prompt_id TEXT REFERENCES prompt_library(id),
                model_id TEXT REFERENCES model_configurations(id),
                priority INTEGER NOT NULL DEFAULT 0,
                conditions TEXT,
                is_active BOOLEAN NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );
CREATE TABLE meeting_notes (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                summary TEXT,
                key_topics TEXT,       -- JSON array of topics
                decisions TEXT,        -- JSON array of decisions
                action_items TEXT,     -- JSON array of action items
                participants TEXT,     -- JSON array of detected participants
                generated_at TEXT NOT NULL DEFAULT (datetime('now')),
                model_used TEXT, stale_after_edit INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
CREATE INDEX idx_meeting_notes_meeting ON meeting_notes(meeting_id);
CREATE TABLE meeting_comments (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                user_id TEXT,
                comment TEXT NOT NULL,
                comment_type TEXT DEFAULT 'note',  -- 'note', 'decision', 'action', 'question'
                timestamp_ref REAL,                -- Optional: reference to transcript timestamp
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT,
                parent_id TEXT,                    -- For threaded comments
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
                FOREIGN KEY (parent_id) REFERENCES meeting_comments(id) ON DELETE CASCADE
            );
CREATE INDEX idx_meeting_comments_meeting ON meeting_comments(meeting_id);
CREATE TABLE study_materials (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                summary TEXT,
                key_concepts TEXT,     -- JSON array of {term, definition}
                quiz_questions TEXT,   -- JSON array of quiz questions
                flashcards TEXT,       -- JSON array of flashcards
                generated_at TEXT NOT NULL DEFAULT (datetime('now')),
                model_used TEXT, stale_after_edit INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
CREATE INDEX idx_study_materials_meeting ON study_materials(meeting_id);
CREATE TABLE transcript_clusters (
                id TEXT PRIMARY KEY,
                meeting_id TEXT NOT NULL,
                cluster_name TEXT,
                start_time TEXT,
                end_time TEXT,
                transcript_ids TEXT,   -- JSON array of transcript IDs in this cluster
                auto_generated INTEGER DEFAULT 1,
                confidence REAL DEFAULT 0.0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
CREATE INDEX idx_transcript_clusters_meeting ON transcript_clusters(meeting_id);
CREATE INDEX idx_snapshots_ts ON text_snapshots(ts);
CREATE TABLE meeting_attendees (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                meeting_id TEXT NOT NULL,
                name TEXT NOT NULL,
                email TEXT NOT NULL,
                company TEXT,
                role TEXT NOT NULL DEFAULT 'attendee',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
            );
CREATE INDEX idx_attendees_meeting ON meeting_attendees(meeting_id);
CREATE INDEX idx_attendees_email ON meeting_attendees(email);
CREATE INDEX idx_transcripts_meeting_hash ON transcripts(meeting_id, text_hash);
CREATE TABLE people (
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
        );
CREATE TABLE meeting_details (
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
        );
CREATE UNIQUE INDEX idx_attendees_meeting_email ON meeting_attendees(meeting_id, email);
CREATE TABLE assistant_conversations (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                user_query TEXT NOT NULL,
                assistant_response TEXT NOT NULL,
                model_used TEXT NOT NULL DEFAULT '',
                context_refs TEXT NOT NULL DEFAULT '[]'
            , stale_after_edit INTEGER NOT NULL DEFAULT 0);
CREATE INDEX idx_assistant_conversations_ts ON assistant_conversations(timestamp);
CREATE TRIGGER transcripts_au AFTER UPDATE OF text, meeting_id ON transcripts BEGIN
                INSERT INTO transcripts_fts(transcripts_fts, rowid, text, meeting_id)
                VALUES ('delete', old.id, old.text, old.meeting_id);
                INSERT INTO transcripts_fts(rowid, text, meeting_id)
                VALUES (new.id, new.text, new.meeting_id);
            END;
CREATE TABLE redactions (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK (kind IN ('words', 'line', 'screen')),
            action TEXT NOT NULL CHECK (action IN ('delete', 'strike')),
            media_start TEXT,
            media_end TEXT,
            created_at TEXT NOT NULL,
            reason TEXT,
            -- Mac extras: which line holds the marker; how many screens
            transcript_id INTEGER,
            item_count INTEGER NOT NULL DEFAULT 1,
            -- Delete only, during the 5s undo window: ids/offsets (never
            -- content). The row is removed when the delete commits.
            pending_payload TEXT
        , failed_at TEXT, failure TEXT);
CREATE INDEX idx_redactions_meeting ON redactions(meeting_id);
CREATE TRIGGER redactions_strike_no_update
        BEFORE UPDATE ON redactions WHEN old.action = 'strike'
        BEGIN SELECT RAISE(ABORT, 'stricken-from-the-record markers cannot be edited'); END;
CREATE TRIGGER redactions_strike_no_delete
        BEFORE DELETE ON redactions
        WHEN old.action = 'strike' AND EXISTS (SELECT 1 FROM meetings WHERE id = old.meeting_id)
        BEGIN SELECT RAISE(ABORT, 'stricken-from-the-record markers cannot be removed'); END;
