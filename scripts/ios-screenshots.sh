#!/usr/bin/env bash
# App Store screenshots for the iPhone/iPad app.
#
# Usage:
#   scripts/ios-screenshots.sh            # iPhone 6.9" + iPad 13" + Apple Watch 46mm
#   scripts/ios-screenshots.sh iphone     # just one device
#   scripts/ios-screenshots.sh ipad
#   scripts/ios-screenshots.sh watch
#
# Reuses existing simulators ("NF iPhone 17 Pro Max", "NF iPad Pro 13",
# "NF Apple Watch Series 11 (46mm)"); boots them, sets the status bar to
# 9:41 / full battery / 4 bars (iPhone/iPad; watchOS simulators don't take a
# status bar override), and runs NoFrictionUITests/AppStoreScreenshots with
# the -NFSeedDemo sample data (invented people and companies; no real
# personal data). The watch app is launched in its debug demo states
# (-NFWatchDemo recording|idle|list|class|discreet: no microphone, sample rows only) and
# captured with simctl, then flattened to opaque PNGs (the App Store rejects
# alpha). PNGs land in ios/AppStore/screenshots/<device>/NN-name.png and are
# checked against the App Store sizes (6.9": 1320x2868 or 1290x2796; 13" iPad:
# 2064x2752 or 2048x2732; Apple Watch Series 10/11 46mm: 416x496, Ultra 3: 422x514).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IOS="$ROOT/ios"
OUT="${NF_SCREENSHOT_OUTPUT:-$IOS/AppStore/screenshots}"
BUILD_ROOT="${NF_IOS_BUILD_ROOT:-$IOS/build}"
DERIVED_DATA="${NF_SCREENSHOT_DERIVED_DATA:-$BUILD_ROOT/dd}"
SCREENSHOT_TEST="${NF_SCREENSHOT_TEST:-NoFrictionUITests/AppStoreScreenshots}"
# Override test/output to capture review-only images without replacing store images.

# name | device type | output folder | allowed WxH sizes
DEVICES=(
  "NF iPhone 17 Pro Max|com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro-Max|iphone-6.9|1320x2868 1290x2796"
  "NF iPad Pro 13|com.apple.CoreSimulator.SimDeviceType.iPad-Pro-13-inch-M4-8GB|ipad-13|2064x2752 2048x2732"
  "NF Apple Watch Series 11 (46mm)|com.apple.CoreSimulator.SimDeviceType.Apple-Watch-Series-11-46mm|watch-46mm|416x496"
)
# Watch scenes: launch argument | output name
WATCH_SCENES=("recording|01-recording" "idle|02-record" "list|03-recordings" "class|04-class" "discreet|05-discreet")
WATCH_BUNDLE_ID="com.nofriction.meetings.watchkitapp"

