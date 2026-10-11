# Release status and agent coordination

Read this before starting work. Several agents work in this repo at once.
Last updated 2026-10-10 by the release/distribution session
(`nofriction-meetings-39`). Update it when you change any of the facts below.

## In the stores now

| What | Version (build) | State | Notes |
|---|---|---|---|
| App Store, iPhone/iPad (+ Watch) | 1.0.1 (10) | Waiting for review | Screen capture, sync, Pro paywall, redesign |
| Mac App Store | 3.7.0 (43) | Waiting for review | One window, sync, Pro paywall |
| Subscriptions (Pro Monthly/Yearly + group) | — | Waiting for review | Submitted with iPhone 1.0.1 |
| TestFlight external (https://testflight.apple.com/join/4JnJD3Fk) | iOS 10, Mac 43 | Waiting for beta review | Builds 8/9 and 41/42 expired |

Release is **manual**: the owner presses Release in App Store Connect after
approval. Don't change the store text, screenshots or review notes of the
versions in review.

## Version plan

| Train | Contents | Owner |
|---|---|---|
| iPhone 1.0.2 / Mac 3.7.1 | Post-launch fixes: macOS 13 minimum, Pro label, screen names, model checksum, dependency cleanup (PR `fix/mac-post-launch`), iPhone search jump-to-line (#60, merged) | release session |
| Mac 3.8.0 / iOS 1.1.0 | Lessons (`docs/LESSONS.md`, `.worktrees/lessons-stoichiometry`) | Lessons agent |

## Rules for every agent

- **Distribution.** Only the release session uploads builds, bumps
  `ios/build_number.txt` or `src-tauri/build_number.txt`, changes version
  fields, or touches App Store Connect and TestFlight. Hand it a merged PR.
  Never run `release-*.sh --upload`.
- **Checks.** Never run `npm run build`, because its prebuild bumps the build
  number. Use `npx tsc --noEmit && npx vite build`.
- **Branches.** Work in a worktree on your own branch. Rebase on `origin/main`
  before opening a PR and again before merge. Ask before touching another
  agent's files.
- **Pro.** `docs/PRO.md` is the single source of truth. New Pro features need a
  key in both `src/lib/pro.ts` and `ios/NoFriction/Store/ProFeature.swift`; a
  test checks that they match. AI goes through `ai::client::complete`. Gate
  non-AI Mac features with `entitlement::require_pro_feature`.
- **Navigation.** It is Record ⌘1, Recordings ⌘2, Chat ⌘3 today. Adding a view
  (Lessons) needs the owner's yes, because DESIGN.md has a three-view rule.
- **Invariants.** Follow the CLAUDE.md noFriction section: client-only, sync
  device to device, Delete/Strike purge everywhere, no logging of transcript
  text, and both Mac flavors build.
- **Shared machine resources.**
  - The headed gstack browser on port 34567 belongs to the release session and
    is signed in to Apple. Don't stop or reconnect it.
  - The simulators `NF iPhone 17 Pro Max`, `NF-Audit iPhone 17 Pro` and
    `NF iPad Pro 13` are in use. Create your own.

## Owner to-dos (Casey)

- After approval, press Release:
  https://appstoreconnect.apple.com/apps/6818838861/distribution
- Test on real devices via TestFlight: screen capture, iPhone↔Mac sync, and the
  Watch.
- Sign in to GoDaddy in the gstack browser so the release session can revoke
  the token pasted in chat. DNS no longer depends on it.
