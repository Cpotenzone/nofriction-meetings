#!/usr/bin/env bash
# Real app footage of the noFriction iPhone and Apple Watch apps, recorded in
# the Simulator, for the launch film and App Store app previews.
#
# Usage:
#   marketing/film/capture/ios/capture.sh                 # stills, every iPhone clip, every watch clip
#   marketing/film/capture/ios/capture.sh stills          # the five 6.9" App Store screenshots only
#   marketing/film/capture/ios/capture.sh iphone          # iPhone clips only
#   marketing/film/capture/ios/capture.sh watch           # watch clips only
#   marketing/film/capture/ios/capture.sh ios-03-mark watch-03-discreet   # named clips
#
#   NF_FILM_SKIP_BUILD=1   reuse the last build (ios/build/film-dd)
#   NF_FILM_RECUT=1        iPhone clips: re-cut the last raw recordings, don't record again
#
# Output (marketing/out/ is gitignored):
#   marketing/out/footage/ios/<clip>.mp4 + <clip>.png     H.264 yuv420p, 30 fps CFR, CRF 15, no audio, native size
#   marketing/out/footage/watch/<clip>.mp4 + <clip>.png
#   marketing/out/footage/{ios,watch}/raw/                the raw recordings and cut marks
#   marketing/film/stills/iphone-6.9/NN-name.png (+ -1260x2736.png)   App Store screenshots
#
# Uses its own simulators, "Film iPhone 17 Pro Max" (iOS 26.2) and
# "Film Apple Watch Series 11 (46mm)" (watchOS 26.2), created if missing;
# never the "NF …" ones. 9:41 status bar, 12-hour clock, calendar access
# granted (no "Connect your calendar" card). Sample data only (-NFSeedDemo
# -NFFilm, invented); the iPhone clips are the tests in
# ios/NoFrictionUITests/FilmFootageTests.swift, the watch clips the watch
# app's debug demo states (ios/NoFrictionWatch/App/DemoMode.swift).
#
# Cutting: each iPhone test logs "start" / "still" / "end" (wall clock) to
# raw/<clip>.times; the recorder's start time is logged when simctl says
# "Recording started"; the clip is [start, end] of the raw file, so a re-run
# cuts the same moments. Watch clips are cut at fixed offsets from launch.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
IOS="$ROOT/ios"
OUT="${NF_FILM_OUT:-$ROOT/marketing/out/footage}"
STILLS="${NF_FILM_STILLS:-$ROOT/marketing/film/stills/iphone-6.9}"
DD="$IOS/build/film-dd"
LOGS="$IOS/build/film-logs"
IPHONE_NAME="Film iPhone 17 Pro Max"
IPHONE_TYPE="com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro-Max"
IPHONE_RUNTIME="com.apple.CoreSimulator.SimRuntime.iOS-26-2"
WATCH_NAME="Film Apple Watch Series 11 (46mm)"
WATCH_TYPE="com.apple.CoreSimulator.SimDeviceType.Apple-Watch-Series-11-46mm"
WATCH_RUNTIME="com.apple.CoreSimulator.SimRuntime.watchOS-26-2"
APP_ID="com.nofriction.meetings"
WATCH_ID="com.nofriction.meetings.watchkitapp"
CRF=15

