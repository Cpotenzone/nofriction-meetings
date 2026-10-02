#!/bin/bash
#
# noFriction Meetings - Mac App Store / Mac TestFlight build
#
# Builds the sandboxed `mas` flavor, signs it with Apple Distribution +
# the embedded provisioning profile, verifies it, packages a signed .pkg,
# and optionally uploads it to App Store Connect.
#
# The Developer ID DMG build is scripts/release-macos.sh (unchanged).
# Full guide: docs/MAC_APP_STORE_BUILD.md
#
# Usage:
#   scripts/release-mas.sh                 # release build → dist-mas/*.pkg
#   scripts/release-mas.sh --upload        # ... and upload to App Store Connect
#   scripts/release-mas.sh --local-test    # sandboxed .app for a local launch test
#                                          #   (no profile, no pkg, no build-number bump)
#   scripts/release-mas.sh --universal     # arm64 + x86_64 (default: this Mac's arch)
#   scripts/release-mas.sh --skip-build    # reuse the last build (sign/verify/package only)
#
# Release builds need:
#   src-tauri/embedded.provisionprofile  "Mac App Store Connect" profile for
#                                        com.nofriction.meetings (gitignored)
#   "Apple Distribution: …" and "3rd Party Mac Developer Installer: …" identities
#
# --upload needs (never hardcode these; nothing is stored in the repo):
#   ASC_APP_ID              numeric Apple ID of the app (App Store Connect → App Information)
#   and either
#     APPLE_ID              Apple ID email, with an app-specific password stored in the
#                           login keychain item $ALTOOL_KEYCHAIN_ITEM (default AC_PASSWORD):
#                           xcrun altool --store-password-in-keychain-item AC_PASSWORD -u "$APPLE_ID" -p <app-specific-password>
#   or
#     APPLE_API_KEY_ID + APPLE_API_ISSUER   App Store Connect API key
#                           (~/.appstoreconnect/private_keys/AuthKey_<id>.p8)
#
# Optional:
#   MAS_SIGNING_IDENTITY     default "Apple Distribution: casey potenzone (C7GCEESE2V)"
#   MAS_INSTALLER_IDENTITY   default "3rd Party Mac Developer Installer: casey potenzone (C7GCEESE2V)"
#   LOCAL_TEST_IDENTITY      identity for --local-test (default: Apple Development, else Developer ID)
#   MAS_BUILD_NUMBER         force a CFBundleVersion instead of bumping build_number.txt

set -euo pipefail

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; BLUE='\033[0;34m'; NC='\033[0m'
log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1" >&2; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TAURI_DIR="$PROJECT_ROOT/src-tauri"
TAURI_CONF="$TAURI_DIR/tauri.conf.json"
MAS_CONF="$TAURI_DIR/tauri.mas.conf.json"
ENTITLEMENTS="$TAURI_DIR/entitlements.mas.plist"
PROFILE="$TAURI_DIR/embedded.provisionprofile"
BUILD_NUMBER_FILE="$TAURI_DIR/build_number.txt"
OUTPUT_DIR="$PROJECT_ROOT/dist-mas"
TEAM_ID="C7GCEESE2V"

MAS_SIGNING_IDENTITY="${MAS_SIGNING_IDENTITY:-Apple Distribution: casey potenzone (C7GCEESE2V)}"
MAS_INSTALLER_IDENTITY="${MAS_INSTALLER_IDENTITY:-3rd Party Mac Developer Installer: casey potenzone (C7GCEESE2V)}"
ALTOOL_KEYCHAIN_ITEM="${ALTOOL_KEYCHAIN_ITEM:-AC_PASSWORD}"

LOCAL_TEST=0; UPLOAD=0; UNIVERSAL=0; SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --local-test) LOCAL_TEST=1 ;;
        --upload) UPLOAD=1 ;;
        --universal) UNIVERSAL=1 ;;
        --skip-build) SKIP_BUILD=1 ;;
        -h|--help) sed -n '2,45p' "$0"; exit 0 ;;
        *) log_error "Unknown option: $arg" ;;
    esac
done
if [[ $LOCAL_TEST == 1 && $UPLOAD == 1 ]]; then log_error "--local-test builds can't be uploaded"; fi

json() { python3 -c "import json,sys; print(json.load(open('$1'))$2)"; }
APP_NAME=$(json "$TAURI_CONF" "['productName']")
APP_VERSION=$(json "$TAURI_CONF" "['version']")
BUNDLE_ID=$(json "$TAURI_CONF" "['identifier']")
[[ "$BUNDLE_ID" == "com.nofriction.meetings" ]] || log_error "tauri.conf.json identifier is $BUNDLE_ID, expected com.nofriction.meetings"

