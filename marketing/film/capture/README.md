# Footage capture

## iPhone and Apple Watch (Simulator)

```bash
marketing/film/capture/ios/capture.sh                  # App Store stills, all iPhone clips, all watch clips
marketing/film/capture/ios/capture.sh stills           # the five 6.9" App Store screenshots
marketing/film/capture/ios/capture.sh iphone           # iPhone clips
marketing/film/capture/ios/capture.sh watch            # watch clips
marketing/film/capture/ios/capture.sh ios-03-mark watch-03-discreet   # named clips
NF_FILM_SKIP_BUILD=1 marketing/film/capture/ios/capture.sh iphone     # reuse the last build
```

Requirements: Xcode 26.2 with the iOS 26.2 and watchOS 26.2 simulator
runtimes, XcodeGen, ffmpeg with libx264.

**Simulators.** The script uses its own, creating them if missing: "Film
iPhone 17 Pro Max" (iOS 26.2) and "Film Apple Watch Series 11 (46mm)"
(watchOS 26.2, not paired). It never touches the "NF …" simulators used by
`scripts/ios-screenshots.sh`. It sets the 9:41 status bar (full battery, 4
bars, Wi-Fi), turns off forced 24-hour time (the host's setting leaks into new
simulators; changing it reboots the simulator once) and grants calendar access
so the "Connect your calendar" card stays out of the shots.

**What runs.** The iPhone clips are the tests in
`ios/NoFrictionUITests/FilmFootageTests.swift` (one test per clip, skipped
unless `NF_FILM=1`), on the sample data from `-NFSeedDemo -NFFilm`
(`ios/NoFriction/App/DemoData.swift`, `ios/NoFriction/App/FilmDemo.swift`):
invented people, lectures and a stored study guide, a live transcript that
arrives line by line (`-NFDemoLiveFeed`), a live class (`-NFDemoLiveClass`),
and a Record-sheet Start that shows a demo recording (no microphone or speech
engine; the Simulator has neither). Times on screen read 9:41 like the status
bar. The watch clips are the watch app's debug demo states
(`ios/NoFrictionWatch/App/DemoMode.swift`, `-NFWatchDemo <state> -NFWatchFilm`;
`flow` steps through the Record flow by itself). All of it is `#if DEBUG`.

**Cutting** (`marketing/film/capture/ios/cut.py`). `xcrun simctl io …
recordVideo` records each whole test run. The test logs `start`, `still` and
`end` (wall clock) to `marketing/out/footage/ios/raw/<run>.times`, the script
adds when the recorder started, and each clip is cut between two of those
marks (table `IPHONE_CLIPS` in `capture.sh`; one run can give several clips,
e.g. `ios-01-record-sheet` and `ios-01b-record-start`). The Simulator writes
a frame only when the screen changes, so a gap between frames is a still
screen; gaps longer than the clip's "max still" (XCUITest's own waits, which
grow when the machine is busy) are shortened to it, invisibly, and the result
is placed on a constant 30 fps grid. Frame times come from the packets'
presentation times: the Simulator repeats timestamps, and ffmpeg's own
fps/trim filters then fall back to decode times that run seconds early (it
put the wrong screen in clips). `NF_FILM_RECUT=1` re-cuts the last
recordings without recording again. Watch clips start at the app's first
frame of UI, found in the recording (the top-right clock is dark on the
launch screen; `cut.py --watch-ui`), and run for a fixed length.

The watch shows the real time of the capture: watchOS simulators take no
status bar override.

**Output** (`marketing/out/` is gitignored):

| Path | What |
|---|---|
| `marketing/out/footage/ios/<clip>.mp4`, `.png` | iPhone clips, 1320x2868, and each clip's best frame (full-resolution screenshot) |
| `marketing/out/footage/watch/<clip>.mp4`, `.png` | watch clips, 416x496 |
| `marketing/out/footage/{ios,watch}/raw/` | raw recordings, cut marks, recorder logs |
| `marketing/film/stills/iphone-6.9/NN-name.png` | App Store screenshots, 1320x2868, and `-1260x2736.png` copies (committed) |

Clips are H.264 High, yuv420p, constant 30 fps, CRF 15, no audio, at the
simulator's native size. Test logs: `ios/build/film-logs/`.

**Content rules** for anything added here: invented content only (no real
names, emails, calendars), no brands or logos on screen (the meeting clip
stops above the attendees' profile-link buttons; nothing opens People,
Settings, a share sheet or the paywall), and no prices.