# One recording per test. run (raw file name, as the test names it) | test method (FilmFootageTests)
IPHONE_RUNS=(
  "ios-01-record-sheet|testClip01RecordSheet"
  "ios-02-live|testClip02Live"
  "ios-02b-live-class|testClip02bLiveClass"
  "ios-03-mark|testClip03Mark"
  "ios-04-library|testClip04Library"
  "ios-05-lecture|testClip05Lecture"
  "ios-06-review|testClip06Review"
  "ios-07-personal|testClip07Personal"
  "ios-08-meeting|testClip08Meeting"
)
# Clips cut from them: clip | run | from mark | to mark | max still (s) | end still (s)
# (a still stretch longer than "max still" is shortened to it; cut.py)
IPHONE_CLIPS=(
  "ios-01-record-sheet|ios-01-record-sheet|start|still|0.8|1.2"
  "ios-01b-record-start|ios-01-record-sheet|still|end|0.9|1.4"
  "ios-01-record-sheet-full|ios-01-record-sheet|start|end|0.8|1.4"
  "ios-02-live|ios-02-live|start|end|1.0|1.5"
  "ios-02b-live-class|ios-02b-live-class|start|end|1.0|1.5"
  "ios-03-mark|ios-03-mark|start|end|1.0|1.5"
  "ios-04-library|ios-04-library|start|end|1.6|2.6"
  "ios-05-lecture|ios-05-lecture|start|end|1.1|1.4"
  "ios-06-review|ios-06-review|start|end|1.1|1.5"
  "ios-07-personal|ios-07-personal|start|end|1.1|1.4"
  "ios-08-meeting|ios-08-meeting|start|end|1.6|3.0"
)
# clip | -NFWatchDemo mode | cut start (s after the app's UI first shows, found
# in the recording) | length (s) | still at (s into the clip)
# "flow" (DemoMode.swift), counted from the app's onAppear (the UI shows about
# 0.5–1.5 s later): Record sheet, How long? at +3.2 s, Notebook at +5.4 s,
# recording at +7.6 s, a ★ mark at +10.2 s ("Marked Important" for 3 s).
WATCH_CLIPS=(
  "watch-01-start|flow|0.3|12.5|10.0"
  "watch-01b-start-sheet|start|0.3|6.0|2.0"
  "watch-01c-length|length|0.3|6.0|2.0"
  "watch-01d-notebook|notebook|0.3|6.0|2.0"
  "watch-02-class|class|0.3|9.0|4.0"
  "watch-03-discreet|discreet|0.3|10.0|3.0"
  "watch-04-list|list|0.3|8.0|2.0"
  "watch-05-warning|warning|0.3|8.0|2.0"
)

log() { printf '==> %s\n' "$*"; }
now() { python3 -c 'import time; print(f"{time.time():.3f}")'; }

udid_for() {
  xcrun simctl list devices -j | python3 -c "import json,sys
name=sys.argv[1]
for rt,devs in json.load(sys.stdin)['devices'].items():
    for d in devs:
        if d['name']==name and d.get('isAvailable',True): print(d['udid']); sys.exit()" "$1"
}

# Find or create a film simulator; boot it.
film_sim() {
  local name="$1" type="$2" runtime="$3" udid
  udid="$(udid_for "$name")"
  if [[ -z "$udid" ]]; then
    udid="$(xcrun simctl create "$name" "$type" "$runtime")"
    log "created $name ($udid)" >&2
  fi
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  echo "$udid"
}

# 12-hour clock (the host may force 24-hour time); takes a reboot to reach the status bar.
twelve_hour() {
  local udid="$1"
  if [[ "$(xcrun simctl spawn "$udid" defaults read -g AppleICUForce24HourTime 2>/dev/null || echo 0)" != "0" ]]; then
    xcrun simctl spawn "$udid" defaults write -g AppleICUForce24HourTime -bool NO
    xcrun simctl shutdown "$udid"
    xcrun simctl boot "$udid"
    xcrun simctl bootstatus "$udid" -b >/dev/null
  fi
}

# A busy Mac makes the Simulator drop frames (animations stutter) and XCUITest
# slow. Wait (up to NF_FILM_LOAD_WAIT s) for the 1-minute load average to fall
# under NF_FILM_MAX_LOAD (default: twice the CPU count) before each recording.
wait_for_quiet() {
  local max="${NF_FILM_MAX_LOAD:-$(( $(sysctl -n hw.ncpu) * 2 ))}" waited=0 limit="${NF_FILM_LOAD_WAIT:-600}" load
  while :; do
    load="$(sysctl -n vm.loadavg | awk '{print $2}')"
    python3 -c "import sys; sys.exit(0 if $load < $max else 1)" && return 0
    if (( waited >= limit )); then echo "   (load $load still over $max; recording anyway)"; return 0; fi
    (( waited == 0 )) && echo "   (load $load over $max; waiting for a quieter moment)"
    sleep 10; waited=$((waited + 10))
  done
}

