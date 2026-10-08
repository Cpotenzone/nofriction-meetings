#!/usr/bin/env bash
# Deploy site/ to S3 and invalidate CloudFront.
#
# Required env:
#   BUCKET             S3 bucket name (e.g. nofriction-io-site)
#   DISTRIBUTION_ID    CloudFront distribution id (e.g. E1ABCDEF2GHIJ)
# Optional env:
#   AWS_REGION         default us-east-1
#   SITE_DIR           default <repo>/site
#   FILM_DIR           default <repo>/marketing/out (hero.mp4 / hero.webm copied into video/ if present)
#   DRY_RUN=1          print the aws commands instead of running them
#
# Usage:  BUCKET=nofriction-io-site DISTRIBUTION_ID=E1ABCDEF2GHIJ infra/site/deploy.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SITE_DIR="${SITE_DIR:-$ROOT/site}"
FILM_DIR="${FILM_DIR:-$ROOT/marketing/out}"
export AWS_REGION="${AWS_REGION:-us-east-1}"
: "${BUCKET:?set BUCKET}"
: "${DISTRIBUTION_ID:?set DISTRIBUTION_ID}"

run() { if [ "${DRY_RUN:-0}" = "1" ]; then echo "+ $*"; else "$@"; fi; }

# 1. Hero film: copy from marketing/out when present (the files are not committed).
mkdir -p "$SITE_DIR/video"
for f in hero.mp4 hero.webm; do
  if [ -f "$FILM_DIR/$f" ]; then
    cp "$FILM_DIR/$f" "$SITE_DIR/video/$f"
    echo "copied $f into site/video/"
  fi
done

# 2. Long-cached assets: images, video, css, js, fonts, badges, files. Content type from the extension.
LONG="public, max-age=31536000, immutable"
SHORT="public, max-age=300, must-revalidate"

sync_type() { # <include-glob> <content-type> <cache-control>
  run aws s3 sync "$SITE_DIR" "s3://$BUCKET" \
    --exclude "*" --include "$1" \
    --content-type "$2" --cache-control "$3" \
    --metadata-directive REPLACE --no-progress
}

sync_type "img/*.webp"   "image/webp"      "$LONG"
sync_type "img/*.png"    "image/png"       "$LONG"
sync_type "img/*.ico"    "image/x-icon"    "$LONG"
sync_type "img/badges/*.svg" "image/svg+xml" "$LONG"
sync_type "*.svg"        "image/svg+xml"   "$LONG"
sync_type "video/*.mp4"  "video/mp4"       "$LONG"
sync_type "video/*.webm" "video/webm"      "$LONG"
sync_type "css/*"        "text/css; charset=utf-8" "$LONG"
sync_type "style.css"    "text/css; charset=utf-8" "$SHORT"
sync_type "js/*.js"      "text/javascript; charset=utf-8" "$SHORT"
sync_type "files/*.docx" "application/vnd.openxmlformats-officedocument.wordprocessingml.document" "$LONG"

# 3. HTML and text: short cache so edits show within minutes.
sync_type "*.html"       "text/html; charset=utf-8" "$SHORT"
sync_type "sitemap.xml"  "application/xml; charset=utf-8" "$SHORT"
sync_type "robots.txt"   "text/plain; charset=utf-8" "$SHORT"

# 4. Remove anything no longer in site/ (README.md is never uploaded).
# video/ is gitignored, so a CI checkout has none: never let --delete remove
# the hero film that was uploaded from a machine that has it.
run aws s3 sync "$SITE_DIR" "s3://$BUCKET" --delete \
  --exclude "README.md" --exclude ".DS_Store" --exclude "video/*" --size-only --no-progress

# 5. Invalidate everything; the site is small and HTML is short-cached anyway.
run aws cloudfront create-invalidation --distribution-id "$DISTRIBUTION_ID" --paths "/*" \
  --query 'Invalidation.{Id:Id,Status:Status}' --output table

echo "deployed $SITE_DIR to s3://$BUCKET and invalidated $DISTRIBUTION_ID"
