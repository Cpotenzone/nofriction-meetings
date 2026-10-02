#!/bin/bash
# noFriction Meetings - macOS Icon Generator
# Generates all required icon sizes from a source 1024x1024 PNG
# Usage: ./generate_icons.sh <source_1024x1024.png>

set -e

SOURCE="$1"
ICONS_DIR="$(dirname "$0")"

if [ -z "$SOURCE" ] || [ ! -f "$SOURCE" ]; then
    echo "Usage: $0 <source_1024x1024.png>"
    echo "  Provide a 1024x1024 PNG file as the source."
    exit 1
fi

echo "🎨 noFriction Icon Generator"
echo "Source: $SOURCE"
echo "Output: $ICONS_DIR"
echo ""

# ─── Step 1: Generate individual PNG sizes for Tauri ───────────────

echo "📐 Generating Tauri icon PNGs..."

sips -z 32 32 "$SOURCE" --out "$ICONS_DIR/32x32.png" > /dev/null 2>&1
echo "  ✓ 32x32.png"

sips -z 128 128 "$SOURCE" --out "$ICONS_DIR/128x128.png" > /dev/null 2>&1
echo "  ✓ 128x128.png"

sips -z 256 256 "$SOURCE" --out "$ICONS_DIR/128x128@2x.png" > /dev/null 2>&1
echo "  ✓ 128x128@2x.png (256px)"

cp "$SOURCE" "$ICONS_DIR/icon.png"
echo "  ✓ icon.png (1024px source copy)"

# ─── Step 2: Create .iconset for macOS .icns generation ────────────

ICONSET="$ICONS_DIR/icon.iconset"
mkdir -p "$ICONSET"

echo ""
echo "📦 Building macOS .iconset..."

sips -z 16 16     "$SOURCE" --out "$ICONSET/icon_16x16.png"      > /dev/null 2>&1
sips -z 32 32     "$SOURCE" --out "$ICONSET/icon_16x16@2x.png"   > /dev/null 2>&1
sips -z 32 32     "$SOURCE" --out "$ICONSET/icon_32x32.png"      > /dev/null 2>&1
sips -z 64 64     "$SOURCE" --out "$ICONSET/icon_32x32@2x.png"   > /dev/null 2>&1
sips -z 128 128   "$SOURCE" --out "$ICONSET/icon_128x128.png"    > /dev/null 2>&1
sips -z 256 256   "$SOURCE" --out "$ICONSET/icon_128x128@2x.png" > /dev/null 2>&1
sips -z 256 256   "$SOURCE" --out "$ICONSET/icon_256x256.png"    > /dev/null 2>&1
sips -z 512 512   "$SOURCE" --out "$ICONSET/icon_256x256@2x.png" > /dev/null 2>&1
sips -z 512 512   "$SOURCE" --out "$ICONSET/icon_512x512.png"    > /dev/null 2>&1
sips -z 1024 1024 "$SOURCE" --out "$ICONSET/icon_512x512@2x.png" > /dev/null 2>&1

echo "  ✓ 10 iconset images generated"

# ─── Step 3: Convert .iconset → .icns ──────────────────────────────

echo ""
echo "🔧 Converting .iconset → icon.icns..."
iconutil -c icns "$ICONSET" -o "$ICONS_DIR/icon.icns"
echo "  ✓ icon.icns created"

# ─── Step 4: Generate Windows .ico (optional) ──────────────────────

# macOS sips can't make .ico natively; we'll create a 256px PNG and
# let Tauri handle the conversion, or use ImageMagick if available
if command -v convert &> /dev/null; then
    echo ""
    echo "🪟 Generating Windows icon.ico (via ImageMagick)..."
    convert "$SOURCE" \
        \( -clone 0 -resize 16x16 \) \
        \( -clone 0 -resize 32x32 \) \
        \( -clone 0 -resize 48x48 \) \
        \( -clone 0 -resize 64x64 \) \
        \( -clone 0 -resize 128x128 \) \
        \( -clone 0 -resize 256x256 \) \
        -delete 0 "$ICONS_DIR/icon.ico"
    echo "  ✓ icon.ico created"
else
    echo ""
    echo "⚠️  ImageMagick not found. Skipping icon.ico generation."
    echo "   Install with: brew install imagemagick"
    echo "   Or the existing icon.ico will remain."
fi

# ─── Step 5: Generate Windows Store logos ──────────────────────────

echo ""
echo "🏪 Generating Windows Store logos..."
sips -z 30 30   "$SOURCE" --out "$ICONS_DIR/Square30x30Logo.png"   > /dev/null 2>&1
sips -z 44 44   "$SOURCE" --out "$ICONS_DIR/Square44x44Logo.png"   > /dev/null 2>&1
sips -z 71 71   "$SOURCE" --out "$ICONS_DIR/Square71x71Logo.png"   > /dev/null 2>&1
sips -z 89 89   "$SOURCE" --out "$ICONS_DIR/Square89x89Logo.png"   > /dev/null 2>&1
sips -z 107 107 "$SOURCE" --out "$ICONS_DIR/Square107x107Logo.png" > /dev/null 2>&1
sips -z 142 142 "$SOURCE" --out "$ICONS_DIR/Square142x142Logo.png" > /dev/null 2>&1
sips -z 150 150 "$SOURCE" --out "$ICONS_DIR/Square150x150Logo.png" > /dev/null 2>&1
sips -z 284 284 "$SOURCE" --out "$ICONS_DIR/Square284x284Logo.png" > /dev/null 2>&1
sips -z 310 310 "$SOURCE" --out "$ICONS_DIR/Square310x310Logo.png" > /dev/null 2>&1
sips -z 50 50   "$SOURCE" --out "$ICONS_DIR/StoreLogo.png"         > /dev/null 2>&1
echo "  ✓ 10 Windows Store logos generated"

# ─── Cleanup ───────────────────────────────────────────────────────

rm -rf "$ICONSET"
echo ""
echo "🗑️  Cleaned up intermediate .iconset directory"

# ─── Summary ───────────────────────────────────────────────────────

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "✅ All icons generated successfully!"
echo ""
echo "Files updated in: $ICONS_DIR/"
ls -la "$ICONS_DIR"/*.png "$ICONS_DIR"/*.icns "$ICONS_DIR"/*.ico 2>/dev/null | awk '{print "  " $NF " (" $5 " bytes)"}'
echo ""
echo "Run 'npx tauri build' to bundle with new icons."
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
