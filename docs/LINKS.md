# Links & References: shared spec (Mac + iOS)

Every recording has a **Links** list: the sites that came up in it, and the
references the user adds: a meeting's agenda and slides, a class's syllabus
and readings, or the page a friend mentioned, and every site someone named or
showed.

## For users

- Open a recording and choose **LINKS** (Mac: next to Rewind and Notes; iPhone
  and iPad: the Links section on the meeting).
- Each link shows its site and a short path, where it came from, how often,
  and the first time it came up:
  - **Said**: addresses in the transcript, written ("khanacademy.org/math") or
    spoken ("khan academy dot org slash math", "w w w dot …").
  - **On screen** (Mac): addresses in the screen text, plus the address of the
    browser page you had in front while recording (see below).
  - **Added**: references you add: a web address, an optional title ("Syllabus")
    and an optional note ("pages 4–9").
- Mac: click the time ("first said at 12:03") to see that moment in Rewind.
  **Open** opens web links (http and https only) in your browser. **Hide** takes
  a detected link off the list (Show hidden brings it back). **Copy all as
  Markdown** copies the list.
- Nothing is fetched: noFriction never visits the pages, not even for a title
  or an icon. Titles come from what is already on your device: the browser
  window's title, or what you typed.
- Deleting or striking the words or screens a link came from removes the link.
  Deleting the meeting removes its references.

### Browser addresses (Mac, Developer ID build)

While a meeting records, noFriction notes the address of the frontmost Safari,
Chrome, Arc, Edge or Brave page when all of these are true:

- screen capture is on, the recording isn't paused, and **Record the browser's
  address during meetings** is on (the bottom of the Links view; on by default);
- Accessibility is **already** allowed for noFriction. It is checked without
  asking: noFriction never shows the Accessibility prompt for this.

It reads only the address (and the window title), never the page. Windows whose
title says Private, Incognito or InPrivate are skipped. Safari doesn't put
"Private" in its window titles, so a Safari private window can't be told apart:
turn the setting off before browsing privately during a recording. The Mac App
Store build has no Accessibility access and never records browser addresses.

## Design

- **Derived, not stored.** "Said" and "On screen" links are computed from the
  transcript and the screen text each time the list is shown. No copy of a
  detected URL is kept, so Delete, Strike and time ranges remove links together
  with the text they came from (docs/REDACTION.md).
- **Browser addresses are screen text.** Each captured address is a
  `text_snapshots` row (`source = 'browser_url'`, meeting id, time, browser
  name, window title, the normalized URL as `text`), deduplicated (a new row
  only when the page changes) and capped at 2,000 per recording. Every screen
  purge already covers such rows: screen Delete/Strike (loose screen text
  captured while that screen was shown), time ranges, meeting delete
  (`ON DELETE CASCADE` on `meeting_id`), and app backups. Tests prove each one.
- **References** are `meeting_references` on the Mac (`id, meeting_id, url,
  title, note, created_at`, `ON DELETE CASCADE`, and deleted explicitly in the
  meeting-delete path) and `MeetingReference` on iOS (cascade from `Meeting`).
  Only http(s) addresses are accepted; one typed without a scheme gets
  `https://`. The address is kept as the user typed it.
- **Hide** stores only `sha256(meeting_id + "\n" + key)` per meeting
  (`meeting_link_hidden(meeting_id, url_hash)`), never the URL text, and salted
  per meeting so the same site hashes differently in each. After every Delete,
  Strike or time range, hashes whose link no longer appears anywhere in the
  meeting are deleted (`meeting_links::prune_hidden`).
- **No network.** No titles, no favicons, no link previews.
- **Open** accepts `http`/`https` only, checked in the UI and again in Rust
  (`open_meeting_link`) or Swift (`LinkDetector.isOpenable`). Never
  `javascript:`, `file:`, `data:`, `mailto:` or anything else.
- **Logs** never contain URLs or transcript text.

## Detection rules

Code: `src-tauri/src/meeting_links/detect.rs` (Mac) and
`ios/NoFriction/Links/LinkDetector.swift` (iOS). Both run the cases in
`src-tauri/src/meeting_links/detection_cases.json`; the iOS test target bundles
that file. A rule change goes in both files and the cases.