# Background recorder: sets REC_PID and REC_T0 (wall clock when recording began).
start_recording() {
  wait_for_quiet
  local udid="$1" file="$2" reclog="$3"
  rm -f "$file" "$reclog"
  xcrun simctl io "$udid" recordVideo --codec=h264 --force "$file" >"$reclog" 2>&1 &
  REC_PID=$!
  for _ in $(seq 1 100); do
    grep -q "Recording started" "$reclog" 2>/dev/null && break
    sleep 0.05
  done
  REC_T0="$(now)"
  grep -q "Recording started" "$reclog" || { echo "!! recorder didn't start: $(cat "$reclog")" >&2; return 1; }
}

stop_recording() {
  kill -INT "$REC_PID" 2>/dev/null || true
  wait "$REC_PID" 2>/dev/null || true
}

# Cut [ss, ss+dur] of the raw recording to a 30 fps CFR H.264 clip (cut.py).
# The Simulator writes a frame only when the screen changes; cut.py resamples
# to 30 fps and shortens still stretches longer than MAX_HOLD (XCUITest's own
# waits between actions), keeping the last one up to END_HOLD.
cut_clip() {
  local raw="$1" ss="$2" dur="$3" out="$4" max_hold="${5:-0.9}" end_hold="${6:-1.2}"
  shift 6 2>/dev/null || shift $#
  # remaining arguments: --keep A:B (raw seconds never shortened)
  python3 -I "$HERE/cut.py" "$raw" "$out" --start "$ss" --duration "$dur" \
    --max-hold "$max_hold" --end-hold "$end_hold" --crf "$CRF" "$@" | sed 's/^/   /'
}

# The test's "keep+" / "keep-" mark pairs (a state shown in full, e.g. a
# flipped flashcard) → --keep A:B in raw seconds.
keep_args() {
  local times="$1" t0="$2"
  awk -v t0="$t0" '$1=="keep+" {a=$2} $1=="keep-" && a!="" {printf "--keep %.3f:%.3f\n", a-t0, $2-t0; a=""}' "$times"
}

# "name time" lines → the time of `name` (first match)
mark_time() { awk -v n="$2" '$1==n {print $2; exit}' "$1"; }

# PNG without alpha (the App Store and some editors reject transparency).
flatten_png() {
  local in="$1" out="$2" size
  size="$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=s=x:p=0 "$in")"
  ffmpeg -nostdin -loglevel error -y -f lavfi -i "color=black:s=$size" -i "$in" \
    -filter_complex "[0][1]overlay=format=auto,format=rgb24" -frames:v 1 "$out"
}

build_iphone() {
  local udid="$1"
  [[ "${NF_FILM_SKIP_BUILD:-}" == 1 && -d "$DD/Build/Products" ]] && return 0
  log "xcodegen + build-for-testing (iPhone)"
  (cd "$IOS" && xcodegen generate >/dev/null)
  mkdir -p "$LOGS"
  (cd "$IOS" && xcodebuild build-for-testing -project NoFriction.xcodeproj -scheme NoFriction \
      -destination "id=$udid" -derivedDataPath "$DD" >"$LOGS/build-iphone.log" 2>&1) \
    || { grep -E "error: " "$LOGS/build-iphone.log" | head -10; return 1; }
}

run_test() {
  local udid="$1" method="$2" logfile="$3"; shift 3
  (cd "$IOS" && env "$@" TEST_RUNNER_NF_FILM=1 xcodebuild test-without-building \
      -project NoFriction.xcodeproj -scheme NoFriction -destination "id=$udid" -derivedDataPath "$DD" \
      -parallel-testing-enabled NO -only-testing:"NoFrictionUITests/FilmFootageTests/$method" >"$logfile" 2>&1)
}

