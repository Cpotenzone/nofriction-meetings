# Sync with your Mac: shared spec (Mac + iOS)

Your iPhone and your Mac keep the same recordings, directly, over your own
Wi-Fi. There is no noFriction server, no iCloud and no relay: the two devices
find each other with Bonjour, talk over TLS that the iPhone pins to the Mac's
certificate, and authenticate each other with a secret only they hold.
"Data Not Collected" stays true: noFriction receives nothing.

Sync is part of **noFriction Pro**. It runs only while Pro is active on the
iPhone (`store.isPro`) and, in the Mac App Store build, while
`entitlement::require_pro_feature(ProFeature::Sync)` passes (the Developer ID build has no
gate). Free users see **Sync with your Mac** in Settings with a short
explanation and the paywall (feature key `sync`).

## What syncs (v1)

| Mac | iPhone | Wire item |
|-----|--------|-----------|
| `meetings` + `meeting_details` + `meeting_attendees` | `Meeting` + `Attendance`/`Person` | `recording` |
| `transcripts` (`sync_id`) | `Segment` (`syncID`) | `line`, `edit` |
| `redactions` (strikes of words and lines) | `Redaction` (strike, words/line) | `strike` |
| `meeting_notes` (latest) | `Meeting.aiNotes` | `notes` |
| `meeting_markers` | `MomentMarker` | `mark` |
| `meeting_references` | `MeetingReference` | `ref` |
| `meeting_topics` | `MeetingTopic` | `topic` |
| deletions of any of the above | | `gone` |

A recording's fields: title, start and end, type (Meeting · Class ·
Personal), notebook, planned length, and the calendar fields (event, planned
start and end, location, meeting link, invite notes, people with their role).

**Not in v1:**

- **Photos and screens.** iPhone photos and Mac screens stay on the device
  that took them. A strike of a screen therefore doesn't sync either (there
  is nothing on the other device to remove). The protocol reserves `blob`
  messages for them (below).
- **Audio.** Phone audio stays on the phone; the Mac never had any.
- **Chats and review/study guides.** Each device makes its own from the
  synced transcript; they are AI outputs and are purged locally as before.
- **Word timings.** A synced line has no word timings on the other device,
  so a time-range edit there uses the line rules in REDACTION.md.

IDs are stable across devices: a recording keeps its UUID everywhere, a Mac
line gets a random `sync_id` (32 hex digits) and an iPhone line a `syncID`
UUID the first time it syncs. On the wire every id is a lowercase UUID with
hyphens; the Mac stores 32-hex ids (`Uuid::simple`) for everything except
recordings, so it converts on the way in and out.

## Discovery and transport

- **Bonjour.** While sync is on (and Pro is active) the Mac advertises
  `_nofriction._tcp` with its device id in the TXT record (`id=…`, `v=1`),
  via the system's mDNSResponder (`DNSServiceRegister`). It stops
  advertising when sync is turned off or Pro lapses. The iPhone browses with
  `NWBrowser`. Same network only; nothing is routed off the LAN.
- **Listener.** The Mac listens on TCP (an OS-chosen port, IPv4 and IPv6)
  only while sync is on. It refuses connections from addresses that aren't
  local (loopback, RFC 1918, link-local, CGNAT/Tailscale 100.64/10, ULA
  `fc00::/7`).
