#!/bin/bash
#
# Nano Banana Meetings - macOS Release Script
# Creates a signed, notarized DMG for distribution
#
# Usage:
#   ./scripts/release-macos.sh
#
# Required environment variables:
#   APPLE_SIGNING_IDENTITY  - "Developer ID Application: Your Name (TEAM_ID)"
#   APPLE_TEAM_ID           - Your Apple Team ID (10 characters)
#   APPLE_ID                - Your Apple ID email
#   APPLE_APP_SPECIFIC_PASSWORD - App-specific password for notarytool
#
# Optional:
#   SKIP_NOTARIZATION=1     - Skip notarization for local testing
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

# ---------------------------------------------------------
# Configuration (auto-read from tauri.conf.json)
# ---------------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TAURI_DIR="$PROJECT_ROOT/src-tauri"
TAURI_CONF="$TAURI_DIR/tauri.conf.json"

# Parse app info from tauri.conf.json
APP_NAME=$(python3 -c "import json; print(json.load(open('$TAURI_CONF'))['productName'])")
APP_VERSION=$(python3 -c "import json; print(json.load(open('$TAURI_CONF'))['version'])")
BUNDLE_ID=$(python3 -c "import json; print(json.load(open('$TAURI_CONF'))['identifier'])")

# Derived paths
APP_BUNDLE="$APP_NAME.app"
DMG_NAME="${APP_NAME// /-}-${APP_VERSION}.dmg"
DMG_VOLUME_NAME="$APP_NAME $APP_VERSION"
BUILD_DIR="$TAURI_DIR/target/release/bundle/macos"
OUTPUT_DIR="$PROJECT_ROOT/dist"

log_info "=================================="
log_info "Nano Banana Meetings Release Build"
log_info "=================================="
log_info "App Name: $APP_NAME"
log_info "Version: $APP_VERSION"
log_info "Bundle ID: $BUNDLE_ID"
log_info ""

# ---------------------------------------------------------
# Validate Environment
# ---------------------------------------------------------
validate_env() {
    log_info "Validating environment..."
    
    # Check for signing identity
    if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
        # Try to auto-detect
        DETECTED_IDENTITY=$(security find-identity -v -p codesigning | grep "Developer ID Application" | head -1 | sed 's/.*"\(.*\)"/\1/' || true)
        if [[ -n "$DETECTED_IDENTITY" ]]; then
            log_warn "Auto-detected signing identity: $DETECTED_IDENTITY"
            APPLE_SIGNING_IDENTITY="$DETECTED_IDENTITY"
        else
            log_error "APPLE_SIGNING_IDENTITY not set and could not auto-detect. Run: security find-identity -v -p codesigning"
        fi
    fi
    
    # Check for notarization credentials (unless skipped)
    if [[ -z "${SKIP_NOTARIZATION:-}" ]]; then
        if [[ -z "${APPLE_TEAM_ID:-}" ]] || [[ -z "${APPLE_ID:-}" ]] || [[ -z "${APPLE_APP_SPECIFIC_PASSWORD:-}" ]]; then
            log_warn "Notarization credentials not set. Set SKIP_NOTARIZATION=1 to skip."
            log_error "Required: APPLE_TEAM_ID, APPLE_ID, APPLE_APP_SPECIFIC_PASSWORD"
        fi
    fi
    
    # Check for required tools
    command -v npm >/dev/null 2>&1 || log_error "npm not found"
    command -v cargo >/dev/null 2>&1 || log_error "cargo not found"
    command -v codesign >/dev/null 2>&1 || log_error "codesign not found"
    command -v xcrun >/dev/null 2>&1 || log_error "xcrun not found"
    command -v hdiutil >/dev/null 2>&1 || log_error "hdiutil not found"
    
    log_success "Environment validated"
}

