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
# The build number is recorded only after a successful archive.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IOS="$ROOT/ios"
PROJECT_YML="$IOS/project.yml"
EXPORT_OPTIONS="$IOS/ExportOptions.plist"
BUILD_FILE="$IOS/build_number.txt"
OUT="$IOS/build/release"
SCHEME="NoFriction"
TEAM_ID="C7GCEESE2V"
BUNDLE_ID="com.nofriction.meetings"

MODE="export"
for arg in "$@"; do
  case "$arg" in
    --check) MODE="check" ;;
    --upload) [[ $MODE == check ]] || MODE="upload" ;;
    -h|--help) sed -n '2,27p' "$0"; exit 0 ;;
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
for tool in xcodebuild xcodegen plutil /usr/libexec/PlistBuddy; do
  command -v "$tool" >/dev/null 2>&1 && ok "$tool" || err "$tool not found"
done

[[ -f "$PROJECT_YML" ]] || { err "missing $PROJECT_YML"; }
grep -q "DEVELOPMENT_TEAM: $TEAM_ID" "$PROJECT_YML" && ok "team $TEAM_ID" || err "project.yml: DEVELOPMENT_TEAM is not $TEAM_ID"
grep -q "PRODUCT_BUNDLE_IDENTIFIER: $BUNDLE_ID" "$PROJECT_YML" && ok "bundle id $BUNDLE_ID" || err "project.yml: bundle id is not $BUNDLE_ID"
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
if security find-identity -v -p codesigning 2>/dev/null | grep -q "Apple Distribution"; then
  ok "Apple Distribution certificate in the keychain"
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
  -allowProvisioningUpdates \
  ${AUTH[@]+"${AUTH[@]}"} \
  CURRENT_PROJECT_VERSION="$NEXT_BUILD" \
  2>&1 | tee "$OUT/archive-$NEXT_BUILD.log" | grep -E "error:|warning: .*(sign|profile|provision)|ARCHIVE (SUCCEEDED|FAILED)"
status=${PIPESTATUS[0]}
set -e
if [[ $status != 0 || ! -d "$ARCHIVE" ]]; then
  echo "==> Archive failed; see ${OUT#$ROOT/}/archive-$NEXT_BUILD.log. Build number not used." >&2
  exit 1
fi

# The archive carries this build number now: record it (monotonic), keep project.yml in step
echo "$NEXT_BUILD" > "$BUILD_FILE"
sed -i '' -E "s/^( *CURRENT_PROJECT_VERSION: *)\"?[0-9]+\"?/\1\"$NEXT_BUILD\"/" "$PROJECT_YML"
ok "build number $NEXT_BUILD recorded in ios/build_number.txt and project.yml"

OPTIONS="$OUT/ExportOptions-$NEXT_BUILD.plist"
cp "$EXPORT_OPTIONS" "$OPTIONS"
DEST="export"; [[ $MODE == upload ]] && DEST="upload"
/usr/libexec/PlistBuddy -c "Set :destination $DEST" "$OPTIONS"

EXPORT_DIR="$OUT/export-$NEXT_BUILD"
echo "==> Exporting (destination: $DEST)"
set +e
xcodebuild -exportArchive \
  -archivePath "$ARCHIVE" \
  -exportOptionsPlist "$OPTIONS" \
  -exportPath "$EXPORT_DIR" \
  -allowProvisioningUpdates \
  ${AUTH[@]+"${AUTH[@]}"} 2>&1 | tee "$OUT/export-$NEXT_BUILD.log" | grep -E "error:|EXPORT (SUCCEEDED|FAILED)|Upload|upload"
status=${PIPESTATUS[0]}
set -e
if [[ $status != 0 ]]; then
  echo "==> Export failed (exit $status); see ${OUT#$ROOT/}/export-$NEXT_BUILD.log" >&2
  exit $status
fi

if [[ $DEST == upload ]]; then
  echo "==> Uploaded $VERSION ($NEXT_BUILD). It appears in App Store Connect → TestFlight after processing."
else
  echo "==> Exported: ${EXPORT_DIR#$ROOT/}/ (upload later with --upload, or Transporter)"
fi