if [[ $UNIVERSAL == 1 ]]; then
    TARGET_ARGS=(--target universal-apple-darwin)
    BUNDLE_DIR="$TAURI_DIR/target/universal-apple-darwin/release/bundle/macos"
else
    TARGET_ARGS=()
    BUNDLE_DIR="$TAURI_DIR/target/release/bundle/macos"
fi
APP_PATH="$BUNDLE_DIR/$APP_NAME.app"
TMP_DIR="$(mktemp -d -t nofriction-mas)"
trap 'rm -rf "$TMP_DIR"' EXIT

have_identity() { security find-identity -v -p "$2" 2>/dev/null | grep -qF "\"$1\""; }

# ---------------------------------------------------------------------------
preflight() {
    for tool in npx cargo codesign xcrun productbuild pkgutil python3 security /usr/libexec/PlistBuddy; do
        command -v "$tool" >/dev/null 2>&1 || log_error "$tool not found"
    done
    if [[ $LOCAL_TEST == 1 ]]; then
        if [[ -z "${LOCAL_TEST_IDENTITY:-}" ]]; then
            LOCAL_TEST_IDENTITY=$(security find-identity -v -p codesigning | grep -m1 "Apple Development" | sed 's/.*"\(.*\)"/\1/' || true)
            if [[ -z "$LOCAL_TEST_IDENTITY" ]]; then
            LOCAL_TEST_IDENTITY=$(security find-identity -v -p codesigning | grep -m1 "Developer ID Application" | sed 's/.*"\(.*\)"/\1/' || true)
        fi
        fi
        [[ -n "$LOCAL_TEST_IDENTITY" ]] || log_error "No Apple Development / Developer ID identity for the local test build"
        SIGN_IDENTITY="$LOCAL_TEST_IDENTITY"
        log_warn "LOCAL TEST build: signed with '$SIGN_IDENTITY', no provisioning profile, not for upload"
    else
        have_identity "$MAS_SIGNING_IDENTITY" codesigning || log_error "Signing identity not found: $MAS_SIGNING_IDENTITY"
        security find-identity -v 2>/dev/null | grep -qF "\"$MAS_INSTALLER_IDENTITY\"" \
            || log_error "Installer identity not found: $MAS_INSTALLER_IDENTITY"
        [[ -f "$PROFILE" ]] || log_error "Provisioning profile missing: $PROFILE
  Create it at developer.apple.com → Profiles → + → Mac App Store Connect → com.nofriction.meetings,
  download it and save it as src-tauri/embedded.provisionprofile (gitignored)."
        check_profile
        SIGN_IDENTITY="$MAS_SIGNING_IDENTITY"
    fi
    log_success "Preflight OK ($APP_NAME $APP_VERSION, $BUNDLE_ID)"
}

check_profile() {
    local plist="$TMP_DIR/profile.plist"
    security cms -D -i "$PROFILE" > "$plist" 2>/dev/null || log_error "Can't decode $PROFILE"
    local app_id; app_id=$(/usr/libexec/PlistBuddy -c "Print :Entitlements:com.apple.application-identifier" "$plist" 2>/dev/null || true)
    [[ "$app_id" == "$TEAM_ID.$BUNDLE_ID" ]] || log_error "Profile is for '$app_id', expected '$TEAM_ID.$BUNDLE_ID'"
    local expires; expires=$(/usr/libexec/PlistBuddy -c "Print :ExpirationDate" "$plist")
    if /usr/libexec/PlistBuddy -c "Print :ProvisionedDevices" "$plist" >/dev/null 2>&1; then
        log_error "Profile lists devices (development profile). Use a 'Mac App Store Connect' distribution profile."
    fi
    log_success "Provisioning profile OK ($app_id, expires $expires)"
}

# ---------------------------------------------------------------------------
build_number() {
    if [[ -n "${MAS_BUILD_NUMBER:-}" ]]; then
        BUILD_NUMBER="$MAS_BUILD_NUMBER"
    elif [[ $LOCAL_TEST == 1 ]]; then
        BUILD_NUMBER="$(tr -d '[:space:]' < "$BUILD_NUMBER_FILE")"
    else
        # Every upload needs a unique, increasing CFBundleVersion
        BUILD_NUMBER=$(( $(tr -d '[:space:]' < "$BUILD_NUMBER_FILE") + 1 ))
        echo "$BUILD_NUMBER" > "$BUILD_NUMBER_FILE"
    fi
    [[ "$BUILD_NUMBER" =~ ^[0-9]+(\.[0-9]+){0,2}$ ]] || log_error "Bad build number: $BUILD_NUMBER"
    log_info "CFBundleShortVersionString $APP_VERSION, CFBundleVersion $BUILD_NUMBER"
}