- **TLS.** `rustls` with a self-signed certificate made by `rcgen` the first
  time sync is turned on. The private key (PKCS#8) and certificate live only
  in the Keychain (service `com.nofriction.meetings.sync`). The iPhone pins
  the certificate's SHA-256 fingerprint in
  `sec_protocol_options_set_verify_block`; any other certificate fails the
  handshake. TLS 1.2 minimum.
- **Framing.** Every message is a 4-byte big-endian length followed by that
  many bytes of UTF-8 JSON (at most 16 MiB). Each message has `"v": 1` and a
  type `"t"`. JSON is written with sorted keys, no whitespace, `/` not
  escaped, absent optionals omitted and integers only (times are milliseconds
  since 1970, confidence is per mille), so both sides produce the same bytes.
  Golden fixtures made by the Rust side (`ios/NoFrictionTests/SyncFixtures/`)
  are decoded and re-encoded byte for byte by the Swift tests.

## Pairing

1. **Settings → Sync → Pair a device** on the Mac shows a QR code (made with
   the `qrcode` crate as SVG) and a **Copy pairing link** button. The link:

   `nfsync:1?id=<mac id>&n=<Mac name>&fp=<sha-256 hex of the certificate>&h=<host,host>&p=<port>&c=<code>`

   `c` is a one-time code of 10 characters (Crockford base32, 50 bits). It
   expires after 5 minutes, works once, and is thrown away after 5 wrong
   attempts. `h` lists the Mac's LAN addresses as hints; Bonjour is tried
   first.
2. The iPhone scans it (**Settings → Sync → Pair with your Mac**, camera via
   `DataScannerViewController`, or **Paste pairing link**), connects with
   the fingerprint pinned, and sends `pair {code, device_id, name}`.
3. The Mac checks the code (constant-time), makes a 32-byte random device
   secret, stores it in the Keychain (account `device:<phone id>`) and the
   device's name in `sync_devices`, and answers `paired {device_id, name,
   secret}`. The iPhone stores the secret in its Keychain (service
   `com.nofriction.meetings.sync`, account `mac:<mac id>`) and the pin and
   name in its sync state. The secret never leaves these two Keychains.
4. **Forget** on either side deletes that side's secret (and on the Mac the
   device row). The other side's next session fails authentication and asks
   to pair again.

## Session

The iPhone always starts a session; iOS can't listen in the background. It
syncs when the app comes to the foreground, when a recording stops, and on
**Sync now**. The Mac only answers.

```
iPhone                                   Mac
hello {device_id, nonce_p}          →
                                    ←    challenge {nonce_m, proof_m}
auth {proof_p}                      →
                                    ←    welcome {device_id, name}
batch {phase:"removals", items, last} →  (repeated until last)
                                    ←    applied {}
pull {since}                        →
                                    ←    batch {phase:"changes", items, last, upto}  (repeated)
batch {phase:"changes", items, last} →   (repeated until last)
                                    ←    applied {}
