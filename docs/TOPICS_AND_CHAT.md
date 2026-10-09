# Topics and Chat: shared spec (Mac + iOS)

Two AI features on top of recordings, with the same words on both platforms:

- **Topics**: what a recording was about, as 1–4 short noun phrases
  ("Q4 roadmap", "Mitosis"). Named by the AI when notes are made (and on
  demand with **Find topics**); the user can rename, remove and add. On the
  Mac a **Topics** chip row sits beside **Notebooks** in the recordings list
  and **Group by: Date · Notebook · Topic** regroups the list; on iOS topics
  are a search facet and live on the recording (Notebooks are the one
  filter, the list is by day).
- **Chat**: ask about your recordings. A scope at the top, **All recordings
  · this Notebook · this Topic · this recording**, decides which recordings
  are searched. Answers are Markdown with `[n]` citations; each citation
  opens that recording's transcript at the moment it quotes. The scope is
  shown with every answer.

Both use the user's AI exactly as notes do (Apple on-device or the one
user-entered endpoint, [AI_PROVIDERS.md](AI_PROVIDERS.md)): the same Pro
gate and the same consent dialog, no separate gate. Retrieval for Chat is
local: no network, no embedding service, no index outside the app's store.
Nothing here stores transcript or screen text anywhere the purge
([REDACTION.md](REDACTION.md)) doesn't reach.

The Mac section is written by the Mac implementation, the iOS section by
the iOS side. Rules both follow: 1–4 topics per recording, labels of at
most 40 characters and 6 words, generic words ("meeting") refused,
near-duplicate keys merged (equal, equal without spaces, one typo apart);
AI topics the user removed stay removed on a re-run; scope labels **All
recordings**, **Notebook · X**, **Topic · X**, **Recording · X**; chat
answers that drew on an edited or deleted recording are deleted and the
thread flagged, the user's questions kept.

## Mac

Code: `src-tauri/src/topics.rs` (schema, label and key rules, prompt,
validation, storage, commands; tests in `topics/tests.rs`),
`src-tauri/src/chat.rs` (threads, messages, the ask flow, purge, commands),
`chat/retrieval.rs` (scope → SQL filter, passages, budget packing),
`chat/prompt.rs` (system prompt, history, user message; tests in
`chat/tests.rs`). Front end: `src/lib/topicsLogic.ts` + `topics.ts`,
`src/lib/chatLogic.ts` + `chat.ts` (tests `*.test.ts`, `npm test`),
`src/components/TopicChips.tsx` (filter chips, Group by, row chips, the
Notes-tab editor), `src/components/chat/` (`RecordingsChat.tsx`,
`ScopePicker.tsx`, `ChatMarkdown.tsx`).

### Topics

- **Model.** Table `meeting_topics (id, meeting_id → meetings ON DELETE
  CASCADE, topic, topic_key, confidence, source 'ai'|'user', created_at,
  UNIQUE (meeting_id, topic_key))` and `meeting_topics_removed (meeting_id,
  topic_key)` for AI topics the user removed. Created in
  `topics::ensure_schema`, called from `DatabaseManager::run_migrations`
  on its one connection. `topic_key(label)`: lowercase, anything but
  letters and digits becomes a space (apostrophes dropped), simple plurals
  folded ("Cell Membranes" → `cell membrane`). `display_label`: cleaned
  with the study parser (`clean_text`: one line, no control characters, no
  list markers), quotes and trailing punctuation trimmed, ≤ 40 characters,
  ≤ 6 words, generic words refused, an all-lowercase label capitalized
  ("q4 roadmap" → "Q4 roadmap"; "BIO 101" stays).
