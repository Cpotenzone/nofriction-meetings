#!/usr/bin/env bash
# Build (and optionally upload) the iPhone/iPad app for App Store Connect / TestFlight.
#
# Usage:
#   scripts/release-ios.sh --check     # validate config only; changes nothing
#   scripts/release-ios.sh             # archive + export an App Store .ipa (no upload)
#   scripts/release-ios.sh --upload    # archive + upload to App Store Connect (TestFlight)
#
# Steps: xcodegen generate → next build number (ios/build_number.txt holds the
# last one used; it only ever goes up) → xcodebuild archive (generic iOS,
# Release, automatic signing, -allowProvisioningUpdates) → xcodebuild
# -exportArchive with ios/ExportOptions.plist (destination "export", or
# "upload" with --upload).
#
# Upload auth: an App Store Connect API key, from the environment only —
# never put key material in this repo:
#   ASC_KEY_ID      key ID              (App Store Connect → Users and Access → Integrations)
#   ASC_ISSUER_ID   issuer ID
#   ASC_KEY_PATH    path to the AuthKey_<ID>.p8 file
# --upload fails if any is missing. Without --upload they're optional; when
# set they're also used for -allowProvisioningUpdates (no Xcode account needed).
#
# Output: ios/build/release/noFriction-<version>-<build>.xcarchive and, for an
# export, ios/build/release/export-<build>/noFriction.ipa.
# NF_IOS_BUILD_ROOT / NF_IOS_DERIVED_DATA override the build locations.
# DerivedData stays beside this checkout by default (no internal Xcode cache).
# The build number is recorded only after a successful archive.
# NF_IOS_PROFILE_UUID + NF_WATCH_PROFILE_UUID + NF_IOS_SIGNING_IDENTITY select
# installed manual App Store profiles (com.nofriction.meetings and
# com.nofriction.meetings.watchkitapp) and the distribution certificate,
# without portal provisioning changes. Automatic signing creates both profiles.
#
# The Apple Watch app (Watch/NoFrictionWatch.app) is built, signed and
# exported inside the iPhone app; the script checks it is embedded with the
# same version/build and that the credential scan covered it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IOS="$ROOT/ios"
PROJECT_YML="$IOS/project.yml"
EXPORT_OPTIONS="$IOS/ExportOptions.plist"
BUILD_FILE="$IOS/build_number.txt"
BUILD_ROOT="${NF_IOS_BUILD_ROOT:-$IOS/build}"
OUT="$BUILD_ROOT/release"
DERIVED_DATA="${NF_IOS_DERIVED_DATA:-$BUILD_ROOT/dd-release}"
SCHEME="NoFriction"
TEAM_ID="C7GCEESE2V"
BUNDLE_ID="com.nofriction.meetings"
WATCH_BUNDLE_ID="com.nofriction.meetings.watchkitapp"
WATCH_APP_PATH="Watch/NoFrictionWatch.app"
AUDIT_PYTHON="${NF_AUDIT_PYTHON:-python3}"

MODE="export"
for arg in "$@"; do
  case "$arg" in
    --check) MODE="check" ;;
    --upload) [[ $MODE == check ]] || MODE="upload" ;;
    -h|--help) sed -n '2,35p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg (see --help)" >&2; exit 2 ;;
  esac
done
# --check --upload: check, including the upload credentials
CHECK_UPLOAD=0
[[ " $* " == *" --upload "* ]] && CHECK_UPLOAD=1

errors=0
err()  { echo "  ✗ $*" >&2; errors=$((errors + 1)); }
ok()   { echo "  ✓ $*"; }
warn() { echo "  ! $*"; }

echo "==> Checking configuration"
"$AUDIT_PYTHON" "$ROOT/scripts/check-ai-provider-policy.py" || err "AI source policy failed"
for tool in xcodebuild xcodegen plutil /usr/libexec/PlistBuddy; do
  command -v "$tool" >/dev/null 2>&1 && ok "$tool" || err "$tool not found"
done