- **Strike markers** (`⟦strickenid…⟧`, `⟦stricken:…⟧`) become a boundary first:
  no link is built across one.
- **Written** (transcripts and screen text):
  - `http://…` / `https://…` up to whitespace or a quote/angle bracket.
  - Dotted hosts: `www.<anything>`, or a bare domain whose last label is in the
    TLD list (`com org net edu gov io ai co uk ca de …`). File extensions that
    are also country codes (`.rs .md .py .sh .pl .zip .mov .app`) are not in it.
  - A bare domain needs a lowercase TLD unless the whole host is uppercase
    (`EXAMPLE.COM`), so two sentences run together (`project.It`) aren't a link.
    `example.com.Next` backs off to `example.com`.
  - Not a link: emails (`@` before or after), something right after a slash,
    dot, `@`, `$`, `:` and similar (paths, identifiers), a host followed by `(`
    (`df.info()`), numbers (`3.14`, `1.2.3`), abbreviations (`Mr.`, `e.g.`,
    `U.S.`).
  - Trailing sentence punctuation is dropped; a closing bracket that has its
    opening one in the link is kept (`/wiki/Foo_(bar)`).
- **Spoken** (transcripts only): `label dot label … dot tld` with tld in
  `com org net edu gov io ai co dev app info me tv us uk ca au de fr xyz`
  (also "a i", "i o", "a.i."), optional `w w w` / `triple w` / `dub dub dub`
  first, and `slash word` path segments.
  - A label can't be a common word ("the dot com bubble"), and no punctuation
    may sit inside the chain ("Google, dot com").
  - "<name> at school dot edu" is an email (skipped); "is at", "available at"
    and similar introduce a place (kept).
  - Only the one word before "dot" is taken: "khan academy dot org" gives
    `academy.org`. Speech recognition usually writes the address itself
    ("khanacademy.org"), which is detected as written.
- **Normalization** (the dedupe key): lowercase host; drop `www.`; merge http
  and https (Open uses https unless the link was only ever written as http);
  drop the default port, trailing slashes, the fragment (except `#/route`),
  `utm_*`, `fbclid`, `gclid` and other click ids, and parameters that can carry
  credentials (`token`, `access_token`, `key`, `pwd`, `password`, `sig`,
  `session`, …). Hosts must be ASCII; user info (`user:pass@`) is refused.

## Counts and times

- Said: one count per mention; the time is the transcript line's.
- On screen: one count per screen capture it appeared in (a page full of the
  same link counts once); the time is the capture's.
- First time is the earliest mention or capture, in ms from the meeting start
  (the time Rewind shows).
- An added reference with the same address as a detected link shows as one row
  with both badges.

## Platforms

- **Mac**: `src-tauri/src/meeting_links.rs` (list, references, hide, commands),
  `meeting_links/browser_url.rs` (DMG only; `#[cfg(not(feature = "mas"))]`),
  UI `src/components/MeetingLinksPanel.tsx`, helpers `src/lib/meetingLinks.ts`.
  Commands: `list_meeting_links`, `add_meeting_reference`,
  `update_meeting_reference`, `delete_meeting_reference`, `hide_meeting_link`
  (`hidden: false` shows it again), `open_meeting_link`,
  `get_browser_url_capture`, `set_browser_url_capture`.
- **iOS / iPadOS**: Said and Added only (no screen capture on iOS), with add,
  edit, delete and open. `ios/NoFriction/Links/` and
  `ios/NoFriction/Views/MeetingLinksSection.swift`. No Hide on iOS yet.

## Tests

- Mac `cargo test --lib meeting_links`: shared detection and normalization
  cases, the scheme allowlist, the list (merging, counts, first time, order),
  references, hidden hashes (hash only, salted, pruned after a strike), derived
  links leaving with deleted/stricken words, and browser-address rows removed by
  meeting delete, screen delete, screen strike and time-range delete/strike.
  DMG: the capture's dedupe, cap, private-window skip and row shape.
- Mac `npm test`: open allowlist, typed addresses, display, times, Markdown.
- iOS `MeetingLinksTests`: the same shared cases, the scheme allowlist,
  references (add, edit, delete, cascade with the meeting), and Said links
  leaving with deleted/stricken words.
- Not covered by automated tests: reading the address from a live browser
  through Accessibility (needs a running browser and the permission).