done {}                             →
```

- **Authentication.** `nonce_p` and `nonce_m` are 32 random bytes (base64).
  `proof_m = HMAC-SHA256(secret, "nfsync-v1 mac" ‖ nonce_p ‖ nonce_m)` proves
  the Mac holds the secret, `proof_p = HMAC-SHA256(secret, "nfsync-v1 phone"
  ‖ nonce_m ‖ nonce_p)` the iPhone. Proofs are compared in constant time.
  An unknown device, a wrong proof or a wrong fingerprint ends the session
  (`error {code, message}`). Codes: `unknown_device`, `bad_proof`,
  `bad_code`, `expired_code`, `pro_required`, `sync_off`, `version`,
  `protocol`.
- **Removals first, both ways, before any content.** The iPhone sends its
  deletions and strikes; the Mac applies them through its purge path. Then
  the Mac sends its own removals followed by its changes, built *after*
  the iPhone's removals were applied; the iPhone applies them through its
  purge path. Only then does the iPhone send its changes. So content one
  device removed is never sent back to it, and the receiver never
  re-imports it.
- **Cursors.** The Mac numbers its changes (`sync_meta.seq`). `pull.since`
  is the highest Mac number the iPhone has applied; the last batch carries
  `upto`, which the iPhone stores after applying. Changes the Mac applied
  from that iPhone are not sent back to it. The iPhone keeps, per Mac, a
  list of what it has sent (ids and hashes of the non-transcript records;
  ids only for lines) and sends what is new or different.
- Batches hold at most 500 items. A session that fails part-way is safe to
  repeat: every item is idempotent. `applied {retry}` lists ids the Mac
  couldn't apply yet (for example a recording it is still recording); the
  iPhone sends them again next time.

## Items

Each item has `"k"`. Times are integer milliseconds since 1970.

| `k` | Fields |
|-----|--------|
| `recording` | `id`, `title`, `started`, `ended?`, `kind` (`meeting`/`class`/`personal`), `notebook?`, `planned?` (minutes), `cal?` {`event?`, `start?`, `end?`, `location?`, `url?`, `notes?`}, `people` [{`email`, `name?`, `role`}], `mod` |
| `line` | `id`, `rec`, `text`, `at`, `dur?` (ms), `speaker?`, `src?` (`screen`: heard from what was playing during iPhone screen capture, `Segment.source`; the Mac keeps it in `transcripts.source`) |
| `edit` | `id`, `rec`, `keep` (see below) |
| `strike` | `id`, `rec`, `target` (`words`/`line`), `from?`, `to?`, `created`, `reason?`, `line?` (id of the line holding the marker) |
| `notes` | `rec`, `md` (Markdown), `made`, `stale`, `mod` |
| `mark` | `id`, `rec`, `at`, `kind` (`important`/`question`/`test`), `note?`, `created`, `mod` |
| `ref` | `id`, `rec`, `url`, `title?`, `note?`, `created`, `mod` |
| `topic` | `id`, `rec`, `label`, `key`, `conf` (0–1000), `source` (`ai`/`user`), `created` |
| `gone` | `entity` (`recording`/`line`/`mark`/`ref`/`topic`/`notes`), `id`, `rec?` |

Strike markers inside line text use one canonical form on the wire,
`⟦stricken:<uuid>⟧`; the Mac stores `⟦strickenid<32 hex>⟧` and the iPhone
`⟦stricken:<UUID>⟧`, each converting on the way in and out.

Notes: the Mac renders its structured notes as Markdown in the iPhone's
style (bold headings, `•` bullets, headings by recording type). Notes from
the iPhone are saved on the Mac as Markdown (`model_used =
"synced-markdown"`) and shown with the Markdown renderer.

## Merge

- **Last writer wins per record** for recordings, notes, marks and
  references: the item with the later `mod` replaces the other. The Mac
  stamps `mod` when a row changes (triggers on each table write
  `sync_meta.modified_at`). The iPhone can't tell when a field changed, so it
  stamps a changed record with the time it notices the change, at the next
  sync: when both devices changed the same record between two syncs, the
  iPhone's version wins. Clocks are the devices' own.
- **Lines never change except by removal.** A new line is sent once, whole.
  After that a line only ever loses words (Delete) or gains a strike marker
  (Strike), and it travels as an `edit`, never as text again.
- **Deletions and strikes always win and are permanent.** A `gone` is kept
  forever as an id (Mac `sync_meta.deleted`, iPhone sync state), so a late
  copy of that record is never re-imported. Strike records can't be edited
  or removed on either platform.

### Line edits never carry removed text

An `edit` describes the line as it now is on the sender, without sending
its words: `keep` lists, in order, `w:<hash>` for each remaining word and
`m:<uuid>` for each strike marker, where `hash` is the first 8 bytes (hex)
of `HMAC-SHA256(token_key, word)` and `token_key = HMAC-SHA256(secret,
"nfsync-v1 tokens")`. Words are the line's whitespace-separated runs
(Unicode White_Space), with markers split out.

The receiver hashes its own copy of the line the same way and takes the
longest common subsequence with `keep`:

- its words that aren't in `keep` were removed by the sender: each maximal
  run of them is removed through the receiver's own purge path;
- a marker in `keep` that the receiver doesn't have marks a strike: the run
  in that gap is struck with that marker id (or, if the receiver had
  already deleted those words, only the marker is inserted);
- the receiver's own markers stay (strikes are permanent), and words the
  receiver already removed stay removed.

So both devices end with the union of their removals, the struck words
never travel, and the receiver re-runs its full purge for the removed
words: search index, AI notes (rewritten only for distinctive words, flagged
either way), review guide, AI topics and chat answers (deleted), app
backups and logs on the Mac, freed-space scrub. A line whose words are all
gone is removed. `gone` for a line removes the whole line the same way, and
`gone` for a recording runs the device's full Delete recording path.

## Security model

- **Who can connect.** Only a device that scanned the QR code in the 5
  minutes it was valid (or got the copied link) can pair. Afterwards only a
  device holding the 32-byte secret can sync; the iPhone only talks to the
  Mac whose certificate it pinned.
- **What an attacker on the same Wi-Fi sees.** That a noFriction Mac is
  advertising (its Bonjour name) and TLS traffic. Not the content, not the
  secret, and they can't pair without the code or impersonate the Mac
  without its private key.
- **Keys.** The TLS private key and every device secret live only in the
  Keychain. They are never logged, never shown in the UI, never written to
  SQLite. Transcript text is never logged.
- **No server.** Nothing is sent anywhere but the paired device on the same
  network. `scripts/check-ai-provider-policy.py` checks that the sync code
  has no URL, HTTP client or AI call, that the Mac starts the listener only
  through `sync::start_if_enabled` (sync on and Pro), and that the watch
  app has no network.

## Limits

- Same network only, and the iPhone app must be open (or have just
  stopped a recording) to sync. iOS doesn't let apps listen in the
  background, so the Mac can't push.
- One Mac can pair with several iPhones (and iPads); an iPhone can pair with
  several Macs.
- Wall-clock last-writer-wins; the iPhone wins concurrent edits of the same
  record (above).
- Photos, screens, audio, chats and review guides don't sync (above).
- Topics: new topics and removed topics sync; renaming a topic doesn't
  (yet). AI topics are deleted by each device's own purge after an edit.
- Notes made on the iPhone replace the Mac's when they are newer, and the
  Mac shows them as Markdown until **Make again** there.

## Reserved for photos and screens (not in v1)

iPhone photos and screen-capture screens (`Snapshot`, with `source` =
`screen` for a captured screen, nil for a photo) and Mac screens (`frames`,
`screen_states`) will travel as files:
`blob_offer {id, rec, kind: photo|screen, source?, at, size, sha256}`,
`blob_want {ids}`, `blob_chunk {id, offset, data}` (256 KiB, base64),
`blob_done {id}`; the receiver checks the SHA-256 before saving the file.
A deleted or stricken photo/screen travels as `gone {entity: "screen"}`.

## Code

- Mac: `src-tauri/src/sync/` — `protocol.rs` (messages, canonical JSON,
  framing, ids, markers, word hashes, proofs), `merge.rs` (line-edit merge),
  `store.rs` (schema, triggers, backfill, outgoing items, applying),
  `pairing.rs` (certificate, codes, secrets, QR), `server.rs` (listener and
  session), `bonjour.rs`, `mod.rs` (start/stop and the Tauri commands
  `sync_status`, `sync_set_enabled`, `sync_pair_start`, `sync_pair_cancel`,
  `sync_forget`). Incoming line edits go through
  `redaction::edit_line_synced`. UI: `src/features/settings/SyncSettings.tsx`,
  `src/lib/syncLogic.ts`.
- iPhone: `ios/NoFriction/Sync/` — `SyncProtocol.swift`, `SyncMerge.swift`,
  `SyncState.swift` (per-Mac state files and the ledger in Application
  Support/Sync, Keychain), `SyncEngine.swift`, `SyncClient.swift`
  (NWConnection, NWBrowser, the session), `SyncCenter.swift` (status and
  triggers), `SyncView.swift`. Incoming removals go through
  `RedactionEngine+Sync.swift`; Delete recording is `RecordingDeletion`.
  `RedactionEngine` reports line edits to the ledger (ids only).

## Testing

- `cargo test --lib sync::` (protocol, auth, pairing codes, merge rules,
  tombstones through the purge, a loopback session over TLS, golden
  fixtures). `NF_WRITE_SYNC_FIXTURES=1 cargo test --lib sync::tests::golden`
  rewrites `ios/NoFrictionTests/SyncFixtures/` after a deliberate protocol
  change.
- `SyncWireTests` and `SyncEngineTests` (iOS): the fixtures byte for byte,
  the auth vectors, the shared merge cases, and the engine on an in-memory
  store.
- End to end, iOS Simulator against a Mac server built from the branch
  (temp directory, in-memory secrets; nothing of the app's data or
  Keychain is touched):

  ```sh
  cd src-tauri && cargo run --example sync_test_server -- /tmp/nf-sync-e2e &
  cd ios && TEST_RUNNER_NF_SYNC_LINK_FILE=/tmp/nf-sync-e2e/link.txt xcodebuild test \
    -project NoFriction.xcodeproj -scheme NoFriction \
    -only-testing:NoFrictionTests/SyncEndToEndTests -destination 'id=<simulator>'
  ```

  It pairs, checks a wrong pin is refused, syncs both ways, then strikes on
  the iPhone and deletes on the Mac and checks both removals crossed and
  were purged from search and notes on both sides.
- On real devices: pair from **Settings → Sync** on both and check the same
  things by hand; Bonjour and Local Network permission can only be checked
  on hardware.
