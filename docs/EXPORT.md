# Export a recording as a package

One recording, as something you can hand to someone: a folder (or a .zip of
it) with the recording as a Markdown document and as a web page, the photos
and screens you picked, the transcript, and the review guide and flashcards
if you made them. The AI can fill it in: summary and notes, highlights,
captions for the pictures, picking the pictures, and the follow-up email.

Mac: Recordings → a recording → **Export…** (right of the Rewind · Notes ·
Links · Review tabs). iPhone/iPad: a recording → Share button → **Export…**
(**Share as Text** is still there).

## What's in a package

```
2026-10-21 Weekly sync/
  Weekly sync.md          the recording as Markdown, images embedded by relative path
  Weekly sync.html        the same as a web page: no scripts, no outside requests; print it to PDF
  screens/01-screen-2-00.png   the picked screens and photos, numbered in order, then the time
  screens/02-photo-14-07.jpg
  transcript.txt          [12:03] Speaker: text, one line each
  Review guide.md         (Study guide.md for a class) if one was made
  flashcards.csv          if the guide has flashcards (front,back for Anki/Quizlet)
```

The document's sections, in order, each only if ticked and present: title
and type · date · length, notebook, people; Summary; Highlights; Key topics,
Decisions, Action items (iPhone notes and notes synced from the iPhone go
under **Notes** as they were written); Screens / Photos / Screens and photos
with captions; Marked moments; Links; Follow-up email; Also in this package.
The footer says when any part was written by AI.

Both platforms write the same layout (`src-tauri/src/export_package/build.rs`,
`ios/NoFriction/Export/ExportPackage.swift`); the tests pin the file names.

## The sheet

- **What goes in**: one checkbox per part. A part that doesn't exist yet is
  unticked and greyed, with its AI tool beside it (Make notes, Find
  highlights, Draft email).
- **Screens and photos**: click (tap) to pick. Opening the sheet picks every
  photo, then screens spread evenly over the recording, 12 in all. Each picked
  one gets a caption field. Mac: double-click or ⤢ for a large preview;
  filter All · Picked · Screens · Photos.
- **AI tools**
  - **Make notes**: the recording's usual notes (and topics), saved to the
    recording as if made from the Notes tab.
  - **Find highlights**: 3–8 key moments. The model picks numbered parts of
    the transcript and quotes them; a quote that isn't really in that part
    (or contains `[stricken from the record]`) is dropped, so a highlight
    never puts words in anyone's mouth. Labels are editable; remove with ×.
  - **Pick with AI**: chooses up to 8 pictures worth keeping and captions
    them, from what was being said around each (and, on the Mac, the app and
    window). Text only, so Apple's on-device model can do it.
  - **Write captions**: captions for the picked ones that have none.
  - **Draft email** (meetings): the usual follow-up email, editable.
  - **Fill with AI**: whichever of the above is missing, one after another.
- **Save** (Mac): **Save as Zip…** (save panel) or **Save to Folder…** (a new
  folder inside the folder you pick; never overwrites, adds " 2"). Then
  **Show in Finder**. **Share…** (iPhone): makes the zip and opens the share
  sheet (Save to Files, AirDrop, Mail…).

## Rules

- **Free.** Exporting is data portability, like Share as Text and the JSON
  export (docs/PRO.md). The AI tools are Pro through the usual gate: the Mac's
  `ai::client::complete` (Pro, then consent), the iPhone's Pro → AI setup →
  consent steps. Paywall titles: notes → Notes, email → Follow-up email,
  the rest → AI features.
- **Nothing new is stored.** Highlights, captions and the email draft live in
  the sheet until they're written into the user's file. Make notes saves notes
  exactly as the Notes tab does. So Delete and Strike have nothing new to
  purge (docs/REDACTION.md).
- **Removed text stays removed.** Transcript text goes through
  `redaction::render_plain` (Mac) / `RedactionText.entries` (iPhone):
  stricken words export as `[stricken from the record]`; deleted lines and
  stricken screens aren't there. Files already exported can't be recalled,
  like any export.
- **The iPhone's working copy.** The iPhone builds the package in
  `tmp/Export` before sharing. It's emptied when the sheet closes, before each
  new package, at launch, and by every Delete or Strike (`StoreHygiene.scrub`).
- **Sandbox.** The Mac App Store build writes only where the save or folder
  panel granted access: the zip goes straight to the chosen file (no temp
  file beside it), the folder inside the chosen folder. No new entitlement.
- **Untrusted model output.** Answers are parsed as JSON with the review
  guide's parser, every field cleaned to one line and capped; part and picture
  numbers are checked against what was sent. Everything is escaped in the
  Markdown and the HTML; only http(s) links become anchors.

## Files

| | Mac | iPhone |
|---|---|---|
| Package | `src-tauri/src/export_package/build.rs` | `ios/NoFriction/Export/ExportPackage.swift` |
| AI tools | `src-tauri/src/export_package/ai.rs` | `ios/NoFriction/Export/ExportAI.swift` |
| Sheet | `src/components/export/ExportSheet.tsx`, `src/lib/exportLogic.ts` | `ios/NoFriction/Export/ExportSheet.swift` |
| Commands | `src-tauri/src/export_package/commands.rs` | — |
| Tests | `src-tauri/src/export_package/tests.rs`, `src/lib/exportLogic.test.ts` | `ios/NoFrictionTests/ExportTests.swift`, `ios/NoFrictionUITests/ExportFlowTests.swift` |

The film harness (`marketing/film/mac-harness/mock/backend.ts`) mocks the
Mac commands, so the sheet can be filmed or checked in a browser.