prepare_iphone() {
  local udid="$1"
  twelve_hour "$udid"
  xcrun simctl status_bar "$udid" override --time 9:41 --dataNetwork wifi --wifiMode active --wifiBars 3 \
    --cellularMode active --cellularBars 4 --batteryState charged --batteryLevel 100
  xcrun simctl privacy "$udid" grant calendar "$APP_ID" 2>/dev/null || true
  xcrun simctl privacy "$udid" grant microphone "$APP_ID" 2>/dev/null || true
  xcrun simctl ui "$udid" appearance dark 2>/dev/null || true
}

capture_stills() {
  local udid="$1" staging
  staging="$(mktemp -d -t nf-film-stills)"
  log "App Store stills → ${STILLS#$ROOT/}"
  run_test "$udid" testStoreStills "$LOGS/stills.log" TEST_RUNNER_NF_FILM_STILLS_DIR="$staging" \
    || { echo "!! stills test failed (log: ${LOGS#$ROOT/}/stills.log)"; grep -E "error|failed" "$LOGS/stills.log" | head -5; return 1; }
  mkdir -p "$STILLS"
  local png name
  for png in "$staging"/*.png; do
    name="$(basename "$png" .png)"
    flatten_png "$png" "$STILLS/$name.png"
    # 6.5"-class 1260x2736: Lanczos to width 1260, center crop to 2736 high
    ffmpeg -nostdin -loglevel error -y -i "$STILLS/$name.png" \
      -vf "scale=1260:-1:flags=lanczos,crop=1260:2736:0:(ih-2736)/2,format=rgb24" -frames:v 1 "$STILLS/$name-1260x2736.png"
    echo "   $(ffprobe -v error -show_entries stream=width,height -of csv=s=x:p=0 "$STILLS/$name.png")  $name.png"
  done
  rm -rf "$staging"
  mkdir -p "$OUT/ios"
  : >"$OUT/ios/STILLS_READY"
}

# Record one test run: raw/<run>.mp4 and the marks it logged, raw/<run>.times.
record_iphone_run() {
  local udid="$1" run="$2" method="$3"
  local raw="$OUT/ios/raw/$run.mp4" times="$OUT/ios/raw/$run.times"
  mkdir -p "$OUT/ios/raw"
  if [[ "${NF_FILM_RECUT:-}" == 1 && -f "$raw" && -f "$times" ]]; then
    log "$run: re-cutting the last recording"
    return 0
  fi
  log "$run ($method)"
  rm -f "$times"
  xcrun simctl terminate "$udid" "$APP_ID" >/dev/null 2>&1 || true
  start_recording "$udid" "$raw" "$OUT/ios/raw/$run.recorder.log"
  local status=0
  run_test "$udid" "$method" "$LOGS/$run.log" TEST_RUNNER_NF_FILM_DIR="$OUT/ios/raw" || status=$?
  stop_recording
  echo "recorder_start $REC_T0" >>"$times"
  if [[ $status != 0 ]]; then
    echo "!! $run: test failed (log: ${LOGS#$ROOT/}/$run.log)"
    grep -E "error|failed" "$LOGS/$run.log" | head -5
    return 1
  fi
}

# Cut one clip from its run, between two marks; its still is the test's
# screenshot raw/<clip>.png, or else the clip's last frame.
cut_iphone_clip() {
  local clip="$1" run="$2" from="$3" to="$4" max_hold="$5" end_hold="$6"
  local raw="$OUT/ios/raw/$run.mp4" times="$OUT/ios/raw/$run.times"
  local t0 a b ss dur
  t0="$(mark_time "$times" recorder_start)"; a="$(mark_time "$times" "$from")"; b="$(mark_time "$times" "$to")"
  [[ -n "$t0" && -n "$a" && -n "$b" ]] || { echo "!! $clip: marks $from/$to missing in ${times#$ROOT/}"; return 1; }
  ss="$(python3 -c "print(f'{max(0.0, $a - $t0):.3f}')")"
  dur="$(python3 -c "print(f'{$b - $a:.3f}')")"
  log "$clip ($run, $from → $to)"
  local keeps=() k v
  while read -r k v; do keeps+=("$k" "$v"); done < <(keep_args "$times" "$t0")
  cut_clip "$raw" "$ss" "$dur" "$OUT/ios/$clip.mp4" "$max_hold" "$end_hold" ${keeps[@]+"${keeps[@]}"}
  if [[ -f "$OUT/ios/raw/$clip.png" ]]; then
    flatten_png "$OUT/ios/raw/$clip.png" "$OUT/ios/$clip.png"
  else
    ffmpeg -nostdin -loglevel error -y -sseof -0.1 -i "$OUT/ios/$clip.mp4" -frames:v 1 -update 1 "$OUT/ios/$clip.png"
  fi
  echo "   $(ffprobe -v error -select_streams v:0 -show_entries stream=width,height,r_frame_rate:format=duration -of csv=p=0 "$OUT/ios/$clip.mp4" | tr '\n' ' ')"
}

build_watch() {
  local udid="$1"
  if [[ "${NF_FILM_SKIP_BUILD:-}" != 1 || ! -d "$DD/Build/Products/Debug-watchsimulator/NoFrictionWatch.app" ]]; then
    log "build (watch)"
    (cd "$IOS" && xcodegen generate >/dev/null)
    mkdir -p "$LOGS"
    (cd "$IOS" && xcodebuild build -project NoFriction.xcodeproj -scheme NoFrictionWatch \
        -destination "id=$udid" -derivedDataPath "$DD" >"$LOGS/build-watch.log" 2>&1) \
      || { grep -E "error: " "$LOGS/build-watch.log" | head -10; return 1; }
  fi
  xcrun simctl install "$udid" "$DD/Build/Products/Debug-watchsimulator/NoFrictionWatch.app"
}

capture_watch_clip() {
  local udid="$1" clip="$2" mode="$3" cut="$4" len="$5" at="$6"
  local raw="$OUT/watch/raw/$clip.mp4"
  mkdir -p "$OUT/watch/raw"
  log "$clip (-NFWatchDemo $mode)"
  xcrun simctl terminate "$udid" "$WATCH_ID" >/dev/null 2>&1 || true
  # The app writes the time of its first frame here (DemoMode.noteAppeared, -NFWatchFilm)
  local marks; marks="$(xcrun simctl get_app_container "$udid" "$WATCH_ID" data)/Documents/film-marks.txt"
  rm -f "$marks"
  sleep 0.5
  start_recording "$udid" "$raw" "$OUT/watch/raw/$clip.recorder.log"
  sleep 0.5
  local t_launch; t_launch="$(now)"
  xcrun simctl launch "$udid" "$WATCH_ID" -NFWatchDemo "$mode" -NFWatchFilm >/dev/null
  for _ in $(seq 1 300); do [[ -s "$marks" ]] && break; sleep 0.1; done
  local t_ui; t_ui="$(mark_time "$marks" ui 2>/dev/null)"
  [[ -n "$t_ui" ]] || t_ui="$(python3 -c "print($t_launch + 2.5)")"
  # The UI reaches the screen up to ~2 s after the app's onAppear, more under load
  python3 -c "import time; time.sleep(max(0.0, $t_ui + 2.5 + $cut + $len + 0.5 - time.time()))"
  stop_recording
  xcrun simctl terminate "$udid" "$WATCH_ID" >/dev/null 2>&1 || true
  # First frame of the app's UI, found in the recording (cut.py --watch-ui)
  local vis; vis="$(python3 -I "$HERE/cut.py" --watch-ui "$raw" "$(python3 -c "print($t_launch - $REC_T0)")")"
  if [[ -z "$vis" ]]; then
    echo "   (UI frame not found in the recording; using the app's mark + 1 s)"
    vis="$(python3 -c "print(f'{$t_ui + 1.0 - $REC_T0:.3f}')")"
  fi
  printf 'recorder_start %s\nlaunch %s\nui %s\nui_visible_raw %s\n' "$REC_T0" "$t_launch" "$t_ui" "$vis" >"$OUT/watch/raw/$clip.times"
  local ss; ss="$(python3 -c "print(f'{$vis + $cut:.3f}')")"
  cut_clip "$raw" "$ss" "$len" "$OUT/watch/$clip.mp4" 99 99   # timed by the app: no hold shortening
  # Still: the clip's frame at its moment (CRF 15 at 416x496 is visually lossless)
  ffmpeg -nostdin -loglevel error -y -ss "$at" -i "$OUT/watch/$clip.mp4" -frames:v 1 -update 1 "$OUT/watch/$clip.png"
  echo "   $(ffprobe -v error -select_streams v:0 -show_entries stream=width,height,r_frame_rate:format=duration -of csv=p=0 "$OUT/watch/$clip.mp4" | tr '\n' ' ')"
}

# --- what to capture
want_stills=0; want=()
if [[ $# == 0 ]]; then set -- stills iphone watch; fi
for arg in "$@"; do
  case "$arg" in
    stills) want_stills=1 ;;
    iphone) for c in "${IPHONE_CLIPS[@]}"; do want+=("${c%%|*}"); done ;;
    watch) for c in "${WATCH_CLIPS[@]}"; do want+=("${c%%|*}"); done ;;
    ios-*|watch-*) want+=("$arg") ;;
    *) echo "unknown: $arg (stills | iphone | watch | <clip name>)"; exit 2 ;;
  esac
done
wants() { local w; for w in "${want[@]:-}"; do [[ "$w" == "$1" ]] && return 0; done; return 1; }

fail=0
need_iphone=$want_stills
for c in "${IPHONE_CLIPS[@]}"; do wants "${c%%|*}" && need_iphone=1; done
need_watch=0
for c in "${WATCH_CLIPS[@]}"; do wants "${c%%|*}" && need_watch=1; done

if [[ $need_iphone == 1 ]]; then
  IPH="$(film_sim "$IPHONE_NAME" "$IPHONE_TYPE" "$IPHONE_RUNTIME")"
  log "$IPHONE_NAME ($IPH)"
  prepare_iphone "$IPH"
  build_iphone "$IPH"
  if [[ $want_stills == 1 ]]; then capture_stills "$IPH" || fail=1; fi
  for r in "${IPHONE_RUNS[@]}"; do
    IFS='|' read -r run method <<<"$r"
    needed=0
    for c in "${IPHONE_CLIPS[@]}"; do
      IFS='|' read -r clip crun _ <<<"$c"
      [[ "$crun" == "$run" ]] && wants "$clip" && needed=1
    done
    [[ $needed == 1 ]] || continue
    record_iphone_run "$IPH" "$run" "$method" || { fail=1; continue; }
    for c in "${IPHONE_CLIPS[@]}"; do
      IFS='|' read -r clip crun from to max_hold end_hold <<<"$c"
      [[ "$crun" == "$run" ]] && wants "$clip" || continue
      cut_iphone_clip "$clip" "$run" "$from" "$to" "$max_hold" "$end_hold" || fail=1
    done
  done
fi

if [[ $need_watch == 1 ]]; then
  WUD="$(film_sim "$WATCH_NAME" "$WATCH_TYPE" "$WATCH_RUNTIME")"
  log "$WATCH_NAME ($WUD)"
  twelve_hour "$WUD"
  # watchOS simulators take no status bar override: the watch shows the real time
  # (DemoMode's own times are 9:41-based).
  build_watch "$WUD"
  for c in "${WATCH_CLIPS[@]}"; do
    IFS='|' read -r clip mode cut len at <<<"$c"
    wants "$clip" || continue
    capture_watch_clip "$WUD" "$clip" "$mode" "$cut" "$len" "$at" || fail=1
  done
fi
exit $fail