[[ -f "$PROJECT_YML" ]] || { err "missing $PROJECT_YML"; }
grep -q "DEVELOPMENT_TEAM: $TEAM_ID" "$PROJECT_YML" && ok "team $TEAM_ID" || err "project.yml: DEVELOPMENT_TEAM is not $TEAM_ID"
grep -q "PRODUCT_BUNDLE_IDENTIFIER: $BUNDLE_ID" "$PROJECT_YML" && ok "bundle id $BUNDLE_ID" || err "project.yml: bundle id is not $BUNDLE_ID"
grep -q "PRODUCT_BUNDLE_IDENTIFIER: $WATCH_BUNDLE_ID" "$PROJECT_YML" && ok "watch app bundle id $WATCH_BUNDLE_ID" || err "project.yml: watch app bundle id is not $WATCH_BUNDLE_ID"
grep -q "WKCompanionAppBundleIdentifier: $BUNDLE_ID" "$PROJECT_YML" && ok "watch companion $BUNDLE_ID" || err "project.yml: WKCompanionAppBundleIdentifier is not $BUNDLE_ID"
grep -q "CODE_SIGN_STYLE: Automatic" "$PROJECT_YML" && ok "automatic signing" || err "project.yml: CODE_SIGN_STYLE is not Automatic"
VERSION="$(sed -n 's/^ *MARKETING_VERSION: *"\{0,1\}\([0-9.]*\)"\{0,1\}.*/\1/p' "$PROJECT_YML" | head -1)"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]] && ok "version $VERSION" || err "project.yml: can't read MARKETING_VERSION"

if plutil -lint "$EXPORT_OPTIONS" >/dev/null 2>&1; then
  pb() { /usr/libexec/PlistBuddy -c "Print :$1" "$EXPORT_OPTIONS" 2>/dev/null || true; }
  [[ "$(pb method)" == "app-store-connect" ]] && ok "ExportOptions method app-store-connect" || err "ExportOptions: method must be app-store-connect"
  [[ "$(pb teamID)" == "$TEAM_ID" ]] && ok "ExportOptions teamID" || err "ExportOptions: teamID must be $TEAM_ID"
  [[ "$(pb uploadSymbols)" == "true" ]] && ok "ExportOptions uploadSymbols" || err "ExportOptions: uploadSymbols must be true"
  [[ "$(pb manageAppVersionAndBuildNumber)" == "false" ]] && ok "ExportOptions manageAppVersionAndBuildNumber false" || err "ExportOptions: manageAppVersionAndBuildNumber must be false"
else
  err "ExportOptions.plist missing or invalid: $EXPORT_OPTIONS"
fi

LAST_BUILD="$(tr -d '[:space:]' < "$BUILD_FILE" 2>/dev/null || true)"
if [[ "$LAST_BUILD" =~ ^[0-9]+$ ]]; then
  NEXT_BUILD=$((LAST_BUILD + 1))
  ok "build number: last $LAST_BUILD, next $NEXT_BUILD"
else
  err "ios/build_number.txt must hold the last build number used (an integer)"
fi

# Signing: automatic signing can create a distribution certificate and the
# App Store profile itself (-allowProvisioningUpdates), given an Xcode account
# for the team or an ASC API key. Report what's on this Mac.
if security find-identity -v -p codesigning 2>/dev/null | grep -E -q "Apple Distribution: .*\($TEAM_ID\)"; then
  ok "Apple Distribution certificate for team $TEAM_ID in the keychain"
else
  warn "no Apple Distribution certificate in the keychain; Xcode will try to create one (needs an Admin/App Manager account or ASC key)"
fi

have_asc=1
for v in ASC_KEY_ID ASC_ISSUER_ID ASC_KEY_PATH; do [[ -n "${!v:-}" ]] || have_asc=0; done
if [[ $have_asc == 1 ]]; then
  [[ -f "$ASC_KEY_PATH" ]] && ok "ASC API key $ASC_KEY_ID (file present)" || err "ASC_KEY_PATH does not exist: $ASC_KEY_PATH"
elif [[ $MODE == upload || $CHECK_UPLOAD == 1 ]]; then
  for v in ASC_KEY_ID ASC_ISSUER_ID ASC_KEY_PATH; do [[ -n "${!v:-}" ]] || err "$v is not set (required for --upload)"; done
else
  warn "ASC_KEY_ID / ASC_ISSUER_ID / ASC_KEY_PATH not set: signing uses the Xcode account; --upload would fail"
fi

if [[ $errors -gt 0 ]]; then
  echo "==> $errors problem(s); fix them first." >&2
  exit 1