# Re-encode a PNG without alpha (black background; the watch UI is black).
flatten_png() {
  local tmpdir script; tmpdir="$(mktemp -d -t nf-flatten)"; script="$tmpdir/flatten.swift"
  cat >"$script" <<'SWIFT'
import AppKit
let a = CommandLine.arguments
guard a.count == 3, let img = NSImage(contentsOfFile: a[1]),
      let cg = img.cgImage(forProposedRect: nil, context: nil, hints: nil),
      let ctx = CGContext(data: nil, width: cg.width, height: cg.height, bitsPerComponent: 8, bytesPerRow: 0,
                          space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
else { exit(1) }
ctx.setFillColor(CGColor(red: 0, green: 0, blue: 0, alpha: 1))
ctx.fill(CGRect(x: 0, y: 0, width: cg.width, height: cg.height))
ctx.draw(cg, in: CGRect(x: 0, y: 0, width: cg.width, height: cg.height))
guard let out = ctx.makeImage(), let data = NSBitmapImageRep(cgImage: out).representation(using: .png, properties: [:]) else { exit(1) }
do { try data.write(to: URL(fileURLWithPath: a[2])) } catch { exit(1) }
SWIFT
  local status=0
  xcrun swift "$script" "$1" "$2" || status=$?
  rm -rf "$tmpdir"
  return $status
}

# Watch: build the watch app for the simulator, install it, and capture each demo scene.
capture_watch() {
  local udid="$1" staging="$2" log="$3"
  (cd "$IOS" && xcodebuild build -project NoFriction.xcodeproj -scheme NoFrictionWatch \
      -destination "id=$udid" -derivedDataPath "$DERIVED_DATA" \
      -clonedSourcePackagesDirPath "$BUILD_ROOT/SourcePackages" >"$log" 2>&1) || return 1
  xcrun simctl install "$udid" "$DERIVED_DATA/Build/Products/Debug-watchsimulator/NoFrictionWatch.app" || return 1
  local scene arg name raw
  for scene in "${WATCH_SCENES[@]}"; do
    IFS='|' read -r arg name <<<"$scene"
    xcrun simctl terminate "$udid" "$WATCH_BUNDLE_ID" >/dev/null 2>&1 || true
    xcrun simctl launch "$udid" "$WATCH_BUNDLE_ID" -NFWatchDemo "$arg" >/dev/null || return 1
    sleep 4
    raw="$staging/.raw-$name.png"
    xcrun simctl io "$udid" screenshot "$raw" >/dev/null 2>&1 || return 1
    flatten_png "$raw" "$staging/$name.png" || return 1
    rm -f "$raw"
  done
  xcrun simctl terminate "$udid" "$WATCH_BUNDLE_ID" >/dev/null 2>&1 || true
}

want="${1:-all}"

udid_for() {
  xcrun simctl list devices -j | python3 -c "import json,sys
name=sys.argv[1]
for rt,devs in json.load(sys.stdin)['devices'].items():
    for d in devs:
        if d['name']==name and d.get('isAvailable',True): print(d['udid']); sys.exit()" "$1"
}

echo "==> xcodegen generate"
(cd "$IOS" && xcodegen generate >/dev/null)

fail=0
for entry in "${DEVICES[@]}"; do
  IFS='|' read -r name type folder sizes <<<"$entry"
  case "$want" in
    all) ;;
    iphone) [[ $folder == iphone* ]] || continue ;;
    ipad) [[ $folder == ipad* ]] || continue ;;
    watch) [[ $folder == watch* ]] || continue ;;
    *) echo "unknown device '$want'"; exit 2 ;;
  esac

  udid="$(udid_for "$name")"
  if [[ -z "$udid" ]]; then
    echo "!! Existing simulator '$name' is missing; no device was created." >&2
    echo "   Create it with: xcrun simctl create \"$name\" $type <runtime id from: xcrun simctl list runtimes>" >&2
    exit 1
  fi
  echo "==> $name ($udid)"
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null

  dest="$OUT/$folder"
  mkdir -p "$BUILD_ROOT" "$OUT"
  staging="$(mktemp -d "$OUT/.capture-$folder.XXXXXX")"
  log="$BUILD_ROOT/screenshots-$folder.log"

  if [[ $folder == watch* ]]; then
    if ! capture_watch "$udid" "$staging" "$log"; then
      echo "!! watch capture failed on $name (log: ${log#$ROOT/})"
      grep -E "error: " "$log" | head -10 || true
      fail=1
      rm -rf "$staging"
      continue
    fi
  else
  xcrun simctl status_bar "$udid" override --time 9:41 --dataNetwork wifi --wifiMode active --wifiBars 3 \
    --cellularMode active --cellularBars 4 --batteryState charged --batteryLevel 100

  # Calendar on, so the "Connect your calendar" card doesn't sit in the shots
  xcrun simctl privacy "$udid" grant calendar com.nofriction.meetings 2>/dev/null || true

  if ! (cd "$IOS" && TEST_RUNNER_NF_APPSTORE_DIR="$staging" xcodebuild test \
      -project NoFriction.xcodeproj -scheme NoFriction \
      -destination "id=$udid" -derivedDataPath "$DERIVED_DATA" \
      -clonedSourcePackagesDirPath "$BUILD_ROOT/SourcePackages" \
      -parallel-testing-enabled NO -maximum-concurrent-test-simulator-destinations 1 \
      -only-testing:"$SCREENSHOT_TEST" >"$log" 2>&1); then
    echo "!! screenshot test failed on $name (log: ${log#$ROOT/})"
    grep -E "error: |\*\* TEST" "$log" | head -10 || true
    fail=1
    rm -rf "$staging"
    xcrun simctl status_bar "$udid" clear || true
    continue
  fi
  xcrun simctl status_bar "$udid" clear || true
  fi

  for png in "$staging"/*.png; do
    [[ -e "$png" ]] || { echo "!! no screenshots in $dest"; fail=1; break; }
    w=$(sips -g pixelWidth "$png" | awk '/pixelWidth/{print $2}')
    h=$(sips -g pixelHeight "$png" | awk '/pixelHeight/{print $2}')
    alpha=$(sips -g hasAlpha "$png" | awk '/hasAlpha/{print $2}')
    if [[ $alpha == yes ]]; then
      echo "   BAD alpha channel  ${png#$ROOT/} (the App Store rejects transparency)"; fail=1
    elif [[ " $sizes " == *" ${w}x${h} "* ]]; then
      echo "   ok  ${w}x${h}  ${png#$ROOT/}"
    else
      echo "   BAD ${w}x${h}  ${png#$ROOT/} (want one of: $sizes)"; fail=1
    fi
  done
  if [[ $fail == 0 ]]; then
    mkdir -p "$dest"
    cp "$staging"/*.png "$dest/"
  fi
  rm -rf "$staging"
done
exit $fail
