# feedback server — deploy

Rust + axum + PostgreSQL 16 for the addon's About tab (Message developer +
taxonomy). One app container and an unpublished Postgres `db`. Put Traefik (or
equivalent) in front.

Build from the **repo root** (the image embeds `data/feedback_taxonomy.json`):

    docker build -f server/feedback/Dockerfile -t gw2bo-feedback:latest .

Run next to `compose.yml` and a filled `.env` (see `.env.example`):

    docker compose up -d
    docker compose logs -f feedback

`compose.yml` has no `build:` block; it uses the `gw2bo-feedback:latest` tag.
Rebuild the image on the same host that runs compose, then:

    docker compose up -d --force-recreate feedback

Do not recreate `db` or delete the `pgdata` volume.

## Admin

`GET /admin` is a login page. Set `FEEDBACK_ADMIN_USER`,
`FEEDBACK_ADMIN_PASSWORD`, and `FEEDBACK_SESSION_SECRET`.
`Authorization: Bearer $FEEDBACK_ADMIN_TOKEN` still works on `/v1/admin/*`.
Set `FEEDBACK_TRUST_XFF=1` when Traefik sits in front so login and report
rate limits use the proxy-appended client IP.

## Verify

    curl -s https://feedback.robagentic.tech/healthz
    curl -s https://feedback.robagentic.tech/v1/taxonomy | head -c 200

## Backups

See `deploy/backup.sh`.