fi
if [[ $MODE == check ]]; then
  echo "==> Config OK (version $VERSION, next build $NEXT_BUILD). Nothing was changed."
  exit 0
fi

AUTH=()
SIGNING=(-allowProvisioningUpdates)
if [[ -n "${NF_IOS_PROFILE_UUID:-}" ]]; then
  [[ -n "${NF_IOS_SIGNING_IDENTITY:-}" ]] || { echo "NF_IOS_SIGNING_IDENTITY required with NF_IOS_PROFILE_UUID" >&2; exit 1; }
  # The watch app needs its own App Store profile (App ID $WATCH_BUNDLE_ID)
  [[ -n "${NF_WATCH_PROFILE_UUID:-}" ]] || { echo "NF_WATCH_PROFILE_UUID required with NF_IOS_PROFILE_UUID (App Store profile for $WATCH_BUNDLE_ID)" >&2; exit 1; }
  # Per-target profiles: project.yml maps these to each target's PROVISIONING_PROFILE_SPECIFIER
  SIGNING=(CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM="$TEAM_ID" NF_IOS_PROFILE_SPECIFIER="$NF_IOS_PROFILE_UUID"
           NF_WATCH_PROFILE_SPECIFIER="$NF_WATCH_PROFILE_UUID" CODE_SIGN_IDENTITY="$NF_IOS_SIGNING_IDENTITY")
