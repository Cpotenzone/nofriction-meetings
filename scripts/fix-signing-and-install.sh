#!/bin/bash
#
# One-shot fix for "macOS keeps asking for mic / screen permission" and
# "other participants' audio isn't being captured".
#
# Cause: the signing certs had a custom "Always Trust" override in the login
# keychain, so codesign produced a signature the app itself fails. macOS keys
# permission grants to that signature, so grants never stuck.
#
# This script:
#   1. Removes the custom trust override from your Apple signing certs
#      (macOS will ask for your login password once per cert)
#   2. Builds + signs the desktop app (no notarization, local install)
#   3. Verifies the signature is Apple-anchored
#   4. Installs it to /Applications (old copy goes to the Trash)
#   5. Resets mic + screen permissions so you grant them once, cleanly
#
# Usage:  ./scripts/fix-signing-and-install.sh
#

set -euo pipefail

BLUE='\033[0;34m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; NC='\033[0m'
step() { echo -e "\n${BLUE}==>${NC} $1"; }
ok()   { echo -e "${GREEN}✓${NC} $1"; }
warn() { echo -e "${YELLOW}!${NC} $1"; }
die()  { echo -e "${RED}✗ $1${NC}"; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
APP_NAME="noFriction Meetings"
BUILT_APP="$PROJECT_ROOT/src-tauri/target/release/bundle/macos/$APP_NAME.app"
INSTALLED_APP="/Applications/$APP_NAME.app"
BUNDLE_ID="com.nofriction.meetings"   # from src-tauri/Info.plist
TEAM_ID="C7GCEESE2V"

# Certs that had "Always Trust" set (security dump-trust-settings)
CERTS=(
    "Developer ID Application: casey potenzone ($TEAM_ID)"
    "Mac Developer: casey potenzone"
    "3rd Party Mac Developer Installer: casey potenzone ($TEAM_ID)"
)

# ---------------------------------------------------------------------------
step "1/5  Removing custom trust overrides from signing certs"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

for name in "${CERTS[@]}"; do
    if ! security dump-trust-settings 2>/dev/null | grep -qF "$name"; then
        ok "$name: already using system defaults"
        continue
    fi
    pem="$TMP/cert.pem"
    if ! security find-certificate -c "$name" -p > "$pem" 2>/dev/null || [[ ! -s "$pem" ]]; then
        warn "$name: has a trust override but the cert isn't in your keychain; skipping"
        continue
    fi
    echo "   $name: macOS will ask for your login password…"
    security remove-trusted-cert "$pem" || die "Couldn't remove trust for $name (cancelled?). Re-run the script."
    ok "$name: trust reset to system defaults"
done

if security dump-trust-settings 2>/dev/null | grep -qF "Developer ID Application"; then
    die "Developer ID cert still has a custom trust setting. Open Keychain Access → the cert → Get Info → Trust → 'Use System Defaults', then re-run."
fi
ok "No custom trust on the Developer ID cert"

# ---------------------------------------------------------------------------
step "2/5  Building and signing the desktop app (takes a few minutes)"
osascript -e "tell application \"$APP_NAME\" to quit" >/dev/null 2>&1 || true
(cd "$PROJECT_ROOT" && SKIP_NOTARIZATION=1 ./scripts/release-macos.sh) \
    || die "Build failed; see output above."
[[ -d "$BUILT_APP" ]] || die "Built app not found at $BUILT_APP"

# ---------------------------------------------------------------------------
step "3/5  Verifying signature"
codesign --verify --deep --strict "$BUILT_APP" || die "Signature check failed"
DR="$(codesign -dr - "$BUILT_APP" 2>&1)"
echo "$DR" | grep -q "anchor apple generic" || die "Signature isn't Apple-anchored:\n$DR"
echo "$DR" | grep -q "$TEAM_ID" || die "Signature is missing team $TEAM_ID:\n$DR"
ok "Signature is valid and tied to team $TEAM_ID"

# ---------------------------------------------------------------------------
step "4/5  Installing to /Applications"
if [[ -d "$INSTALLED_APP" ]]; then
    TRASHED="$HOME/.Trash/$APP_NAME $(date +%Y%m%d-%H%M%S).app"
    mv "$INSTALLED_APP" "$TRASHED"
    ok "Old copy moved to the Trash"
fi
ditto "$BUILT_APP" "$INSTALLED_APP"
codesign --verify --deep --strict "$INSTALLED_APP" || die "Installed copy fails signature check"
ok "Installed $INSTALLED_APP"

# ---------------------------------------------------------------------------
step "5/5  Resetting mic + screen permissions for a clean grant"
tccutil reset Microphone "$BUNDLE_ID" >/dev/null 2>&1 || true
tccutil reset ScreenCapture "$BUNDLE_ID" >/dev/null 2>&1 || true
ok "Permissions reset"

open "$INSTALLED_APP"
echo -e "\n${GREEN}Done.${NC} The app is opening. Grant Microphone and Screen & System Audio"
echo "Recording once when asked (for screen recording macOS asks you to relaunch the app)."
echo "You shouldn't be asked again after that."