- **Near-duplicates.** `near(a, b)`: equal keys, equal without spaces, or
  one edit apart for keys of 6+ characters that hold no digit ("q4
  roadmap" ≠ "q3 roadmap"). On save, `canonical` looks the key up among
  every key already stored (most used label first): a near key adopts
  that key *and* its label, so the chip row and the Chat scope see one
  topic. `index()` (`list_topics`) returns each key with its most used
  label and recording count, and each recording's topics, user topics
  first.
- **When.** `meeting_notes.rs` calls `topics::find_after_notes` after
  notes are saved (automatic notes after a recording and **Generate
  notes**/**Regenerate**): a background task with the same AI gates as
  the notes, failures logged, never surfaced. **Find topics** / **Find
  again** on the Notes tab calls `find_topics` directly; a recording
  being recorded is refused, one run per recording at a time (`RUNNING`).
- **Prompt and validation.** `system_prompt()` asks for
  `{"topics":[{"label","confidence"}]}`, 1–4 noun phrases of 1–4 words,
  nothing generic, nothing invented, stricken text never guessed. The
  input is `study::load_input` (the Review guide's: `[m:ss]` lines,
  stricken spans as `[stricken from the record]`, deleted words gone), a
  long transcript chunked with `study::prompt::chunk_lines` to the
  model's window ("Part i of n", at most 8 parts sampled across the
  recording) and the parts merged by `merge_parts` (near keys join, a
  topic named in several parts scores higher). One retry per request on
  an unusable answer; AI access errors (consent, Pro, no provider) stop
  the run and reach the UI as their machine-readable strings. `validate`
  extracts the first JSON value (fences, prose and think blocks
  stripped, trailing commas tolerated), accepts a bare array or an array
  of strings, cleans labels, clamps confidence (missing → 0.5), merges
  near-duplicates, keeps the best four. Candidates under 0.3 are not
  stored. Nothing from the answer is logged.
- **Saving (`save_ai_topics`).** `BEGIN IMMEDIATE`; the transcript
  fingerprint (`StudyInput::fingerprint`) and the pending-Delete check are
  re-run inside the lock, exactly as the Review guide does, so labels made
  from text deleted or stricken meanwhile are never written. Then the
  recording's AI rows are replaced; user rows stay; a candidate near a
  user topic or a removed key is skipped; AI rows fill up to 4 in all.
- **Editing (`set_meeting_topics`).** The Notes tab's editor sends the
  whole list; every label is stored as the user's (≤ 8, cleaned like the
  AI's, generic ones dropped, keys canonicalized so a spelling the store
  already has merges). Any topic missing from the list is remembered in
  `meeting_topics_removed`; a label added back clears that.
- **Recordings list** (`MeetingHistory.tsx`). **Topics** chips (All ·
  label · count) under the Notebooks chips; a chip filters the list
  (client-side, from the index). **Group by: Date · Notebook · Topic**
  (remembered in `localStorage` `nf.recordings.groupBy`): Date groups by
  day ("Today", "Yesterday", the date), Notebook by name (case folded,
  then "No notebook"), Topic lists a recording under each of its topics,
  biggest topic first, then "No topics" (`groupRecordings`). Each row
  shows up to two topic chips and "+n"; clicking one filters by it. The
  Notes tab shows the recording's topics with **edit** (rename, remove,
  add) and **Find topics**; user topics have a dashed outline.
- **Purge.** A Delete (when it commits) or Strike of transcript text
  deletes the recording's AI topics (`topics::purge_ai_for_meeting`, in
  `redaction::redact_ai_outputs`, live database and app backups; the
  preview says "n AI topics on this recording: deleted"); user topics
  stay. Screen-only edits keep them. Deleting the recording deletes every
  topic and its removed-key memory (`DatabaseManager::delete_meeting`,
  explicit, besides the cascade). See [REDACTION.md](REDACTION.md).

### Chat

- **Scope.** `Scope { kind: all|notebook|topic|meeting, value }`
  (`chat/retrieval.rs`), resolved to a `Filter` that becomes one SQL
  fragment (`… IN (SELECT id FROM meetings WHERE class_name = ? COLLATE
  NOCASE)`, `… IN (SELECT meeting_id FROM meeting_topics WHERE topic_key
  = ?)`, `= ?`) and a label. `ScopePicker` offers All recordings, This
  Notebook (recent notebooks), This Topic (the index), This recording
  (every recording, the one open in Recordings first); a new chat opens with
  the recording open in Recordings, else All. `chat_scope_summary` returns
  the label, count, recent titles, topics, notebooks and types of the
  scope for the header and the suggestions. Every answer stores and shows
  the scope it was made in.
- **Threads.** `chat_threads (id, title, scope_kind, scope_value, flag,
  created_at, updated_at)`, `chat_messages (id, thread_id → threads ON
  DELETE CASCADE, role, content, citations JSON, scope_label,
  created_at)`, `chat_message_sources (message_id, meeting_id)`: every
  recording whose passages went into the prompt for that answer, cited
  or not. The title is the first question (≤ 48 characters). **Chats**
  lists past threads (delete with the bin); **New chat** starts one on
  the next question. The last 8 messages of the thread go back as prior
  turns (4 in a window under 8K tokens, each clipped), without the
  passages; the system prompt limits citations to the numbers given now.
- **Retrieval (`retrieval::retrieve`, local only).** Transcript lines by
  FTS5 (`transcripts_fts MATCH` with the OR-ed terms of
  `database::fts_or_query`, bm25-ranked, scoped by the filter on
  `t.meeting_id`, rendered through `redaction::render_plain` so stricken
  spans read `[stricken from the record]` and placeholder-only lines are
  skipped), the newest saved notes per recording (summary, decisions,
  action items, key points, scored by the share of the question's terms
  they contain) and marker notes (same, with the marker's time). Every
  passage carries the recording's title, date, type, Notebook and time.
  `pack` ranks them, caps a recording at 4 passages when the scope spans
  several, drops duplicates, fits a character budget left beside the
  system prompt, scope, question, memory and answer
  (`study::prompt::body_budget`), 12 at most, and numbers them 1…n. Fewer
  than three hits ("Summarize my week") fall back to the newest scoped
  recordings' notes summary or lines sampled across their transcript (12
  for one recording, 3 each for several). No embeddings, no index outside
  SQLite, no network.
- **Prompt and answer.** `prompt::system_prompt` (answer only from the
  sources, cite as `[n]` right after the fact, say what's missing, Markdown
  without HTML or tables, stricken text never guessed) + memory + `SCOPE:
  … SOURCES: [n] Title — date, Type, Notebook X — said at m:ss / marked
  at m:ss / from the saved notes … QUESTION: …`. `max_tokens` ≤ 900,
  scaled down for small windows. One request through `study::Completer`
  (`LiveCompleter` → `ai::complete_text`: consent, endpoint policy, Pro in
  the Mac App Store build); no streaming in the client, so the UI shows a
  thinking state. The answer is `clean_answer`ed (think blocks, a wrapping
  fence) and stored with `citations` = every passage sent (`Citation { n,
  meeting_id, title, timestamp_ms, excerpt, source }`). `ChatMarkdown`
  renders a Markdown subset (paragraphs, headings capped at h3, bullet and
  numbered lists, bold, code, fenced code) as React text nodes, never
  HTML; each `[n]` becomes a chip, and **Sources** under the answer lists
  them all. A chip calls `requestRecordingSeek(meeting_id, ms)`
  (`lib/navigation.ts`) and selects the recording; `InsightDeckView` takes
  the request when the recording is selected (or at once) and shows the
  moment through `RewindGallery`'s `seek` prop, like the quiz and Links.
- **Suggested questions.** `suggestedQuestions(scope, summary)`: up to
  four from the scope's titles, types, notebooks and topics ("What did we
  decide about Q4 roadmap?", "What's on the test for BIO 101?", "Summarize
  my week"); no AI call.
- **Purge (`chat::purge_for_meeting`).** Every assistant message with a
  `chat_message_sources` row for the recording is deleted and its thread
  flagged (`FLAG_REMOVED`, shown above the chat and in the list); a thread
  scoped to the recording is flagged too; user questions stay. Called in
  `redaction::redact_ai_outputs` (so a Delete that commits and any Strike
  of transcript text, live and in app backups; the preview says "n CHAT
  answers that drew on this recording: deleted") and in
  `DatabaseManager::delete_meeting`. Questions and answers are never
  logged.

### Tests (`topics/tests.rs`, `chat/tests.rs`, `*.test.ts`)

Rust (20): topic JSON shapes and the cap of four, malformed input never
panics, key folding and `near`, display labels, part merging, one request
for a short recording, chunked reading of a long one within a 4K window,
retry once and AI errors stop, cross-recording merge with user edits and
removed keys kept, no save after an edit (fingerprint) and no stricken
words in the prompt, purge on Strike (user topics kept) and meeting
delete with backups and old schemas tolerated; chat citation numbers,
answer cleaning and titles, scope parsing and filters, packing (rank,
per-recording cap, budget), history clipping, retrieval within scope and
budget (notes, markers, fallback, summary), an answer's scope and
citations with thread memory, stricken words never in the prompt, purge
on Strike and recording delete (questions kept, threads flagged, backups,
old schemas). TypeScript (12 of 67): topic filter, row chips, grouping by
date, notebook and topic, Group by parsing; default scope and labels,
suggested questions per scope, inline and block Markdown parsing,
citation numbers, clock. No test calls a real endpoint.

## iOS

Code: `ios/NoFriction/Topics/` (`TopicModels.swift`: the `MeetingTopic`
model, the `Topic` label/key rules, `TopicIndex`, `TopicStore`;
`TopicAI.swift`: prompt, `TopicParse`, `MeetingAI.findTopics`;
`TopicViews.swift`: chips, filter row, editor) and `ios/NoFriction/Chat/`
(`ChatModels.swift`: `ChatScope`, `ChatThread`, `ChatThreadMessage`,
`ChatCitation`, `ChatStore`; `ChatRetrieval.swift`; `ChatAI.swift`: prompt,
`MeetingAI.chat`, citation mapping, `ChatSuggestions`; `ChatView.swift`).
Tests: `ios/NoFrictionTests/TopicsAndChatTests.swift`.

### Topics

- **Model.** `MeetingTopic { id, label, key, confidence, source (ai|user),
  createdAt, meeting }`, cascade with `Meeting` (`Meeting.topics`). `key` is
  `Topic.key(label)`: lowercased, diacritics folded, anything but letters
  and digits becomes a space, articles and connectives dropped, simple
  plurals singularized ("The Q4 Roadmaps" → `q4 roadmap`). Labels are
  trimmed, one line, ≤ 40 characters, ≤ 6 words. `Meeting.removedTopicKeysJSON`
  (keys only) remembers AI topics the user removed. Everything added is an
  optional column or a new entity, so stores from older builds open without
  a schema version (a test round-trips an on-disk store).
- **When.** The same hook as notes: **Make notes** / **Make again** names the
  topics after the notes are saved (a failure shows under the topics, not
  under the notes). **Find topics** / **Find again** in the recording's
  Notes section runs it alone. Both go through `MeetingDetailView.requestAI`,
  so Pro, provider setup and consent are checked the same way.
- **Prompt and validation.** `MeetingAI.topicsSystem` asks for
  `{"topics":[{"label":…,"confidence":0–1}]}`, 1–4 noun phrases of 1–4
  words, no generic words, nothing invented, stricken text never guessed.
  The transcript goes as `[m:ss]` lines (`StudyInput.transcriptLines`); a
  transcript that doesn't fit the model's context is sent in chunks
  (`MeetingAI.chunkLines`, "Part i of n") and the chunks' topics are merged
  (`mergeTopicCandidates`: near-duplicate keys join, the commonest label
  leads, a topic several chunks named scores higher). Each request is retried
  once when the answer can't be used; AI access errors stop the run. The
  answer is untrusted: `TopicParse.validate` extracts the first JSON value
  (fences, prose and `<think>` stripped, trailing commas tolerated), cleans
  every label (`StudyParse.clean`), clamps confidence (`0.8`, `"80%"` and
  `80` all read as 0.8; missing → 0.5), drops generic and over-long labels,
  merges near-duplicates and keeps at most 4. Nothing from the answer is
  logged.
- **Near-duplicates.** `Topic.similar(a, b)`: equal keys, equal without
  spaces ("off site" ~ "offsite"), the same words in another order
  ("roadmap q4"), or one typo apart for keys of 8+ characters. `TopicIndex`
  merges topics across recordings with that rule, so the chip row stays
  small: one group per topic with its recordings, the user's spelling (or
  the commonest) as the label, biggest group first. The Topic filter, the
  Topic grouping and the Chat scope all use the index, so "Q4 roadmap" and
  "the q4 roadmaps" are one topic everywhere.
- **Saving (`TopicStore.applyAI`).** A run replaces the recording's AI
  topics. User topics stay; a candidate matching a user topic (similar key)
  or a removed key is skipped; candidates under confidence 0.3 are dropped;
  at most 4 AI topics and 8 topics in all.
- **Editing.** In Notes → Topics: tap a chip for **Rename…** / **Remove**,
  **Add** for a new one. A rename makes the topic the user's; adding a label
  that matches an AI topic makes that topic the user's; removing an AI topic
  records its key so a re-run doesn't bring it back; adding it again clears
  that. Generic words ("meeting") are refused. User topics show a small
  person mark.
- **Recordings list.** No topic chips and no Group by (removed in the
  Fadell-audit pass, F-12: a recording listed under each topic appeared
  several times). The list is by day, **Notebooks** chips are the one
  filter, and the search box matches topic labels. Topics show on the
  recording and as a Chat scope.
- **Purge.** Topics go with the recording (cascade). A Delete (when it
  commits) or Strike of transcript text deletes the recording's AI topics,
  through `RedactionEngine.purgeDerived` (with the study guide and the chat
  answers); the user's own topics stay. Screen-only edits keep topics. See
  [REDACTION.md](REDACTION.md).

### Chat

- **Tab bar.** Record · Recordings · **Chat** · People · Settings. Five tabs
  fit the iPhone tab bar (iOS shows "More" only from six), so People stays
  a tab; on iPad the same tabs are the sidebar.
- **Scope.** The menu at the top: **All recordings**, **This Notebook ▸**
  (recent notebooks), **This Topic ▸** (the merged topic groups), **This
  recording ▸** (saved recordings with a transcript). The scope covers the
  saved recordings with a transcript; the one being recorded is excluded.
  The bar shows how many recordings the scope covers. `ChatScope.filter`
  applies it. Every answer is stored with the scope it was made in and shows
  it ("Scope: Notebook · BIO 101").
- **Threads.** `ChatThread { id, title, createdAt, updatedAt, scopeKind,
  scopeValue, scopeLabel, flagged, flagNote, messages }`, cascade to
  `ChatThreadMessage { id, role (user|assistant), content, createdAt,
  scopeLabel, citationsJSON, thread }`. The title is the first question (≤ 48
  characters). **New chat** starts a thread on the next question; the list
  button shows past chats (swipe to delete). The last 8 turns (16 messages)
  of the thread go back to the model as memory, without the passages.
- **Retrieval (`ChatRetrieval`, local only).** The scoped recordings become
  `ChatSource`s (transcript lines as the prompts read them: stricken spans
  as `[stricken from the record]`, deleted words gone; AI notes; marker
  notes). Passages are windows of consecutive transcript lines (~420
  characters, cited at the first line's time), one per `##` section of the
  notes, and one per marker note. The question's terms (lowercased,
  diacritics folded, stopwords out, plurals singularized) rank passages by
  term frequency weighted by rarity across the passages, with a bonus for a
  passage that has every term or the words in a row, a small one for notes,
  recent recordings breaking ties. The best fit the character budget left
  beside the prompt, the memory and the answer (`MeetingAI.bodyBudget`), at
  most 5 per recording and 12 in all, and go to the model in reading order.
  When no term matches ("summarize everything"), the most recent
  recordings' notes and opening lines stand in. No network and no
  embeddings; nothing is indexed outside the store.
- **Prompt.** `MeetingAI.chatSystem` (answer only from the passages, cite as
  `[n]`, say what wasn't found, never guess at stricken text) +
  memory + `SCOPE: … PASSAGES: [n] Title · date · Transcript 12:34 ·
  Notebook: … QUESTION: …`. `maxTokens` ≤ 900.
- **Citations.** `MeetingAI.citations(in:passages:)` reads every `[n]` in
  the answer (first mention order, each once, numbers with no passage
  ignored) and stores `ChatCitation { n, meetingID, title, timestamp, offset,
  kind (Transcript|Notes|Marker), excerpt (≤ 140 characters) }` with the
  message. The chips under an answer show `[n] Title 12:34`; a tap opens
  `MeetingDetailView(meeting:jumpTo:)`, which scrolls the transcript to the
  line being spoken at that moment (the same `jump` the markers and the
  quiz use). A deleted recording's chip shows "This recording was deleted".
- **Suggested questions.** An empty chat offers up to four questions built
  from the scope's titles, types and topics (`ChatSuggestions.questions`),
  with no AI call; a tap asks it.
- **Gating.** `ChatView.ask` runs the same steps as a recording's AI buttons:
  Pro (paywall) → provider configured (setup sheet) → consent for a public
  endpoint (consent sheet, naming the destination) → the request through
  `AIClient`. The question is kept and asked once the step is done.
- **Purge.** `ChatStore.purge(meetingID:title:deleted:context:)`: every
  assistant message citing the recording is deleted (answers quote its
  transcript and notes), the thread is flagged with a note ("was deleted /
  was edited; answers that cited it were removed"), and a thread scoped to
  that recording is flagged too. User questions are the user's own words
  and stay. Called by **Delete Recording** (before the row goes) and by
  `RedactionEngine.purgeDerived` for a Delete that commits (also a Delete
  recovered at launch) and for a Strike of transcript text. Deleting a
  thread cascades its messages. Questions and answers are never logged.

### Tests (`TopicsAndChatTests.swift`)

Topic JSON extraction and validation (fences, thinking, bare arrays,
alternative field names, confidence forms, generic/long/duplicate labels,
the cap of four, malformed input never crashes); label and key
normalization; `similar` and `canonicalKey`; `TopicIndex` merging across
recordings; chunk merging; `findTopics` with a mock (retry once, access
errors throw, chunking of a long transcript); `applyAI` keeping user topics
and removed keys; the per-recording cap; purge on Delete commit (user
topics kept) and on Strike (screen-only strike keeps them); cascade on
recording delete; Group by Notebook / Topic / Date; chat purge on Strike,
Delete commit and recording delete (thread flagged, questions kept,
recording-scoped thread flagged, thread delete cascades); 8-turn memory;
scope filtering and stored scopes; chat end to end with a mock (memory
placement, prompt shape, citations); an on-disk store round trip; passage
building; term ranking; scoping, budget, per-recording cap, reading order
and the no-match fallback; citation mapping and round trip; suggested
questions. No test calls a real endpoint.
