# noFriction - macOS Release Runbook (Developer ID DMG)

## Overview

This document covers the **Developer ID (DMG) flavor**: building, signing,
notarizing and verifying a DMG distributed outside the App Store.

- The **Mac App Store** build (sandboxed `.pkg`, StoreKit, TestFlight) is a
  separate flavor: see [MAC_APP_STORE_BUILD.md](MAC_APP_STORE_BUILD.md).
- The end-to-end commercial launch (App Store Connect, TestFlight, review)
  is in [LAUNCH_CHECKLIST.md](LAUNCH_CHECKLIST.md) and
  [APP_STORE_RELEASE.md](APP_STORE_RELEASE.md).
- The DMG build has no StoreKit and no Pro gating; it can't check App Store
  purchases.

---

## 🔐 Required Secrets (GitHub Actions)

Configure these in **Settings > Secrets and Variables > Actions**:

| Secret | Description | How to Get |
|--------|-------------|------------|
| `APPLE_CERT_P12` | Base64-encoded Developer ID certificate | Export from Keychain, `base64 -i cert.p12` |
| `APPLE_CERT_PASSWORD` | Password for the .p12 file | Set when exporting |
| `APPLE_TEAM_ID` | 10-character Team ID | [Apple Developer Portal](https://developer.apple.com/account) |
| `APPLE_ID` | Apple ID email | Your Apple Developer email |
| `APPLE_APP_SPECIFIC_PASSWORD` | Notarization password | [appleid.apple.com](https://appleid.apple.com) > App-Specific Passwords |

### Export Certificate

```bash
# Find your Developer ID certificate
security find-identity -v -p codesigning

# Export to .p12 (you'll set a password)
# Do this in Keychain Access: Right-click cert > Export

# Base64 encode for GitHub secret
base64 -i ~/Desktop/Developer_ID_Application.p12 | pbcopy
# Now paste into APPLE_CERT_P12 secret
```

---

## 🖥️ Local Release Build

### Prerequisites

1. **Xcode Command Line Tools**
   ```bash
   xcode-select --install
   ```

2. **Developer ID Certificate** installed in Keychain

3. **Environment Variables**
   ```bash
   export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAM_ID)"
   export APPLE_TEAM_ID="XXXXXXXXXX"
   export APPLE_ID="your@email.com"
   export APPLE_APP_SPECIFIC_PASSWORD="xxxx-xxxx-xxxx-xxxx"
   ```

### Run Release Build

```bash
# from the repository root

# Full release (with notarization)
./scripts/release-macos.sh

# Skip notarization (for local testing)
SKIP_NOTARIZATION=1 ./scripts/release-macos.sh
```

### Output

```
dist/
├── noFriction-Meetings-<version>.dmg     # Final signed/notarized DMG
└── release-manifest.json              # Build metadata
```

---

## ✅ Verification Checklist

### 1. Verify Code Signature

```bash
# Verify .app bundle
codesign --verify --deep --strict --verbose=2 \
  "src-tauri/target/release/bundle/macos/noFriction.app"

# Expected output: "valid on disk" and "satisfies its Designated Requirement"
```

### 2. Verify DMG Signature

```bash
codesign --verify --verbose=2 dist/noFriction-Meetings-<version>.dmg
```

### 3. Gatekeeper Assessment

```bash
# Check if Gatekeeper will allow the app
spctl --assess --type execute --verbose=2 \
  "src-tauri/target/release/bundle/macos/noFriction.app"

# Check DMG
spctl --assess --type open --context context:primary-signature --verbose=2 \
  dist/noFriction-Meetings-<version>.dmg
```

### 4. Check Quarantine (simulated download)

```bash
# Add quarantine attribute (simulates browser download)
xattr -w com.apple.quarantine "0081;$(printf '%x' $(date +%s));Safari;$(uuidgen)" \
  dist/noFriction-Meetings-<version>.dmg

# Check it's set
xattr -l dist/noFriction-Meetings-<version>.dmg

# Should show quarantine attribute
```

### 5. Verify Notarization Staple

```bash
xcrun stapler validate dist/noFriction-Meetings-<version>.dmg
# Expected: "The validate action worked!"
```

---

## 🧪 Fresh Mac Test Procedure

This simulates a real user download experience.

### Setup

1. Use a **clean macOS VM** or a Mac that has never seen this app
2. Remove any existing installation:
   ```bash
   rm -rf /Applications/noFriction\ Meetings.app
   rm -rf ~/Library/Application\ Support/com.nofriction.meetings
   tccutil reset All com.nofriction.meetings
   ```

### Test Steps

1. **Download DMG** from browser (Safari/Chrome)
   - The file should have quarantine attribute

2. **Open DMG**
   - Should mount without "damaged image" warning
   - Should show app icon + Applications shortcut

3. **Drag to Applications**
   - Copy should complete normally

4. **First Launch** (from Applications, not from DMG)
   - ❌ Should NOT show "damaged" or "unidentified developer"
   - ✅ May show "downloaded from internet" dialog → Click "Open"

5. **Permission Prompts** (should appear at right time)
   
   | Permission | When Requested | System Preferences Path |
   |------------|----------------|-------------------------|
   | Microphone | When recording starts | Privacy > Microphone |
   | Screen & System Audio Recording | When capture starts | Privacy > Screen & System Audio Recording |
   | Calendar | Setup wizard / first calendar match | Privacy > Calendars |
   | Accessibility (DMG only, optional) | When screen-text capture is turned on | Privacy > Accessibility |

6. **Verify Functionality**
   - Start a recording
   - Confirm audio is captured
   - Confirm screen capture works (after granting permission)

### Common Issues

| Issue | Cause | Fix |
|-------|-------|-----|
| "App is damaged" | Not notarized or stapled | Re-run notarization |
| "Unidentified developer" | Not signed with Developer ID | Check signing identity |
| No permission prompt | TCC database cached | `tccutil reset All <bundle-id>` |
| Gatekeeper blocks | Not properly signed | Check `spctl --assess` |

---

## 📋 SHA256 Verification

```bash
# Calculate checksum
shasum -a 256 dist/noFriction-Meetings-<version>.dmg

# Compare with published checksum (from release-manifest.json or GitHub Release)
```

---

## 🔄 CI/CD Trigger

### Manual Trigger (GitHub Actions)

1. Go to **Actions** tab
2. Select **Build & Release macOS DMG**
3. Click **Run workflow**
4. Optionally check "Skip notarization" for testing

### Tag-based Release

```bash
git tag v<version>
git push origin v<version>
# Workflow runs automatically, creates GitHub Release with DMG
```

---

## 📁 File Inventory

| File | Purpose |
|------|---------|
| `scripts/release-macos.sh` | Local release build script |
| `.github/workflows/release-macos.yml` | CI/CD workflow |
| `src-tauri/entitlements.plist` | Hardened Runtime entitlements (Developer ID) |
| `src-tauri/Info.plist` | App bundle metadata + usage descriptions |
| `src-tauri/tauri.conf.json` | Tauri build configuration |

---

## 🔧 Troubleshooting

### "xcrun: error: unable to find utility 'notarytool'"

```bash
xcode-select --install
# or
sudo xcode-select -s /Applications/Xcode.app
```

### "The signature of the binary is invalid"

```bash
# Check what's wrong
codesign -vvv --deep --strict /path/to/app.app

# Common fix: sign frameworks inside Contents/Frameworks first
```

### Notarization fails with "Invalid signature"

```bash
# Check the app was signed with --options runtime (Hardened Runtime)
codesign -dv --verbose=4 /path/to/app.app | grep flags
# Should include "runtime"
```

### "App can't be opened because... cannot be verified"

The DMG wasn't notarized or the staple failed. Run:
```bash
xcrun stapler validate /path/to/file.dmg
```