build() {
    local overlay="$TMP_DIR/overlay.json"
    if [[ $LOCAL_TEST == 1 ]]; then
        # No profile; Tauri signs ad hoc and we re-sign below
        cat > "$overlay" <<EOF
{"bundle":{"macOS":{"bundleVersion":"$BUILD_NUMBER","signingIdentity":"-","files":{"embedded.provisionprofile":null}}}}
EOF
    else
        cat > "$overlay" <<EOF
{"bundle":{"macOS":{"bundleVersion":"$BUILD_NUMBER"}}}
EOF
    fi
    if [[ $SKIP_BUILD == 1 ]]; then
        [[ -d "$APP_PATH" ]] || log_error "--skip-build: no app at $APP_PATH"
        log_warn "Skipping build; reusing $APP_PATH"
        return
    fi
    log_info "Building (features: mas)…"
    rm -rf "$APP_PATH"
    cd "$PROJECT_ROOT"
    npx tauri build --bundles app --features mas \
        --config "$MAS_CONF" --config "$overlay" "${TARGET_ARGS[@]+"${TARGET_ARGS[@]}"}"
    [[ -d "$APP_PATH" ]] || log_error "Build failed: no app at $APP_PATH"
    log_success "Built $APP_PATH"
}

# ---------------------------------------------------------------------------
sign() {
    local ents="$ENTITLEMENTS"
    if [[ $LOCAL_TEST == 1 ]]; then
        # application-identifier / team-identifier need a matching profile;
        # AMFI kills the app at launch without one. Strip them for local runs.
        ents="$TMP_DIR/entitlements.local.plist"
        cp "$ENTITLEMENTS" "$ents"
        /usr/libexec/PlistBuddy -c "Delete :com.apple.application-identifier" "$ents"
        /usr/libexec/PlistBuddy -c "Delete :com.apple.developer.team-identifier" "$ents"
        rm -f "$APP_PATH/Contents/embedded.provisionprofile"
    else
        cp "$PROFILE" "$APP_PATH/Contents/embedded.provisionprofile"
    fi
    log_info "Signing with '$SIGN_IDENTITY'…"
    # Nested code first (none today, but keep it correct if frameworks appear)
    if [[ -d "$APP_PATH/Contents/Frameworks" ]]; then
        find "$APP_PATH/Contents/Frameworks" \( -name "*.dylib" -o -name "*.framework" \) -print0 |
            while IFS= read -r -d '' lib; do
                codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$lib"
            done
    fi
    codesign --force --options runtime --timestamp \
        --entitlements "$ents" --sign "$SIGN_IDENTITY" "$APP_PATH"
    log_success "Signed"
}

