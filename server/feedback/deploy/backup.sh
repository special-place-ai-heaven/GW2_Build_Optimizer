#!/usr/bin/env bash
# Nightly logical backup of the feedback database. Keeps 30 days.
# Cron (root on the VPS):  15 3 * * * /docker/feedback/deploy/backup.sh
set -euo pipefail
DIR=/docker/feedback/backups
mkdir -p "$DIR"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
docker compose -f /docker/feedback/compose.yml exec -T db pg_dump -U feedback -d feedback -Fc > "$DIR/feedback-$STAMP.dump"
find "$DIR" -name 'feedback-*.dump' -mtime +30 -delete
echo "backup ok: $DIR/feedback-$STAMP.dump"