fi
if [[ $have_asc == 1 ]]; then
  AUTH=(-authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID")
fi

echo "==> xcodegen generate"
(cd "$IOS" && xcodegen generate)

mkdir -p "$OUT"
ARCHIVE="$OUT/noFriction-$VERSION-$NEXT_BUILD.xcarchive"
echo "==> Archiving $VERSION ($NEXT_BUILD) → ${ARCHIVE#$ROOT/}"
set +e
xcodebuild archive \
  -project "$IOS/NoFriction.xcodeproj" \
  -scheme "$SCHEME" \
  -configuration Release \
  -destination "generic/platform=iOS" \
  -archivePath "$ARCHIVE" \
  -derivedDataPath "$DERIVED_DATA" \
  -clonedSourcePackagesDirPath "$BUILD_ROOT/SourcePackages" \
  "${SIGNING[@]}" \
  ${AUTH[@]+"${AUTH[@]}"} \
  CURRENT_PROJECT_VERSION="$NEXT_BUILD" \
  2>&1 | tee "$OUT/archive-$NEXT_BUILD.log" | grep -E "error:|warning: .*(sign|profile|provision)|ARCHIVE (SUCCEEDED|FAILED)"
status=${PIPESTATUS[0]}
set -e
if [[ $status != 0 || ! -d "$ARCHIVE" ]]; then
  echo "==> Archive failed; see ${OUT#$ROOT/}/archive-$NEXT_BUILD.log. Build number not used." >&2
  exit 1
fi

# The Apple Watch app must be inside the iPhone app (not a separate product),
# with the same version and build, pointing back at the iPhone app.
APP="$ARCHIVE/Products/Applications/noFriction.app"
WATCH_APP="$APP/$WATCH_APP_PATH"
apps_in_archive="$(find "$ARCHIVE/Products/Applications" -maxdepth 1 -name '*.app' | wc -l | tr -d ' ')"
[[ "$apps_in_archive" -eq 1 ]] || { echo "==> Archive has $apps_in_archive top-level apps; the watch app must only be embedded (SKIP_INSTALL)" >&2; exit 1; }
[[ -d "$WATCH_APP" ]] || { echo "==> Watch app missing from the archive: $WATCH_APP_PATH" >&2; exit 1; }
wpb() { /usr/libexec/PlistBuddy -c "Print :$1" "$WATCH_APP/Info.plist" 2>/dev/null || true; }
[[ "$(wpb CFBundleIdentifier)" == "$WATCH_BUNDLE_ID" ]] || { echo "==> Watch app bundle id is $(wpb CFBundleIdentifier)" >&2; exit 1; }
[[ "$(wpb WKCompanionAppBundleIdentifier)" == "$BUNDLE_ID" ]] || { echo "==> Watch app companion id is $(wpb WKCompanionAppBundleIdentifier)" >&2; exit 1; }
[[ "$(wpb CFBundleShortVersionString)" == "$VERSION" && "$(wpb CFBundleVersion)" == "$NEXT_BUILD" ]] \
  || { echo "==> Watch app version $(wpb CFBundleShortVersionString) ($(wpb CFBundleVersion)) != $VERSION ($NEXT_BUILD)" >&2; exit 1; }
codesign --verify --strict "$WATCH_APP" || { echo "==> Watch app signature check failed" >&2; exit 1; }
ok "watch app embedded: $WATCH_APP_PATH ($WATCH_BUNDLE_ID $VERSION ($NEXT_BUILD)), signed"

# Inspect the actual signed app (watch app included) before any export or
# upload. The scan prints only redacted findings and fails closed on
# incomplete inspection, including a missing or misidentified watch app.
"$AUDIT_PYTHON" "$ROOT/scripts/scan-release-credentials.py" \
  --artifact "$APP" \
  --require-embedded "$WATCH_APP_PATH=$WATCH_BUNDLE_ID" \
  --receipt "$OUT/credential-audit-$NEXT_BUILD.json" --reject-retired-services

# The archive carries this build number now: record it (monotonic), keep project.yml in step
echo "$NEXT_BUILD" > "$BUILD_FILE"
sed -i '' -E "s/^( *CURRENT_PROJECT_VERSION: *)\"?[0-9]+\"?/\1\"$NEXT_BUILD\"/" "$PROJECT_YML"
ok "build number $NEXT_BUILD recorded in ios/build_number.txt and project.yml"

OPTIONS="$OUT/ExportOptions-$NEXT_BUILD.plist"
cp "$EXPORT_OPTIONS" "$OPTIONS"
DEST="export"; [[ $MODE == upload ]] && DEST="upload"
/usr/libexec/PlistBuddy -c "Set :destination $DEST" "$OPTIONS"
EXPORT_PROVISIONING=(-allowProvisioningUpdates)
if [[ -n "${NF_IOS_PROFILE_UUID:-}" ]]; then
  /usr/libexec/PlistBuddy -c "Set :signingStyle manual" "$OPTIONS"
  /usr/libexec/PlistBuddy -c "Add :provisioningProfiles dict" "$OPTIONS"
  /usr/libexec/PlistBuddy -c "Add :provisioningProfiles:$BUNDLE_ID string $NF_IOS_PROFILE_UUID" "$OPTIONS"
  /usr/libexec/PlistBuddy -c "Add :provisioningProfiles:$WATCH_BUNDLE_ID string $NF_WATCH_PROFILE_UUID" "$OPTIONS"
  /usr/libexec/PlistBuddy -c "Add :signingCertificate string $NF_IOS_SIGNING_IDENTITY" "$OPTIONS"
  EXPORT_PROVISIONING=()
fi

EXPORT_DIR="$OUT/export-$NEXT_BUILD"
echo "==> Exporting (destination: $DEST)"
set +e
xcodebuild -exportArchive \
  -archivePath "$ARCHIVE" \
  -exportOptionsPlist "$OPTIONS" \
  -exportPath "$EXPORT_DIR" \
  ${EXPORT_PROVISIONING[@]+"${EXPORT_PROVISIONING[@]}"} \
  ${AUTH[@]+"${AUTH[@]}"} 2>&1 | tee "$OUT/export-$NEXT_BUILD.log" | grep -E "error:|EXPORT (SUCCEEDED|FAILED)|Upload|upload"
status=${PIPESTATUS[0]}
set -e
if [[ $status != 0 ]]; then
  echo "==> Export failed (exit $status); see ${OUT#$ROOT/}/export-$NEXT_BUILD.log" >&2
  exit $status
fi

if [[ $DEST != upload ]]; then
  "$AUDIT_PYTHON" "$ROOT/scripts/scan-release-credentials.py" \
    --artifact "$EXPORT_DIR/noFriction.ipa" \
    --require-embedded "$WATCH_APP_PATH=$WATCH_BUNDLE_ID" \
    --receipt "$OUT/credential-audit-ipa-$NEXT_BUILD.json" --reject-retired-services
fi

if [[ $DEST == upload ]]; then
  echo "==> Uploaded $VERSION ($NEXT_BUILD). It appears in App Store Connect → TestFlight after processing."
else
  echo "==> Exported: ${EXPORT_DIR#$ROOT/}/ (upload later with --upload, or Transporter)"
fi
