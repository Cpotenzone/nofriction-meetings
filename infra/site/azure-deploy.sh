#!/usr/bin/env bash
# Mirror site/ to the dedicated Azure static website using GitHub OIDC/AAD.
# No storage account keys or SAS tokens are used.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SITE_DIR="${SITE_DIR:-$ROOT/site}"
FILM_DIR="${FILM_DIR:-$ROOT/marketing/out}"
AZURE_STORAGE_ACCOUNT="${AZURE_STORAGE_ACCOUNT:-canofrictione242106d}"
LONG="public, max-age=31536000, immutable"
SHORT="public, max-age=300, must-revalidate"

mkdir -p "$SITE_DIR/video"
for name in hero.mp4 hero.webm; do
  if [ -f "$FILM_DIR/$name" ]; then
    cp "$FILM_DIR/$name" "$SITE_DIR/video/$name"
  fi
done

# Refuse files without an explicit public content contract.
while IFS= read -r path; do
  relative="${path#"$SITE_DIR"/}"
  case "$relative" in
    README.md|.DS_Store|*.html|*.webp|*.png|*.ico|*.svg|*.mp4|*.webm|*.docx|style.css|js/*.js|sitemap.xml|robots.txt) ;;
    *) echo "unsupported static object: $relative" >&2; exit 1 ;;
  esac
done < <(find "$SITE_DIR" -type f -print)

az storage blob sync \
  --auth-mode login \
  --account-name "$AZURE_STORAGE_ACCOUNT" \
  --container '$web' \
  --source "$SITE_DIR" \
  --delete-destination true \
  --exclude-path 'video' \
  --exclude-pattern 'README.md;.DS_Store' \
  --only-show-errors \
  --output none

upload_type() {
  local pattern="$1"
  local content_type="$2"
  local cache_control="$3"
  az storage blob upload-batch \
    --auth-mode login \
    --account-name "$AZURE_STORAGE_ACCOUNT" \
    --destination '$web' \
    --source "$SITE_DIR" \
    --pattern "$pattern" \
    --content-type "$content_type" \
    --content-cache-control "$cache_control" \
    --overwrite true \
    --only-show-errors \
    --output none
}

upload_type '*.webp' 'image/webp' "$LONG"
upload_type '*.png' 'image/png' "$LONG"
upload_type '*.ico' 'image/x-icon' "$LONG"
upload_type '*.svg' 'image/svg+xml' "$LONG"
upload_type '*.mp4' 'video/mp4' "$LONG"
upload_type '*.webm' 'video/webm' "$LONG"
upload_type '*.docx' 'application/vnd.openxmlformats-officedocument.wordprocessingml.document' "$LONG"
upload_type '*.html' 'text/html; charset=utf-8' "$SHORT"
upload_type 'style.css' 'text/css; charset=utf-8' "$SHORT"
upload_type 'js/*.js' 'text/javascript; charset=utf-8' "$SHORT"
upload_type 'sitemap.xml' 'application/xml; charset=utf-8' "$SHORT"
upload_type 'robots.txt' 'text/plain; charset=utf-8' "$SHORT"

echo "published $SITE_DIR to https://${AZURE_STORAGE_ACCOUNT}.z21.web.core.windows.net/"
