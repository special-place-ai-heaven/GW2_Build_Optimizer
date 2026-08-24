# feedback server — deploy

Rust + axum + PostgreSQL 16 service for the GW2 Build Optimizer's About tab
(Message Developer + changelog taxonomy). Runs as one Docker container behind
Traefik on `srv1640039`, next to `db` (Postgres, unpublished).

## Deploy (first time)

Either paste `compose.yml` + the filled `.env` into Hostinger → VPS → Docker
Manager → Compose (project name `feedback`), or on the VPS as `svetipeter`:

    sudo mkdir -p /docker/feedback/deploy && cd /docker/feedback
    # copy compose.yml, .env.example -> .env (filled), deploy/backup.sh from the repo
    docker compose pull || docker compose build
    docker compose up -d
    docker compose logs -f feedback   # expect: "feedback listening on 0.0.0.0:8080"

## Verify

    curl -s https://feedback.robagentic.tech/healthz                 # ok
    curl -s https://feedback.robagentic.tech/v1/taxonomy | head -c 200
    curl -s -X POST https://feedback.robagentic.tech/v1/reports \
      -H 'content-type: application/json' -H 'x-addon-version: 1.6.0' \
      -d @server/feedback/deploy/sample-report.json               # 201 {"id":"...","status":"received"}
    curl -s -H "authorization: Bearer $FEEDBACK_ADMIN_TOKEN" \
      https://feedback.robagentic.tech/v1/admin/reports?status=received

## Update

    cd /docker/feedback && docker compose pull && docker compose up -d

## Backups

Install the cron line from `deploy/backup.sh`. Dumps land in `/docker/feedback/backups/`.
