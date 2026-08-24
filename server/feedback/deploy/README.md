# feedback server — deploy

Rust + axum + PostgreSQL 16 service for the GW2 Build Optimizer's About tab
(Message Developer + changelog taxonomy). Runs as one Docker container behind
Traefik on `srv1640039`, next to `db` (Postgres, unpublished).

## Deploy (first time)

There is no registry in play: the image is built on the dev machine and shipped
to the VPS over ssh. `compose.yml` therefore names a plain local tag
(`gw2bo-feedback:latest`) and carries no `build:` block — the repo is not
checked out on the VPS, so a relative build context there would resolve to
`/docker` and fail.

On the dev machine, from the repo root (the build context is the repo root
because the image embeds `data/feedback_taxonomy.json`):

    docker build -f server/feedback/Dockerfile -t gw2bo-feedback:latest .
    docker save gw2bo-feedback:latest | ssh ai-vps docker load

On the VPS as `svetipeter`:

    sudo mkdir -p /docker/feedback/deploy && cd /docker/feedback
    # copy compose.yml, .env.example -> .env (filled), deploy/backup.sh from the repo
    docker compose up -d
    docker compose logs -f feedback   # expect: "feedback listening on 0.0.0.0:8080"

Alternatively the compose text can be pasted into Hostinger → VPS → Docker
Manager → Compose (project name `feedback`), with the `.env` values filled in
there. The image still has to be loaded onto the VPS first by the `docker save`
step above — Docker Manager does not build it.

Later, a registry (ghcr) can replace the save/load step: push the image and
change the one `image:` line in `compose.yml` to the registry reference.

## Verify

    curl -s https://feedback.robagentic.tech/healthz                 # ok
    curl -s https://feedback.robagentic.tech/v1/taxonomy | head -c 200
    curl -s -X POST https://feedback.robagentic.tech/v1/reports \
      -H 'content-type: application/json' -H 'x-addon-version: 1.6.0' \
      -d @server/feedback/deploy/sample-report.json               # 201 {"id":"...","status":"received"}
    curl -s -H "authorization: Bearer $FEEDBACK_ADMIN_TOKEN" \
      https://feedback.robagentic.tech/v1/admin/reports?status=received

## Update

On the dev machine, rebuild and ship the new image:

    docker build -f server/feedback/Dockerfile -t gw2bo-feedback:latest .
    docker save gw2bo-feedback:latest | ssh ai-vps docker load

On the VPS, recreate the container against the freshly loaded image (compose
will not restart it on its own — the tag has not changed):

    cd /docker/feedback && docker compose up -d --force-recreate feedback

## Backups

Install the cron line from `deploy/backup.sh`. Dumps land in `/docker/feedback/backups/`.

## Go-live

Went live 2026-08-24. First report short id: `QP0GZ7ZQ` (deploy smoke test; replied and closed). Verified over HTTPS: Let's Encrypt cert, `/healthz` 200, taxonomy v1, POST 201 + idempotent replay, 426 for addon 1.5.3, 401 without token, admin get → `read`, reply → `answered`, close → `closed`, player status carries the reply.
