#!/usr/bin/env bash
# App Store screenshots for the iPhone/iPad app.
#
# Usage:
#   scripts/ios-screenshots.sh            # iPhone 6.9" + iPad 13"
#   scripts/ios-screenshots.sh iphone     # just one device
#   scripts/ios-screenshots.sh ipad
#
# Creates the simulators if missing ("NF iPhone 17 Pro Max", "NF iPad Pro 13"),
# boots them, sets the status bar to 9:41 / full battery / 4 bars, and runs
# NoFrictionUITests/AppStoreScreenshots with the -NFSeedDemo sample data
# (invented people and companies; no real personal data). PNGs land in
# ios/AppStore/screenshots/<device>/NN-name.png and are checked against the
# App Store sizes (6.9": 1320x2868 or 1290x2796; 13" iPad: 2064x2752 or 2048x2732).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IOS="$ROOT/ios"
OUT="$IOS/AppStore/screenshots"
RUNTIME="${NF_SIM_RUNTIME:-$(xcrun simctl list runtimes -j | python3 -c 'import json,sys; r=[x for x in json.load(sys.stdin)["runtimes"] if x["platform"]=="iOS" and x["isAvailable"]]; print(sorted(r,key=lambda x:[int(p) for p in x["version"].split(".")])[-1]["identifier"])')}"

# name | device type | output folder | allowed WxH sizes
DEVICES=(
  "NF iPhone 17 Pro Max|com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro-Max|iphone-6.9|1320x2868 1290x2796"
  "NF iPad Pro 13|com.apple.CoreSimulator.SimDeviceType.iPad-Pro-13-inch-M4-8GB|ipad-13|2064x2752 2048x2732"
)

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
  case "$want" in all) ;; iphone) [[ $folder == iphone* ]] || continue ;; ipad) [[ $folder == ipad* ]] || continue ;; *) echo "unknown device '$want'"; exit 2 ;; esac

  udid="$(udid_for "$name")"
  if [[ -z "$udid" ]]; then
    echo "==> creating simulator '$name' ($type, $RUNTIME)"
    udid="$(xcrun simctl create "$name" "$type" "$RUNTIME")"
  fi
  echo "==> $name ($udid)"
  xcrun simctl boot "$udid" 2>/dev/null || true
  xcrun simctl bootstatus "$udid" -b >/dev/null
  xcrun simctl status_bar "$udid" override --time 9:41 --dataNetwork wifi --wifiMode active --wifiBars 3 \
    --cellularMode active --cellularBars 4 --batteryState charged --batteryLevel 100

  # Calendar on, so the "Connect your calendar" card doesn't sit in the shots
  xcrun simctl privacy "$udid" grant calendar com.nofriction.meetings 2>/dev/null || true

  dest="$OUT/$folder"
  rm -rf "$dest"; mkdir -p "$dest"
  log="$IOS/build/screenshots-$folder.log"
  mkdir -p "$IOS/build"
  if ! (cd "$IOS" && TEST_RUNNER_NF_APPSTORE_DIR="$dest" xcodebuild test \
      -project NoFriction.xcodeproj -scheme NoFriction \
      -destination "id=$udid" -derivedDataPath build/dd \
      -only-testing:NoFrictionUITests/AppStoreScreenshots >"$log" 2>&1); then
    echo "!! screenshot test failed on $name (log: ${log#$ROOT/})"
    grep -E "error: |\*\* TEST" "$log" | head -10 || true
    fail=1
  fi
  xcrun simctl status_bar "$udid" clear || true

  for png in "$dest"/*.png; do
    [[ -e "$png" ]] || { echo "!! no screenshots in $dest"; fail=1; break; }
    w=$(sips -g pixelWidth "$png" | awk '/pixelWidth/{print $2}')
    h=$(sips -g pixelHeight "$png" | awk '/pixelHeight/{print $2}')
    if [[ " $sizes " == *" ${w}x${h} "* ]]; then
      echo "   ok  ${w}x${h}  ${png#$ROOT/}"
    else
      echo "   BAD ${w}x${h}  ${png#$ROOT/} (want one of: $sizes)"; fail=1
    fi
  done
done
exit $fail
