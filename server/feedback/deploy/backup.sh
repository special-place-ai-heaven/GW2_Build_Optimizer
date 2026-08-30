#!/usr/bin/env bash
# Nightly logical backup of the feedback database. Keeps 30 days.
# Cron (root on the VPS):  15 3 * * * /docker/feedback/deploy/backup.sh
set -euo pipefail
umask 077
COMPOSE=/docker/feedback/compose.yml
DIR=/docker/feedback/backups
mkdir -p "$DIR"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
TMP="$DIR/.feedback-$STAMP.tmp"
OUT="$DIR/feedback-$STAMP.dump"

# Dump to a temp name first: redirecting straight into $OUT creates the file
# before pg_dump can fail, leaving a truncated dump that looks like a backup.
if ! docker compose -f "$COMPOSE" exec -T db pg_dump -U feedback -d feedback -Fc > "$TMP"; then
  rm -f "$TMP"
  echo "backup FAILED: pg_dump did not complete; nothing written" >&2
  exit 1
fi
mv "$TMP" "$OUT"

find "$DIR" -name 'feedback-*.dump' -mtime +30 -delete
echo "backup ok: $OUT"