# ---------------------------------------------------------
# Clean Build
# ---------------------------------------------------------
clean_build() {
    log_info "Cleaning previous builds..."
    rm -rf "$BUILD_DIR"
    rm -rf "$OUTPUT_DIR"
    mkdir -p "$OUTPUT_DIR"
    log_success "Clean complete"
}

# ---------------------------------------------------------
# Build Release
# ---------------------------------------------------------
build_release() {
    log_info "Building Tauri release..."
    cd "$PROJECT_ROOT"
    
    # Build with Tauri
    npm run tauri build -- --bundles app
    
    if [[ ! -d "$BUILD_DIR/$APP_BUNDLE" ]]; then
        log_error "Build failed - app bundle not found at $BUILD_DIR/$APP_BUNDLE"
    fi
    
    log_success "Build complete"
}

# ---------------------------------------------------------
# Sign the Application
# ---------------------------------------------------------
sign_app() {
    log_info "Signing application with Hardened Runtime..."
    
    local APP_PATH="$BUILD_DIR/$APP_BUNDLE"
    local ENTITLEMENTS="$TAURI_DIR/entitlements.plist"
    
    # Sign all nested code first (frameworks, dylibs)
    log_info "Signing nested components..."
    
    # Sign frameworks
    if [[ -d "$APP_PATH/Contents/Frameworks" ]]; then
        find "$APP_PATH/Contents/Frameworks" -type f -name "*.dylib" -o -name "*.framework" 2>/dev/null | while read -r lib; do
            log_info "  Signing: $(basename "$lib")"
            codesign --force --options runtime --timestamp \
                --sign "$APPLE_SIGNING_IDENTITY" \
                --entitlements "$ENTITLEMENTS" \
                "$lib" || true
        done
    fi
    
    # Sign any standalone binaries in MacOS
    find "$APP_PATH/Contents/MacOS" -type f -perm +111 2>/dev/null | while read -r binary; do
        log_info "  Signing: $(basename "$binary")"
        codesign --force --options runtime --timestamp \
            --sign "$APPLE_SIGNING_IDENTITY" \
            --entitlements "$ENTITLEMENTS" \
            "$binary"
    done
    
    # Sign the main app bundle
    log_info "Signing main app bundle..."
    codesign --force --deep --options runtime --timestamp \
        --sign "$APPLE_SIGNING_IDENTITY" \
        --entitlements "$ENTITLEMENTS" \
        "$APP_PATH"
    
    # Verify signature
    log_info "Verifying signature..."
    codesign --verify --deep --strict --verbose=2 "$APP_PATH" || log_warn "Signature verification failed (proceeding anyway)"
    
    log_success "Application signed"
}

# ---------------------------------------------------------
# Create DMG
# ---------------------------------------------------------
create_dmg() {
    log_info "Creating DMG..."
    
    local APP_PATH="$BUILD_DIR/$APP_BUNDLE"
    local DMG_PATH="$OUTPUT_DIR/$DMG_NAME"
    local DMG_TEMP="$OUTPUT_DIR/temp.dmg"
    local MOUNT_POINT="/Volumes/$DMG_VOLUME_NAME"
    
    # Clean up any existing mount
    if [[ -d "$MOUNT_POINT" ]]; then
        hdiutil detach "$MOUNT_POINT" -force 2>/dev/null || true
    fi
    
    # Create temporary DMG
    hdiutil create -srcfolder "$APP_PATH" \
        -volname "$DMG_VOLUME_NAME" \
        -fs HFS+ \
        -fsargs "-c c=64,a=16,e=16" \
        -format UDRW \
        "$DMG_TEMP"
    
    # Mount it
    hdiutil attach "$DMG_TEMP" -readwrite -noverify -noautoopen
    
    # Wait for mount
    sleep 2
    
    # Create Applications symlink
    ln -sf /Applications "$MOUNT_POINT/Applications"
    
    # Set window position and size (AppleScript)
    osascript << EOF
tell application "Finder"
    tell disk "$DMG_VOLUME_NAME"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set bounds of container window to {400, 100, 920, 440}
        set theViewOptions to the icon view options of container window
        set arrangement of theViewOptions to not arranged
        set icon size of theViewOptions to 80
        set position of item "$APP_BUNDLE" of container window to {130, 150}
        set position of item "Applications" of container window to {390, 150}
        update without registering applications
        close
    end tell
end tell
EOF
    
    # Sync and unmount
    sync
    hdiutil detach "$MOUNT_POINT"
    
    # Convert to compressed final DMG
    hdiutil convert "$DMG_TEMP" \
        -format UDZO \
        -imagekey zlib-level=9 \
        -o "$DMG_PATH"
    
    rm -f "$DMG_TEMP"
    
    log_success "DMG created: $DMG_PATH"
}