verify() {
    log_info "Verifying…"
    codesign --verify --deep --strict --verbose=2 "$APP_PATH" || log_error "codesign --verify failed"

    local ents="$TMP_DIR/signed-entitlements.plist"
    codesign -d --entitlements - --xml "$APP_PATH" > "$ents" 2>/dev/null || log_error "Can't read entitlements"
    echo "---- entitlements ----"; plutil -p "$ents"; echo "----------------------"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.app-sandbox' "$ents" 2>/dev/null)" == "true" ]] \
        || log_error "com.apple.security.app-sandbox is missing"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.security.network.client' "$ents" 2>/dev/null)" == "true" ]] \
        || log_error "com.apple.security.network.client is missing"
    for forbidden in com.apple.security.cs.disable-library-validation com.apple.security.cs.allow-jit \
                     com.apple.security.cs.allow-unsigned-executable-memory; do
        if /usr/libexec/PlistBuddy -c "Print :$forbidden" "$ents" >/dev/null 2>&1; then
            log_error "$forbidden must not be in the App Store build"
        fi
    done
    if [[ $LOCAL_TEST == 0 ]]; then
        [[ "$(/usr/libexec/PlistBuddy -c 'Print :com.apple.application-identifier' "$ents")" == "$TEAM_ID.$BUNDLE_ID" ]] \
            || log_error "application-identifier doesn't match $TEAM_ID.$BUNDLE_ID"
        [[ -f "$APP_PATH/Contents/embedded.provisionprofile" ]] || log_error "embedded.provisionprofile missing from the bundle"
    fi

    local plist="$APP_PATH/Contents/Info.plist"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$plist")" == "$BUNDLE_ID" ]] || log_error "CFBundleIdentifier mismatch"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$plist")" == "$BUILD_NUMBER" ]] || log_error "CFBundleVersion is not $BUILD_NUMBER"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :ITSAppUsesNonExemptEncryption' "$plist")" == "false" ]] || log_error "ITSAppUsesNonExemptEncryption missing"
    [[ "$(/usr/libexec/PlistBuddy -c 'Print :LSApplicationCategoryType' "$plist")" == "public.app-category.productivity" ]] || log_error "LSApplicationCategoryType missing"

    # Sandbox blockers must be compiled out (m1 ffmpeg, m3 osascript, m4 ioreg, m7 shell plugin)
    local bin="$APP_PATH/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$plist")"
    local hits; hits=$(strings -a "$bin" | grep -E -i 'ffmpeg|ffprobe|osascript|ioreg -l|System Events|plugin:shell|tauri-plugin-shell|/opt/homebrew' || true)
    [[ -z "$hits" ]] || { echo "$hits" | head -20; log_error "Binary still references sandbox-incompatible tools (above)"; }
    if strings -a "$bin" | grep -q 'AXUIElementCopyAttributeValue'; then
        log_error "Binary still links the Accessibility (AX) API"
    fi
    if ! otool -l "$bin" | grep -A2 LC_LOAD_WEAK_DYLIB | grep -q FoundationModels; then
        log_warn "FoundationModels is not weak-linked; the app would fail to launch on macOS < 26"
    fi
    log_success "Verification passed"
}

package() {
    mkdir -p "$OUTPUT_DIR"
    PKG_PATH="$OUTPUT_DIR/${APP_NAME// /-}-${APP_VERSION}-${BUILD_NUMBER}.pkg"
    log_info "Packaging $PKG_PATH…"
    productbuild --component "$APP_PATH" /Applications --sign "$MAS_INSTALLER_IDENTITY" "$PKG_PATH"
    pkgutil --check-signature "$PKG_PATH" || log_error "pkg signature check failed"
    log_success "Package: $PKG_PATH"
}

upload() {
    [[ -n "${ASC_APP_ID:-}" ]] || log_error "ASC_APP_ID (numeric app Apple ID from App Store Connect) is required for --upload"
    local auth=()
    if [[ -n "${APPLE_API_KEY_ID:-}" && -n "${APPLE_API_ISSUER:-}" ]]; then
        auth=(--api-key "$APPLE_API_KEY_ID" --api-issuer "$APPLE_API_ISSUER")
    elif [[ -n "${APPLE_ID:-}" ]]; then
        security find-generic-password -s "$ALTOOL_KEYCHAIN_ITEM" >/dev/null 2>&1 \
            || log_error "Keychain item '$ALTOOL_KEYCHAIN_ITEM' not found. Create it with:
  xcrun altool --store-password-in-keychain-item $ALTOOL_KEYCHAIN_ITEM -u \"\$APPLE_ID\" -p <app-specific-password>"
        auth=(--username "$APPLE_ID" --password "@keychain:$ALTOOL_KEYCHAIN_ITEM" --team-id "$TEAM_ID")
    else
        log_error "Set APPLE_ID (+ keychain item $ALTOOL_KEYCHAIN_ITEM) or APPLE_API_KEY_ID + APPLE_API_ISSUER for --upload"
    fi
    log_info "Uploading to App Store Connect…"
    xcrun altool --upload-package "$PKG_PATH" --type macos \
        --apple-id "$ASC_APP_ID" --bundle-id "$BUNDLE_ID" \
        --bundle-version "$BUILD_NUMBER" --bundle-short-version-string "$APP_VERSION" \
        "${auth[@]}"
    log_success "Uploaded. Processing takes 5-30 min; then add the build to a TestFlight group."
}

# ---------------------------------------------------------------------------
preflight
build_number
build
sign
verify
if [[ $LOCAL_TEST == 1 ]]; then
    log_success "Local test app: $APP_PATH"
    echo "Launch it (NOT from /Applications):  open \"$APP_PATH\""
    echo "Sandbox container: ~/Library/Containers/$BUNDLE_ID"
    echo "App log: ~/Library/Containers/$BUNDLE_ID/Data/Library/Application Support/$BUNDLE_ID/logs/app.log"
    exit 0
fi
package
if [[ $UPLOAD == 1 ]]; then upload; fi
log_success "Done: $PKG_PATH (CFBundleVersion $BUILD_NUMBER)"
