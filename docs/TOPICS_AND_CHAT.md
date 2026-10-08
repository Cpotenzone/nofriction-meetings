# Topics and Chat: shared spec (Mac + iOS)

Two AI features on top of recordings, with the same words on both platforms:

- **Topics**: what a recording was about, as 1–4 short noun phrases
  ("Q4 roadmap", "Mitosis"). Named by the AI when notes are made (and on
  demand with **Find topics**); the user can rename, remove and add. A
  **Topics** chip row sits beside **Notebooks** in the recordings list, and
  **Group by: Date · Notebook · Topic** regroups the list.
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

The Mac section is written by the Mac implementation; this file was
started by the iOS side.

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
- **When.** The same hook as notes: **Summarize** / **Redo notes** names the
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
- **Recordings list.** **Topics** chips (All · topic · count) under the
  Notebooks row; the search box also matches topic labels. **Group by**
  (toolbar menu, remembered in `recordingsGroupBy`): **Date** (by day),
  **Notebook** (by name, then "No notebook"), **Topic** (a recording appears
  under each of its topics, biggest topic first, then "No topics"). Each row
  shows up to two topic chips (user topics first, then by confidence) and
  "+n".
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