# ---------------------------------------------------------
# Sign DMG
# ---------------------------------------------------------
sign_dmg() {
    log_info "Signing DMG..."
    
    local DMG_PATH="$OUTPUT_DIR/$DMG_NAME"
    
    codesign --force --timestamp \
        --sign "$APPLE_SIGNING_IDENTITY" \
        "$DMG_PATH"
    
    log_success "DMG signed"
}

# ---------------------------------------------------------
# Notarize App
# ---------------------------------------------------------
notarize() {
    if [[ -n "${SKIP_NOTARIZATION:-}" ]]; then
        log_warn "Skipping notarization (SKIP_NOTARIZATION is set)"
        return
    fi
    
    log_info "Notarizing DMG (this may take several minutes)..."
    
    local DMG_PATH="$OUTPUT_DIR/$DMG_NAME"
    
    # Submit for notarization
    xcrun notarytool submit "$DMG_PATH" \
        --apple-id "$APPLE_ID" \
        --password "$APPLE_APP_SPECIFIC_PASSWORD" \
        --team-id "$APPLE_TEAM_ID" \
        --wait
    
    # Staple the notarization ticket
    log_info "Stapling notarization ticket..."
    xcrun stapler staple "$DMG_PATH"
    
    # Verify staple
    xcrun stapler validate "$DMG_PATH"
    
    log_success "Notarization complete"
}

# ---------------------------------------------------------
# Verify Final Build
# ---------------------------------------------------------
verify_final() {
    log_info "Running final verification..."
    
    local DMG_PATH="$OUTPUT_DIR/$DMG_NAME"
    
    # Verify DMG signature
    log_info "Verifying DMG signature..."
    codesign --verify --verbose=2 "$DMG_PATH"
    
    # Gatekeeper assessment
    log_info "Running Gatekeeper assessment..."
    spctl --assess --type open --context context:primary-signature --verbose=2 "$DMG_PATH" || log_warn "Gatekeeper check failed (may be OK for unsigned DMGs)"
    
    # Calculate checksum
    log_info "Calculating SHA256 checksum..."
    local SHA256=$(shasum -a 256 "$DMG_PATH" | cut -d' ' -f1)
    
    echo ""
    log_success "=================================="
    log_success "     RELEASE BUILD COMPLETE"
    log_success "=================================="
    echo ""
    echo "Artifact: $DMG_PATH"
    echo "Size:     $(du -h "$DMG_PATH" | cut -f1)"
    echo "SHA256:   $SHA256"
    echo ""
    
    # Save manifest
    cat > "$OUTPUT_DIR/release-manifest.json" << EOF
{
    "appName": "$APP_NAME",
    "version": "$APP_VERSION",
    "bundleId": "$BUNDLE_ID",
    "dmgFile": "$DMG_NAME",
    "sha256": "$SHA256",
    "buildDate": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "signedBy": "$APPLE_SIGNING_IDENTITY"
}
EOF
    
    log_success "Manifest saved to $OUTPUT_DIR/release-manifest.json"
}

# ---------------------------------------------------------
# Main
# ---------------------------------------------------------
main() {
    validate_env
    clean_build
    build_release
    sign_app
    create_dmg
    sign_dmg
    notarize
    verify_final
}

main "$@"
