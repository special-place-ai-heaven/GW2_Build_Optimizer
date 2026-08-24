# Feedback Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up `feedback.robagentic.tech` — a Postgres-backed HTTP service on the existing VPS that receives player reports from the GW2 Build Optimizer addon, serves the category taxonomy, and lets the developer read and reply.

**Architecture:** One Rust binary (`server/feedback`, axum + sqlx) and one `postgres:16-alpine`, deployed together by Docker Compose behind the VPS's existing Traefik. Public endpoints are unauthenticated and rate-limited; admin endpoints take a bearer token from the environment. The addon is not touched by this plan — the API is verified with curl.

**Tech Stack:** Rust 2021 (toolchain 1.97), axum 0.8, tokio 1, sqlx 0.8 (postgres, runtime-tokio, tls-rustls, migrate, uuid, chrono, json), serde/serde_json, uuid 1, sha2 0.10, rand 0.8, tower 0.5, tower-http 0.6 (limit), tracing. PostgreSQL 16. Docker Compose v2. Traefik (already on the VPS).

**Spec:** `docs/superpowers/specs/2026-08-24-feedback-and-about-design.md` — sections 5 (taxonomy), 6 (payload/schema), 6a (statuses), 7 (server), 10 (errors), 11 (testing), 12 (rollout step 1).

## Global Constraints

- The server lives in `server/feedback/` with its own `Cargo.toml` containing an empty `[workspace]` table; the root workspace adds `exclude = ["server/feedback"]`. It must never be linked into the DLL.
- Database: PostgreSQL 16 only. The `db` container exposes **no host port**. Password and admin token come from `/docker/feedback/.env` on the VPS and are never committed (`server/feedback/.env.example` holds placeholders only).
- Queries use runtime `sqlx::query(...)` with `.bind`, not the `query!` macros — the macros need a live database or offline metadata at every build, including on the developer's Windows machine and in CI. Every query is exercised by a `#[sqlx::test]` instead. (Deliberate simplification of the spec's "compile-time checked" wording.)
- Limits copied from the spec: body cap 16 KB; 10 requests/min per `ip_hash` (POST **and** status GET share the bucket); 50/day per `client_id` (best-effort, honor-system); report `body` ≤ 4000 chars; `title` ≤ 120 chars; `build_snapshot` ≤ 6 KB serialized; send timeout on the addon side 5 s (informs nothing here, but responses must be fast).
- Status vocabulary, verbatim: `received`, `read`, `answered`, `closed`.
- Unknown category/choice ids are **accepted and stored** with `unvalidated = true`, never rejected (older DLL vs newer taxonomy and vice versa must not lose reports).
- Nothing in this plan touches `crates/`, `Cargo.toml` version, README download links, or the release process. No DLL release results from this plan.
- No Claude attribution trailers in commits. PRs are opened, not merged, by the implementer.
- Commit messages: conventional prefix (`feat:`, `test:`, `chore:`, `ci:`, `docs:`), imperative, specific.

---

## File structure

```
server/feedback/
├── Cargo.toml                 own package; [workspace] = {}
├── .env.example               placeholders only
├── Dockerfile                 multi-stage, static binary on debian-slim
├── compose.yml                feedback + db, Traefik labels
├── deploy/
│   ├── README.md              VPS steps, discovery commands, backup cron
│   └── backup.sh              pg_dump -Fc, 30-day retention
├── migrations/
│   └── 0001_init.sql          reports + taxonomy tables
├── src/
│   ├── main.rs                config from env, pool, migrate, seed taxonomy, serve
│   ├── app.rs                 router + shared AppState
│   ├── config.rs              Config::from_env()
│   ├── error.rs               ApiError → (status, JSON {error, reason})
│   ├── ids.rs                 short_id(), ip_hash()
│   ├── ratelimit.rs           sliding-window limiter (in-memory)
│   ├── taxonomy.rs            load/seed/get taxonomy, validate ids
│   ├── reports.rs             POST /v1/reports, GET /v1/reports/status
│   └── admin.rs               bearer auth + admin routes
└── tests/
    └── api.rs                 end-to-end through the router with a real Postgres
data/
└── feedback_taxonomy.json     shared with the addon later; seeded into the DB
.github/workflows/
└── feedback-server.yml        fmt + clippy + tests with a Postgres service
```

`main.rs` wires; every other file owns one concern and is testable through `app::router(state)`.

---

### Task 0: Regain VPS access and discover Traefik's names

Nothing in later tasks can be deployed without this. It produces two values every Traefik label depends on and cannot be guessed.

**Files:**
- Create: `server/feedback/deploy/README.md` (first section only; extended in Task 9)

**Interfaces:**
- Produces: `TRAEFIK_NETWORK` (Docker network name Traefik attaches to) and `CERT_RESOLVER` (Traefik certificate resolver name), recorded in `deploy/README.md` and used verbatim in `compose.yml` (Task 9).

- [ ] **Step 1: Confirm which key the VPS accepts**

From the developer's own shell (not this session — the session's key is denied for both users):

```bash
ssh -o BatchMode=yes ai-vps 'echo ok; whoami'
```

Expected: `ok` and a username. If `Permission denied (publickey)`, add the public key from `~/.ssh/vps_ai_ed25519.pub` to the VPS via the Hostinger panel (VPS → SSH keys → attach) or paste it into `~/.ssh/authorized_keys` for that user from the Hostinger browser terminal, then re-run.

- [ ] **Step 2: Discover the Traefik network and cert resolver**

```bash
ssh ai-vps '
  echo "--- containers ---"; docker ps --format "{{.Names}}\t{{.Image}}";
  echo "--- traefik networks ---"; docker inspect $(docker ps -q --filter ancestor=traefik --filter name=traefik | head -1) --format "{{range \$k,\$v := .NetworkSettings.Networks}}{{\$k}}{{println}}{{end}}";
  echo "--- traefik args/labels mentioning certresolver ---"; docker inspect $(docker ps -q --filter name=traefik | head -1) --format "{{json .Args}} {{json .Config.Cmd}}" | tr "," "\n" | grep -i -E "certresolver|certificatesresolvers" ;
  echo "--- an existing service that already works (agentmemory) ---"; docker inspect $(docker ps -q --filter name=agentmemory | head -1) --format "{{range \$k,\$v := .Config.Labels}}{{\$k}}={{\$v}}{{println}}{{end}}" | grep -i traefik;
  echo "--- compose dirs ---"; ls /docker'
```

Expected: one network name Traefik is attached to (e.g. `traefik_default` or `proxy`), and one resolver name appearing in `--certificatesresolvers.<NAME>.acme...` and in agentmemory's `traefik.http.routers.*.tls.certresolver=<NAME>` label. Copy agentmemory's label set — it is the proven template on this box.

- [ ] **Step 3: Record the values**

Create `server/feedback/deploy/README.md`:

```markdown
# Deploying the feedback server

## Values discovered on srv1640039 (Task 0)

| Name | Value | Where it came from |
|---|---|---|
| `TRAEFIK_NETWORK` | `<paste from Step 2>` | `docker inspect traefik` networks |
| `CERT_RESOLVER` | `<paste from Step 2>` | `--certificatesresolvers.<name>` / agentmemory labels |
| Compose root | `/docker/feedback/` | same layout as `/docker/agentmemory/` |

These two names are used verbatim in `compose.yml`. If Traefik is ever
reconfigured, update them here and in `.env`.
```

(The two `<paste from Step 2>` cells are filled in during this step with the real output; they are the only content of this task.)

- [ ] **Step 4: Commit**

```bash
git add server/feedback/deploy/README.md
git commit -m "docs: record Traefik network and cert resolver for feedback deploy"
```

---

### Task 1: Package scaffold with `/healthz`

**Files:**
- Create: `server/feedback/Cargo.toml`
- Create: `server/feedback/src/main.rs`
- Create: `server/feedback/src/app.rs`
- Create: `server/feedback/src/config.rs`
- Create: `server/feedback/src/error.rs`
- Create: `server/feedback/tests/api.rs`
- Modify: `Cargo.toml` (root) — add `exclude`

**Interfaces:**
- Produces: `app::router(state: AppState) -> axum::Router`, `app::AppState { pool: PgPool, config: Arc<Config>, limiter: Arc<RateLimiter>, taxonomy: Arc<RwLock<Taxonomy>> }` (fields added by later tasks; this task creates it with `pool` and `config` only), `config::Config::from_env() -> Result<Config, String>`, `error::ApiError` with `impl IntoResponse`.

- [ ] **Step 1: Exclude the server from the root workspace**

In the root `Cargo.toml`, under `[workspace]`, add:

```toml
exclude = ["server/feedback"]
```

Run: `cargo check` (root) — Expected: unchanged, still compiles; the exclude is inert until the directory exists.

- [ ] **Step 2: Create the package**

`server/feedback/Cargo.toml`:

```toml
[package]
name = "gw2bo-feedback"
version = "0.1.0"
edition = "2021"
rust-version = "1.85"
publish = false

# Standalone package inside the addon repo; deliberately NOT a workspace member.
[workspace]

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "signal"] }
sqlx = { version = "0.8", default-features = false, features = ["postgres", "runtime-tokio", "tls-rustls", "migrate", "uuid", "chrono", "json", "macros"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
sha2 = "0.10"
rand = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["limit", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
http-body-util = "0.1"
```

- [ ] **Step 3: Write the failing healthz test**

`server/feedback/tests/api.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use gw2bo_feedback::app::{router, AppState};
use gw2bo_feedback::config::Config;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

fn test_config() -> Arc<Config> {
    Arc::new(Config {
        database_url: String::new(), // pool is injected by sqlx::test
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        admin_token: "test-admin-token".into(),
        ip_salt: "test-salt".into(),
        min_addon_version: "1.6.0".into(),
    })
}

async fn state(pool: PgPool) -> AppState {
    AppState::new(pool, test_config()).await
}

#[sqlx::test(migrations = "./migrations")]
async fn healthz_returns_ok(pool: PgPool) {
    let app = router(state(pool).await);
    let res = app
        .oneshot(Request::builder().uri("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

- [ ] **Step 4: Run it to verify it fails**

Needs a Postgres for `sqlx::test`. Locally:

```bash
docker run -d --name feedback-test-pg -e POSTGRES_PASSWORD=test -e POSTGRES_DB=feedback_test -p 5433:5432 postgres:16-alpine
export DATABASE_URL=postgres://postgres:test@localhost:5433/feedback_test
cd server/feedback && cargo test
```

Expected: compile error — `gw2bo_feedback::app` does not exist.

- [ ] **Step 5: Implement config, error, app, main, and a placeholder migrations dir**

`server/feedback/src/config.rs`:

```rust
use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub admin_token: String,
    pub ip_salt: String,
    pub min_addon_version: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        fn need(k: &str) -> Result<String, String> {
            std::env::var(k).map_err(|_| format!("missing env {k}"))
        }
        Ok(Self {
            database_url: need("DATABASE_URL")?,
            bind_addr: std::env::var("BIND_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:8080".into())
                .parse()
                .map_err(|e| format!("BIND_ADDR: {e}"))?,
            admin_token: need("FEEDBACK_ADMIN_TOKEN")?,
            ip_salt: need("FEEDBACK_IP_SALT")?,
            min_addon_version: std::env::var("MIN_ADDON_VERSION").unwrap_or_else(|_| "1.6.0".into()),
        })
    }
}
```

`server/feedback/src/error.rs`:

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    NotFound,
    PayloadTooLarge,
    RateLimited { retry_after_secs: u64 },
    UpgradeRequired,
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error, reason, retry) = match self {
            ApiError::BadRequest(r) => (StatusCode::BAD_REQUEST, "bad_request", r, None),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", String::new(), None),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found", String::new(), None),
            ApiError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large", "body over 16 KB".into(), None),
            ApiError::RateLimited { retry_after_secs } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited", String::new(), Some(retry_after_secs)),
            ApiError::UpgradeRequired => (StatusCode::UPGRADE_REQUIRED, "addon_too_old", "update the addon".into(), None),
            ApiError::Internal(r) => {
                tracing::error!("internal: {r}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", String::new(), None)
            }
        };
        let mut res = (status, Json(json!({ "error": error, "reason": reason }))).into_response();
        if let Some(secs) = retry {
            res.headers_mut().insert("retry-after", secs.to_string().parse().unwrap());
        }
        res
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}
```

`server/feedback/src/app.rs`:

```rust
use crate::config::Config;
use axum::routing::get;
use axum::Router;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
}

impl AppState {
    pub async fn new(pool: PgPool, config: Arc<Config>) -> Self {
        Self { pool, config }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state)
}
```

`server/feedback/src/lib.rs`:

```rust
pub mod app;
pub mod config;
pub mod error;
```

`server/feedback/src/main.rs`:

```rust
use gw2bo_feedback::app::{router, AppState};
use gw2bo_feedback::config::Config;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = Arc::new(Config::from_env().expect("config"));
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&config.database_url)
        .await
        .expect("database");
    sqlx::migrate!("./migrations").run(&pool).await.expect("migrations");
    let state = AppState::new(pool, config.clone()).await;
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await.expect("bind");
    tracing::info!("feedback listening on {}", config.bind_addr);
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async { tokio::signal::ctrl_c().await.ok(); })
        .await
        .expect("serve");
}
```

Create an empty `server/feedback/migrations/` directory with a `.gitkeep` so `migrate!` compiles (Task 2 adds the real migration).

- [ ] **Step 6: Run the test to verify it passes**

Run: `cd server/feedback && cargo test`
Expected: `healthz_returns_ok ... ok`

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml server/feedback
git commit -m "feat(feedback): scaffold standalone axum server with healthz"
```

---

### Task 2: Schema migration

**Files:**
- Create: `server/feedback/migrations/0001_init.sql`
- Modify: `server/feedback/tests/api.rs`
- Delete: `server/feedback/migrations/.gitkeep`

**Interfaces:**
- Produces: tables `reports` and `taxonomy` exactly as in spec §6 (plus `report_id`, `closing_note`, `unvalidated` from §6a).

- [ ] **Step 1: Write the failing test**

Append to `tests/api.rs`:

```rust
#[sqlx::test(migrations = "./migrations")]
async fn schema_has_reports_and_taxonomy(pool: PgPool) {
    let cols: Vec<(String,)> = sqlx::query_as(
        "select column_name::text from information_schema.columns where table_name = 'reports' order by ordinal_position",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let names: Vec<&str> = cols.iter().map(|c| c.0.as_str()).collect();
    for required in ["short_id", "report_id", "client_id", "category", "path", "body", "status", "payload", "ip_hash", "unvalidated", "closing_note"] {
        assert!(names.contains(&required), "missing column {required}");
    }
    let (n,): (i64,) = sqlx::query_as("select count(*) from taxonomy").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test schema_has_reports_and_taxonomy`
Expected: FAIL — relation "reports" does not exist.

- [ ] **Step 3: Write the migration**

`server/feedback/migrations/0001_init.sql`:

```sql
CREATE TABLE reports (
  id             BIGSERIAL PRIMARY KEY,
  short_id       TEXT NOT NULL UNIQUE,
  report_id      UUID NOT NULL UNIQUE,
  received_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  client_id      UUID NOT NULL,
  schema_version SMALLINT NOT NULL,
  category       TEXT NOT NULL,
  path           TEXT[] NOT NULL,
  title          TEXT NOT NULL,
  body           TEXT NOT NULL,
  contact        TEXT,
  account        TEXT,
  addon_version  TEXT NOT NULL,
  game_build     BIGINT,
  status         TEXT NOT NULL DEFAULT 'received'
                 CHECK (status IN ('received','read','answered','closed')),
  reply          TEXT,
  replied_at     TIMESTAMPTZ,
  closing_note   TEXT,
  unvalidated    BOOLEAN NOT NULL DEFAULT false,
  payload        JSONB NOT NULL,
  ip_hash        TEXT NOT NULL
);
CREATE INDEX reports_status_idx  ON reports (status, received_at DESC);
CREATE INDEX reports_client_idx  ON reports (client_id, received_at DESC);
CREATE INDEX reports_payload_gin ON reports USING GIN (payload jsonb_path_ops);

CREATE TABLE taxonomy (
  version     INTEGER PRIMARY KEY,
  body        JSONB NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

Delete `migrations/.gitkeep`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: both tests `ok`.

- [ ] **Step 5: Commit**

```bash
git add server/feedback/migrations server/feedback/tests/api.rs
git commit -m "feat(feedback): initial reports and taxonomy schema"
```

---

### Task 3: Taxonomy — seed file, `GET /v1/taxonomy`, id validation

**Files:**
- Create: `data/feedback_taxonomy.json`
- Create: `server/feedback/src/taxonomy.rs`
- Modify: `server/feedback/src/app.rs`, `src/lib.rs`, `src/main.rs`
- Modify: `server/feedback/tests/api.rs`

**Interfaces:**
- Produces: `taxonomy::Taxonomy { version: i32, body: serde_json::Value }`, `Taxonomy::embedded() -> Taxonomy`, `Taxonomy::validate(&self, category: &str, path: &[String]) -> bool` (true when every id is known), `taxonomy::seed_if_empty(pool) -> sqlx::Result<()>`, `taxonomy::load_current(pool) -> sqlx::Result<Taxonomy>`. `AppState` gains `taxonomy: Arc<tokio::sync::RwLock<Taxonomy>>`.

- [ ] **Step 1: Create the taxonomy data file**

`data/feedback_taxonomy.json` (the file the addon will embed later — keep it at repo root `data/` where the other game data lives):

```json
{
  "taxonomy_version": 1,
  "categories": [
    { "id": "bug",         "type": "report", "label": "cat.bug",         "icon": "bug",      "color": "red",
      "steps": ["area_screen", "severity", "describe"] },
    { "id": "wrong_build", "type": "report", "label": "cat.wrong_build", "icon": "broken",   "color": "orange",
      "steps": ["area_screen", "severity", "describe"], "attach_build": true },
    { "id": "wish",        "type": "report", "label": "cat.wish",        "icon": "bulb",     "color": "green",
      "steps": ["area_feature", "describe"] },
    { "id": "question",    "type": "report", "label": "cat.question",    "icon": "question", "color": "blue",
      "steps": ["area_feature", "describe"] },
    { "id": "praise",      "type": "report", "label": "cat.fistbump",    "icon": "fist",     "color": "gold",
      "steps": ["liked", "note_optional"] },
    { "id": "coffee",      "type": "link",   "label": "cat.coffee",      "icon": "kofi",     "color": "gold",
      "url": "https://ko-fi.com/specialplacerob" }
  ],
  "steps": {
    "area_screen":  { "prompt": "step.where",    "choices": ["optimize", "improve", "choya", "saves", "chat_code", "settings", "setup", "data", "other"] },
    "area_feature": { "prompt": "step.about",    "choices": ["optimizer", "choya", "ui", "data", "other"] },
    "severity":     { "prompt": "step.how_bad",  "choices": ["crash", "blocks", "wrong", "cosmetic"] },
    "liked":        { "prompt": "step.liked",    "choices": ["optimizer", "choya", "design", "speed", "everything"] },
    "describe":     { "prompt": "step.describe", "text": { "min": 10, "max": 4000 } },
    "note_optional":{ "prompt": "step.note",     "text": { "min": 0,  "max": 1000 } }
  }
}
```

- [ ] **Step 2: Write the failing tests**

Append to `tests/api.rs`:

```rust
use http_body_util::BodyExt;

async fn json_body(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn taxonomy_is_seeded_and_served(pool: PgPool) {
    let app = router(state(pool.clone()).await);
    let res = app
        .oneshot(Request::builder().uri("/v1/taxonomy").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = json_body(res).await;
    assert_eq!(v["taxonomy_version"], 1);
    assert!(v["categories"].as_array().unwrap().iter().any(|c| c["id"] == "praise"));
    let (n,): (i64,) = sqlx::query_as("select count(*) from taxonomy").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1, "seeded exactly once");
}

#[test]
fn taxonomy_validate_knows_its_ids() {
    use gw2bo_feedback::taxonomy::Taxonomy;
    let t = Taxonomy::embedded();
    assert!(t.validate("bug", &["optimize".into(), "wrong".into()]));
    assert!(t.validate("praise", &["everything".into()]));
    assert!(!t.validate("bug", &["optimize".into(), "nope".into()]));
    assert!(!t.validate("teleport", &[]));
}
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test taxonomy`
Expected: compile error — `gw2bo_feedback::taxonomy` does not exist.

- [ ] **Step 4: Implement**

`server/feedback/src/taxonomy.rs`:

```rust
use serde_json::Value;
use sqlx::PgPool;

const EMBEDDED: &str = include_str!("../../../data/feedback_taxonomy.json");

#[derive(Debug, Clone)]
pub struct Taxonomy {
    pub version: i32,
    pub body: Value,
}

impl Taxonomy {
    pub fn embedded() -> Self {
        let body: Value = serde_json::from_str(EMBEDDED).expect("embedded taxonomy is valid JSON");
        let version = body["taxonomy_version"].as_i64().expect("taxonomy_version") as i32;
        Self { version, body }
    }

    /// True when the category exists and every path element is a choice of
    /// one of that category's steps. Unknown ids are not an error for the
    /// caller — they mark the report `unvalidated`.
    pub fn validate(&self, category: &str, path: &[String]) -> bool {
        let Some(cat) = self.body["categories"]
            .as_array()
            .and_then(|cs| cs.iter().find(|c| c["id"] == category))
        else {
            return false;
        };
        let steps = self.body["steps"].as_object();
        let step_ids: Vec<&str> = cat["steps"].as_array().map(|s| s.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        path.iter().all(|p| {
            step_ids.iter().any(|sid| {
                steps
                    .and_then(|s| s.get(*sid))
                    .and_then(|st| st["choices"].as_array())
                    .map(|ch| ch.iter().any(|c| c == p))
                    .unwrap_or(false)
            })
        })
    }
}

pub async fn seed_if_empty(pool: &PgPool) -> sqlx::Result<()> {
    let t = Taxonomy::embedded();
    sqlx::query("insert into taxonomy (version, body) values ($1, $2) on conflict (version) do nothing")
        .bind(t.version)
        .bind(&t.body)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn load_current(pool: &PgPool) -> sqlx::Result<Taxonomy> {
    let row: (i32, Value) = sqlx::query_as("select version, body from taxonomy order by version desc limit 1")
        .fetch_one(pool)
        .await?;
    Ok(Taxonomy { version: row.0, body: row.1 })
}
```

`src/app.rs` — add the field, seed on construction, add the route:

```rust
use crate::taxonomy::{self, Taxonomy};
use axum::extract::State;
use axum::Json;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub taxonomy: Arc<RwLock<Taxonomy>>,
}

impl AppState {
    pub async fn new(pool: PgPool, config: Arc<Config>) -> Self {
        taxonomy::seed_if_empty(&pool).await.expect("seed taxonomy");
        let current = taxonomy::load_current(&pool).await.expect("load taxonomy");
        Self { pool, config, taxonomy: Arc::new(RwLock::new(current)) }
    }
}

async fn get_taxonomy(State(s): State<AppState>) -> Json<serde_json::Value> {
    Json(s.taxonomy.read().await.body.clone())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/taxonomy", get(get_taxonomy))
        .with_state(state)
}
```

Add `pub mod taxonomy;` to `lib.rs`. `main.rs` needs no change — seeding happens in `AppState::new`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test`
Expected: all `ok`, including `taxonomy_is_seeded_and_served` and `taxonomy_validate_knows_its_ids`.

- [ ] **Step 6: Commit**

```bash
git add data/feedback_taxonomy.json server/feedback
git commit -m "feat(feedback): taxonomy seed, GET /v1/taxonomy, id validation"
```

---

### Task 4: `POST /v1/reports` — validate, idempotent insert, short id, ip hash

**Files:**
- Create: `server/feedback/src/ids.rs`
- Create: `server/feedback/src/reports.rs`
- Modify: `server/feedback/src/app.rs`, `src/lib.rs`
- Modify: `server/feedback/tests/api.rs`

**Interfaces:**
- Consumes: `Taxonomy::validate`, `ApiError`, `AppState`.
- Produces: `ids::short_id() -> String` (8 chars, Crockford base32), `ids::ip_hash(ip: &str, salt: &str, day: chrono::NaiveDate) -> String`, `reports::NewReport` (request body struct), `reports::create(state, headers, ip, Json<NewReport>) -> Result<(StatusCode, Json<Created>), ApiError>`, `reports::Created { id: String, status: String }`.
- Header contract: `X-Addon-Version: <semver>` required; below `MIN_ADDON_VERSION` → 426. Client IP: first value of `X-Forwarded-For` if present, else the socket address.

- [ ] **Step 1: Write the failing tests**

Append to `tests/api.rs`:

```rust
fn report_json(report_id: &str) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1,
        "report_id": report_id,
        "client_id": "11111111-1111-4111-8111-111111111111",
        "category": "bug",
        "path": ["optimize", "wrong"],
        "title": "Optimize picks Trident on land",
        "body": "Expected a land weapon, got Trident on a Ranger in WvW roam.",
        "contact": null,
        "account": null,
        "context": { "addon_version": "1.6.0", "game_build": 174122, "locale": "en",
                     "mode": "WvW", "scale": "Roam", "role": "Damage",
                     "profession": "Ranger", "elite": "Untamed", "llm_provider": "gemini" },
        "build_snapshot": null
    })
}

fn post_report(body: serde_json::Value, ip: &str, addon_version: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/reports")
        .header("content-type", "application/json")
        .header("x-forwarded-for", ip)
        .header("x-addon-version", addon_version)
        .body(Body::from(body.to_string()))
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn post_report_creates_row_and_returns_short_id(pool: PgPool) {
    let app = router(state(pool.clone()).await);
    let res = app.oneshot(post_report(report_json("22222222-2222-4222-8222-222222222222"), "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let v = json_body(res).await;
    assert_eq!(v["status"], "received");
    assert_eq!(v["id"].as_str().unwrap().len(), 8);
    let (cat, unval, ip_hash): (String, bool, String) =
        sqlx::query_as("select category, unvalidated, ip_hash from reports").fetch_one(&pool).await.unwrap();
    assert_eq!(cat, "bug");
    assert!(!unval);
    assert_ne!(ip_hash, "203.0.113.5", "raw ip must never be stored");
}

#[sqlx::test(migrations = "./migrations")]
async fn post_same_report_id_twice_is_one_row_same_short_id(pool: PgPool) {
    let st = state(pool.clone()).await;
    let a = json_body(router(st.clone()).oneshot(post_report(report_json("33333333-3333-4333-8333-333333333333"), "203.0.113.5", "1.6.0")).await.unwrap()).await;
    let b = json_body(router(st).oneshot(post_report(report_json("33333333-3333-4333-8333-333333333333"), "203.0.113.5", "1.6.0")).await.unwrap()).await;
    assert_eq!(a["id"], b["id"]);
    let (n,): (i64,) = sqlx::query_as("select count(*) from reports").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_unknown_category_is_stored_unvalidated(pool: PgPool) {
    let mut body = report_json("44444444-4444-4444-8444-444444444444");
    body["category"] = "teleport".into();
    let res = router(state(pool.clone()).await).oneshot(post_report(body, "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let (unval,): (bool,) = sqlx::query_as("select unvalidated from reports").fetch_one(&pool).await.unwrap();
    assert!(unval);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_rejects_too_long_body_and_bad_shape(pool: PgPool) {
    let st = state(pool).await;
    let mut long = report_json("55555555-5555-4555-8555-555555555555");
    long["body"] = "x".repeat(4001).into();
    let res = router(st.clone()).oneshot(post_report(long, "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let huge = serde_json::json!({ "pad": "y".repeat(20_000) });
    let res = router(st.clone()).oneshot(post_report(huge, "203.0.113.5", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let res = router(st).oneshot(post_report(report_json("66666666-6666-4666-8666-666666666666"), "203.0.113.5", "1.5.3")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UPGRADE_REQUIRED);
}

#[test]
fn ids_are_shaped_right() {
    use gw2bo_feedback::ids::{ip_hash, short_id};
    let s = short_id();
    assert_eq!(s.len(), 8);
    assert!(s.chars().all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)));
    let d = chrono::NaiveDate::from_ymd_opt(2026, 8, 24).unwrap();
    assert_eq!(ip_hash("1.2.3.4", "salt", d), ip_hash("1.2.3.4", "salt", d));
    assert_ne!(ip_hash("1.2.3.4", "salt", d), ip_hash("1.2.3.4", "salt", d.succ_opt().unwrap()));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test post_ ids_`
Expected: compile errors — `ids` and route missing.

- [ ] **Step 3: Implement `ids.rs`**

```rust
use chrono::NaiveDate;
use rand::RngCore;
use sha2::{Digest, Sha256};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 8 Crockford-base32 chars from 40 random bits. Shown to the player as `#A3F9K2QD`.
pub fn short_id() -> String {
    let mut bytes = [0u8; 5];
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut acc: u64 = 0;
    for b in bytes {
        acc = (acc << 8) | b as u64;
    }
    (0..8)
        .rev()
        .map(|i| ALPHABET[((acc >> (i * 5)) & 31) as usize] as char)
        .collect()
}

/// sha256(ip || salt || day). Rotates daily so it cannot be joined across days.
pub fn ip_hash(ip: &str, salt: &str, day: NaiveDate) -> String {
    let mut h = Sha256::new();
    h.update(ip.as_bytes());
    h.update(salt.as_bytes());
    h.update(day.format("%Y-%m-%d").to_string().as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 4: Implement `reports.rs`**

```rust
use crate::app::AppState;
use crate::error::ApiError;
use crate::ids::{ip_hash, short_id};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use uuid::Uuid;

pub const MAX_BODY_CHARS: usize = 4000;
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_SNAPSHOT_BYTES: usize = 6 * 1024;

#[derive(Debug, Deserialize)]
pub struct NewReport {
    pub schema_version: i16,
    pub report_id: Uuid,
    pub client_id: Uuid,
    pub category: String,
    pub path: Vec<String>,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub account: Option<String>,
    pub context: Value,
    #[serde(default)]
    pub build_snapshot: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Created {
    pub id: String,
    pub status: String,
}

pub fn client_ip(headers: &HeaderMap, addr: Option<SocketAddr>) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| addr.map(|a| a.ip().to_string()))
        .unwrap_or_else(|| "0.0.0.0".into())
}

fn version_at_least(have: &str, min: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> { s.split('.').filter_map(|p| p.parse().ok()).collect() };
    parse(have) >= parse(min)
}

pub fn check_addon_version(headers: &HeaderMap, min: &str) -> Result<(), ApiError> {
    let have = headers
        .get("x-addon-version")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::BadRequest("missing X-Addon-Version".into()))?;
    if version_at_least(have, min) { Ok(()) } else { Err(ApiError::UpgradeRequired) }
}

pub async fn create(
    State(s): State<AppState>,
    headers: HeaderMap,
    addr: Option<ConnectInfo<SocketAddr>>,
    Json(req): Json<NewReport>,
) -> Result<(StatusCode, Json<Created>), ApiError> {
    check_addon_version(&headers, &s.config.min_addon_version)?;
    if req.body.chars().count() > MAX_BODY_CHARS {
        return Err(ApiError::BadRequest(format!("body over {MAX_BODY_CHARS} characters")));
    }
    if req.title.chars().count() > MAX_TITLE_CHARS || req.title.trim().is_empty() {
        return Err(ApiError::BadRequest(format!("title must be 1..{MAX_TITLE_CHARS} characters")));
    }
    if let Some(snap) = &req.build_snapshot {
        if snap.to_string().len() > MAX_SNAPSHOT_BYTES {
            return Err(ApiError::BadRequest(format!("build_snapshot over {MAX_SNAPSHOT_BYTES} bytes")));
        }
    }
    let unvalidated = !s.taxonomy.read().await.validate(&req.category, &req.path);
    let ip = client_ip(&headers, addr.map(|c| c.0));
    let hash = ip_hash(&ip, &s.config.ip_salt, chrono::Utc::now().date_naive());
    let addon_version = req.context["addon_version"].as_str().unwrap_or("unknown").to_string();
    let game_build = req.context["game_build"].as_i64();
    let payload = serde_json::to_value(&RawEcho::from(&req)).map_err(|e| ApiError::Internal(e.to_string()))?;

    // Idempotent: a resend with the same report_id returns the original row.
    let row: (String, String) = sqlx::query_as(
        r#"
        insert into reports
          (short_id, report_id, client_id, schema_version, category, path, title, body,
           contact, account, addon_version, game_build, unvalidated, payload, ip_hash)
        values ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
        on conflict (report_id) do update set report_id = excluded.report_id
        returning short_id, status
        "#,
    )
    .bind(short_id())
    .bind(req.report_id)
    .bind(req.client_id)
    .bind(req.schema_version)
    .bind(&req.category)
    .bind(&req.path)
    .bind(&req.title)
    .bind(&req.body)
    .bind(&req.contact)
    .bind(&req.account)
    .bind(&addon_version)
    .bind(game_build)
    .bind(unvalidated)
    .bind(&payload)
    .bind(&hash)
    .fetch_one(&s.pool)
    .await?;

    Ok((StatusCode::CREATED, Json(Created { id: row.0, status: row.1 })))
}

/// The request as received, re-serialised for the JSONB `payload` column.
#[derive(Serialize)]
struct RawEcho<'a> {
    schema_version: i16,
    report_id: Uuid,
    client_id: Uuid,
    category: &'a str,
    path: &'a [String],
    title: &'a str,
    body: &'a str,
    contact: &'a Option<String>,
    account: &'a Option<String>,
    context: &'a Value,
    build_snapshot: &'a Option<Value>,
}

impl<'a> From<&'a NewReport> for RawEcho<'a> {
    fn from(r: &'a NewReport) -> Self {
        Self {
            schema_version: r.schema_version, report_id: r.report_id, client_id: r.client_id,
            category: &r.category, path: &r.path, title: &r.title, body: &r.body,
            contact: &r.contact, account: &r.account, context: &r.context, build_snapshot: &r.build_snapshot,
        }
    }
}
```

Note on `on conflict ... do update set report_id = excluded.report_id`: a no-op update so `returning` yields the existing row (plain `do nothing` returns nothing).

- [ ] **Step 5: Wire the route with the 16 KB cap**

In `app.rs`:

```rust
use crate::reports;
use axum::routing::post;
use tower_http::limit::RequestBodyLimitLayer;

pub const MAX_REQUEST_BYTES: usize = 16 * 1024;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/v1/taxonomy", get(get_taxonomy))
        .route("/v1/reports", post(reports::create))
        .layer(RequestBodyLimitLayer::new(MAX_REQUEST_BYTES))
        .with_state(state)
}
```

axum turns an over-limit body into `413` on its own; the test asserts that status. Map axum's JSON rejection to `400` by keeping the default `Json` extractor (its rejection is already 400/422 — the `post_rejects_too_long_body_and_bad_shape` test only asserts the 4001-char case, which is our own 400).

Add `pub mod ids; pub mod reports;` to `lib.rs`. In `main.rs`, change `axum::serve(listener, router(state))` to `axum::serve(listener, router(state).into_make_service_with_connect_info::<std::net::SocketAddr>())` so `ConnectInfo` is available when no proxy header is present.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test`
Expected: all `ok`.

- [ ] **Step 7: Commit**

```bash
git add server/feedback
git commit -m "feat(feedback): POST /v1/reports with idempotent insert, short ids, ip hashing"
```

---

### Task 5: Rate limiting — 10/min per ip_hash, 50/day per client_id

**Files:**
- Create: `server/feedback/src/ratelimit.rs`
- Modify: `server/feedback/src/app.rs`, `src/reports.rs`, `src/lib.rs`
- Modify: `server/feedback/tests/api.rs`

**Interfaces:**
- Produces: `ratelimit::RateLimiter::new()`, `RateLimiter::check(&self, key: &str, limit: usize, window: Duration) -> Result<(), u64 /*retry secs*/>`. `AppState` gains `limiter: Arc<RateLimiter>`.

- [ ] **Step 1: Write the failing tests**

```rust
#[sqlx::test(migrations = "./migrations")]
async fn eleventh_post_in_a_minute_from_one_ip_is_429(pool: PgPool) {
    let st = state(pool).await;
    for i in 0..10 {
        let id = format!("77777777-7777-4777-8777-7777777777{:02}", i);
        let res = router(st.clone()).oneshot(post_report(report_json(&id), "198.51.100.9", "1.6.0")).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED, "request {i}");
    }
    let res = router(st).oneshot(post_report(report_json("77777777-7777-4777-8777-777777777799"), "198.51.100.9", "1.6.0")).await.unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry: u64 = res.headers()["retry-after"].to_str().unwrap().parse().unwrap();
    assert!(retry >= 1 && retry <= 60);
}

#[test]
fn limiter_sliding_window() {
    use gw2bo_feedback::ratelimit::RateLimiter;
    use std::time::Duration;
    let l = RateLimiter::new();
    for _ in 0..3 { assert!(l.check("k", 3, Duration::from_secs(60)).is_ok()); }
    let err = l.check("k", 3, Duration::from_secs(60)).unwrap_err();
    assert!(err >= 1);
    assert!(l.check("other", 3, Duration::from_secs(60)).is_ok());
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test limiter eleventh`
Expected: compile error — `ratelimit` missing.

- [ ] **Step 3: Implement**

`src/ratelimit.rs`:

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// In-memory sliding window. State is per process and lost on restart, which
/// is acceptable: the limits exist to stop floods, not to meter billing.
/// ponytail: single Mutex<HashMap>; swap for a sharded map if p99 ever matters.
#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new() -> Self { Self::default() }

    /// Ok(()) if under `limit` within `window`; Err(seconds until the oldest hit expires) otherwise.
    pub fn check(&self, key: &str, limit: usize, window: Duration) -> Result<(), u64> {
        let now = Instant::now();
        let mut map = self.hits.lock().unwrap();
        let v = map.entry(key.to_string()).or_default();
        v.retain(|t| now.duration_since(*t) < window);
        if v.len() >= limit {
            let oldest = v[0];
            let retry = window.saturating_sub(now.duration_since(oldest)).as_secs().max(1);
            return Err(retry);
        }
        v.push(now);
        // Opportunistic cleanup so the map does not grow without bound.
        if map.len() > 10_000 {
            map.retain(|_, hits| hits.iter().any(|t| now.duration_since(*t) < window));
        }
        Ok(())
    }
}
```

In `app.rs`: add `pub limiter: Arc<RateLimiter>` to `AppState`, initialise with `Arc::new(RateLimiter::new())` in `new`.

In `reports::create`, after computing `hash` and before the insert:

```rust
use std::time::Duration;
s.limiter.check(&format!("ip:{hash}"), 10, Duration::from_secs(60))
    .map_err(|retry_after_secs| ApiError::RateLimited { retry_after_secs })?;
s.limiter.check(&format!("client:{}", req.client_id), 50, Duration::from_secs(24 * 3600))
    .map_err(|retry_after_secs| ApiError::RateLimited { retry_after_secs })?;
```

Add `pub mod ratelimit;` to `lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: all `ok`.

- [ ] **Step 5: Commit**

```bash
git add server/feedback
git commit -m "feat(feedback): per-ip and per-client rate limits with Retry-After"
```

---

### Task 6: `GET /v1/reports/status?ids=…`

**Files:**
- Modify: `server/feedback/src/reports.rs`, `src/app.rs`
- Modify: `server/feedback/tests/api.rs`

**Interfaces:**
- Produces: `reports::status(state, headers, addr, Query<StatusQuery>) -> Json<Vec<StatusRow>>`, `StatusQuery { ids: String, client_id: Uuid }`, `StatusRow { id, status, reply: Option<String>, replied_at: Option<DateTime<Utc>>, closing_note: Option<String> }`. A row is returned only when its `short_id` is in `ids` **and** its `client_id` equals the query's; anything else is simply absent (the addon maps absent-after-200 → `unknown`). Max 50 ids per call. Rate-limited per `ip_hash` with the same 10/min as POST (spec §14 ruling 2).

- [ ] **Step 1: Write the failing test**

```rust
const CLIENT: &str = "11111111-1111-4111-8111-111111111111"; // matches report_json()

fn status_req(ids: &str, client_id: &str, ip: &str) -> Request<Body> {
    Request::builder()
        .uri(format!("/v1/reports/status?ids={ids}&client_id={client_id}"))
        .header("x-forwarded-for", ip)
        .body(Body::empty())
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn status_returns_only_requested_ids_owned_by_client(pool: PgPool) {
    let st = state(pool.clone()).await;
    let a = json_body(router(st.clone()).oneshot(post_report(report_json("88888888-8888-4888-8888-888888888881"), "203.0.113.5", "1.6.0")).await.unwrap()).await;
    let _b = json_body(router(st.clone()).oneshot(post_report(report_json("88888888-8888-4888-8888-888888888882"), "203.0.113.5", "1.6.0")).await.unwrap()).await;
    let id = a["id"].as_str().unwrap();

    let res = router(st.clone()).oneshot(status_req(&format!("{id},NOPE1234"), CLIENT, "203.0.113.5")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = json_body(res).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1, "only the requested, existing id — never the sibling row");
    assert_eq!(arr[0]["id"], a["id"]);
    assert_eq!(arr[0]["status"], "received");
    assert!(arr[0]["reply"].is_null());

    // Same id, different client: nothing. Ownership is short_id AND client_id.
    let other = json_body(router(st.clone()).oneshot(status_req(id, "22222222-2222-4222-8222-222222222222", "203.0.113.5")).await.unwrap()).await;
    assert_eq!(other.as_array().unwrap().len(), 0);

    // Missing client_id is a 400, not an open door.
    let res = router(st.clone()).oneshot(Request::builder().uri(format!("/v1/reports/status?ids={id}")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Rate-limited per ip like POST: a fresh ip gets 10 GETs, then 429.
    for i in 0..10 {
        assert_eq!(router(st.clone()).oneshot(status_req(id, CLIENT, "203.0.113.77")).await.unwrap().status(), StatusCode::OK, "get {i}");
    }
    assert_eq!(router(st).oneshot(status_req(id, CLIENT, "203.0.113.77")).await.unwrap().status(), StatusCode::TOO_MANY_REQUESTS);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test status_returns`

(Task 6 depends on Task 5's `limiter`; POST and status GET from one ip draw from the same 10/min bucket. A missing `client_id` is rejected by axum's `Query` extractor with 400.)
Expected: FAIL — 404 (route missing).

- [ ] **Step 3: Implement**

In `reports.rs`:

```rust
use axum::extract::Query;
use chrono::{DateTime, Utc};

#[derive(Deserialize)]
pub struct StatusQuery { pub ids: String, pub client_id: Uuid }

#[derive(Serialize, sqlx::FromRow)]
pub struct StatusRow {
    #[sqlx(rename = "short_id")]
    pub id: String,
    pub status: String,
    pub reply: Option<String>,
    pub replied_at: Option<DateTime<Utc>>,
    pub closing_note: Option<String>,
}

pub async fn status(
    State(s): State<AppState>,
    headers: HeaderMap,
    addr: Option<ConnectInfo<SocketAddr>>,
    Query(q): Query<StatusQuery>,
) -> Result<Json<Vec<StatusRow>>, ApiError> {
    let ip = client_ip(&headers, addr.map(|c| c.0));
    let hash = ip_hash(&ip, &s.config.ip_salt, chrono::Utc::now().date_naive());
    s.limiter.check(&format!("ip:{hash}"), 10, std::time::Duration::from_secs(60))
        .map_err(|retry_after_secs| ApiError::RateLimited { retry_after_secs })?;
    let ids: Vec<String> = q.ids.split(',').map(|x| x.trim().to_uppercase()).filter(|x| !x.is_empty()).take(50).collect();
    if ids.is_empty() {
        return Err(ApiError::BadRequest("ids required".into()));
    }
    let rows = sqlx::query_as::<_, StatusRow>(
        "select short_id, status, reply, replied_at, closing_note from reports where short_id = any($1) and client_id = $2",
    )
    .bind(&ids)
    .bind(q.client_id)
    .fetch_all(&s.pool)
    .await?;
    Ok(Json(rows))
}
```

Route in `app.rs`: `.route("/v1/reports/status", get(reports::status))`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: all `ok`.

- [ ] **Step 5: Commit**

```bash
git add server/feedback
git commit -m "feat(feedback): GET /v1/reports/status for the addon's local rows"
```

---

### Task 7: Admin API — bearer token, list, reply, status, taxonomy replace

**Files:**
- Create: `server/feedback/src/admin.rs`
- Modify: `server/feedback/src/app.rs`, `src/lib.rs`
- Modify: `server/feedback/tests/api.rs`

**Interfaces:**
- Produces, all under `/v1/admin`, all requiring `Authorization: Bearer <FEEDBACK_ADMIN_TOKEN>`:
  - `GET  /reports?status=received&limit=50` → `Vec<AdminRow>`; fetching flips every returned `received` row to `read`.
  - `GET  /reports/{id}` → `AdminRow` (full payload), same auto-`read`.
  - `POST /reports/{id}/reply` `{ "reply": String, "status": "answered" | "closed" }` → 200 `StatusRow`.
  - `POST /reports/{id}/status` `{ "status": String, "closing_note": Option<String> }` → 200 `StatusRow`.
  - `PUT  /taxonomy` (JSON body = full taxonomy with a higher `taxonomy_version`) → 200 `{version}`; swaps the in-memory copy.

- [ ] **Step 1: Write the failing tests**

```rust
fn admin(method: &str, uri: &str, body: Option<serde_json::Value>, token: &str) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri).header("authorization", format!("Bearer {token}"));
    if body.is_some() { b = b.header("content-type", "application/json"); }
    b.body(body.map(|v| Body::from(v.to_string())).unwrap_or(Body::empty())).unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_requires_token(pool: PgPool) {
    let res = router(state(pool).await).oneshot(admin("GET", "/v1/admin/reports", None, "wrong")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_list_marks_read_and_reply_marks_answered(pool: PgPool) {
    let st = state(pool).await;
    let created = json_body(router(st.clone()).oneshot(post_report(report_json("99999999-9999-4999-8999-999999999991"), "203.0.113.5", "1.6.0")).await.unwrap()).await;
    let id = created["id"].as_str().unwrap().to_string();

    let list = json_body(router(st.clone()).oneshot(admin("GET", "/v1/admin/reports?status=received", None, "test-admin-token")).await.unwrap()).await;
    assert_eq!(list.as_array().unwrap().len(), 1);

    let st_after = json_body(router(st.clone()).oneshot(status_req(&id, CLIENT, "203.0.113.5")).await.unwrap()).await;
    assert_eq!(st_after[0]["status"], "read", "listing through admin marks it read");

    let res = router(st.clone()).oneshot(admin("POST", &format!("/v1/admin/reports/{id}/reply"),
        Some(serde_json::json!({ "reply": "Fixed in 1.6.1, thanks!", "status": "answered" })), "test-admin-token")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let final_status = json_body(router(st).oneshot(status_req(&id, CLIENT, "203.0.113.5")).await.unwrap()).await;
    assert_eq!(final_status[0]["status"], "answered");
    assert_eq!(final_status[0]["reply"], "Fixed in 1.6.1, thanks!");
    assert!(final_status[0]["replied_at"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_can_replace_taxonomy_and_it_is_served(pool: PgPool) {
    let st = state(pool).await;
    let mut t = gw2bo_feedback::taxonomy::Taxonomy::embedded().body;
    t["taxonomy_version"] = 2.into();
    t["categories"].as_array_mut().unwrap().push(serde_json::json!({
        "id": "translation", "type": "report", "label": "cat.translation", "icon": "globe", "color": "blue", "steps": ["describe"]
    }));
    let res = router(st.clone()).oneshot(admin("PUT", "/v1/admin/taxonomy", Some(t), "test-admin-token")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let served = json_body(router(st).oneshot(Request::builder().uri("/v1/taxonomy").body(Body::empty()).unwrap()).await.unwrap()).await;
    assert_eq!(served["taxonomy_version"], 2);
    assert!(served["categories"].as_array().unwrap().iter().any(|c| c["id"] == "translation"));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test admin_`
Expected: FAIL — 404s (routes missing).

- [ ] **Step 3: Implement**

`src/admin.rs`:

```rust
use crate::app::AppState;
use crate::error::ApiError;
use crate::reports::StatusRow;
use crate::taxonomy::Taxonomy;
use axum::extract::{Path, Query, Request, State};
use axum::http::HeaderMap;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

async fn require_token(State(s): State<AppState>, headers: HeaderMap, req: Request, next: Next) -> Result<Response, ApiError> {
    let ok = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| constant_time_eq(t.as_bytes(), s.config.admin_token.as_bytes()))
        .unwrap_or(false);
    if ok { Ok(next.run(req).await) } else { Err(ApiError::Unauthorized) }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AdminRow {
    #[sqlx(rename = "short_id")]
    pub id: String,
    pub received_at: DateTime<Utc>,
    pub category: String,
    pub path: Vec<String>,
    pub title: String,
    pub body: String,
    pub contact: Option<String>,
    pub account: Option<String>,
    pub addon_version: String,
    pub game_build: Option<i64>,
    pub status: String,
    pub reply: Option<String>,
    pub unvalidated: bool,
    pub payload: Value,
}

#[derive(Deserialize)]
pub struct ListQuery { pub status: Option<String>, pub limit: Option<i64> }

const ADMIN_COLS: &str = "short_id, received_at, category, path, title, body, contact, account, addon_version, game_build, status, reply, unvalidated, payload";

async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> Result<Json<Vec<AdminRow>>, ApiError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let rows = match q.status {
        Some(st) => sqlx::query_as::<_, AdminRow>(&format!("select {ADMIN_COLS} from reports where status = $1 order by received_at desc limit $2"))
            .bind(st).bind(limit).fetch_all(&s.pool).await?,
        None => sqlx::query_as::<_, AdminRow>(&format!("select {ADMIN_COLS} from reports order by received_at desc limit $1"))
            .bind(limit).fetch_all(&s.pool).await?,
    };
    mark_read(&s, rows.iter().map(|r| r.id.clone()).collect()).await?;
    Ok(Json(rows))
}

async fn get_one(State(s): State<AppState>, Path(id): Path<String>) -> Result<Json<AdminRow>, ApiError> {
    let row = sqlx::query_as::<_, AdminRow>(&format!("select {ADMIN_COLS} from reports where short_id = $1"))
        .bind(id.to_uppercase()).fetch_optional(&s.pool).await?.ok_or(ApiError::NotFound)?;
    mark_read(&s, vec![row.id.clone()]).await?;
    Ok(Json(row))
}

async fn mark_read(s: &AppState, ids: Vec<String>) -> Result<(), ApiError> {
    if ids.is_empty() { return Ok(()); }
    sqlx::query("update reports set status = 'read' where short_id = any($1) and status = 'received'")
        .bind(&ids).execute(&s.pool).await?;
    Ok(())
}

#[derive(Deserialize)]
pub struct ReplyBody { pub reply: String, pub status: String }

async fn reply(State(s): State<AppState>, Path(id): Path<String>, Json(b): Json<ReplyBody>) -> Result<Json<StatusRow>, ApiError> {
    if !matches!(b.status.as_str(), "answered" | "closed") {
        return Err(ApiError::BadRequest("status must be answered or closed".into()));
    }
    let row = sqlx::query_as::<_, StatusRow>(
        "update reports set reply = $2, replied_at = now(), status = $3 where short_id = $1
         returning short_id, status, reply, replied_at, closing_note",
    )
    .bind(id.to_uppercase()).bind(&b.reply).bind(&b.status)
    .fetch_optional(&s.pool).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

#[derive(Deserialize)]
pub struct StatusBody { pub status: String, #[serde(default)] pub closing_note: Option<String> }

async fn set_status(State(s): State<AppState>, Path(id): Path<String>, Json(b): Json<StatusBody>) -> Result<Json<StatusRow>, ApiError> {
    if !matches!(b.status.as_str(), "received" | "read" | "answered" | "closed") {
        return Err(ApiError::BadRequest("unknown status".into()));
    }
    let row = sqlx::query_as::<_, StatusRow>(
        "update reports set status = $2, closing_note = coalesce($3, closing_note) where short_id = $1
         returning short_id, status, reply, replied_at, closing_note",
    )
    .bind(id.to_uppercase()).bind(&b.status).bind(&b.closing_note)
    .fetch_optional(&s.pool).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(row))
}

async fn put_taxonomy(State(s): State<AppState>, Json(body): Json<Value>) -> Result<Json<Value>, ApiError> {
    let version = body["taxonomy_version"].as_i64().ok_or_else(|| ApiError::BadRequest("taxonomy_version required".into()))? as i32;
    let current = s.taxonomy.read().await.version;
    if version <= current {
        return Err(ApiError::BadRequest(format!("taxonomy_version must be > {current}")));
    }
    if !body["categories"].is_array() || !body["steps"].is_object() {
        return Err(ApiError::BadRequest("categories[] and steps{} required".into()));
    }
    sqlx::query("insert into taxonomy (version, body) values ($1, $2)").bind(version).bind(&body).execute(&s.pool).await?;
    *s.taxonomy.write().await = Taxonomy { version, body };
    Ok(Json(serde_json::json!({ "version": version })))
}

pub fn routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/reports", get(list))
        .route("/reports/{id}", get(get_one))
        .route("/reports/{id}/reply", post(reply))
        .route("/reports/{id}/status", post(set_status))
        .route("/taxonomy", put(put_taxonomy))
        .layer(middleware::from_fn_with_state(state, require_token))
}
```

In `app.rs`, nest it: `.nest("/v1/admin", admin::routes(state.clone()))` before `.layer(RequestBodyLimitLayer…)`. Add `pub mod admin;` to `lib.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test`
Expected: all `ok`.

- [ ] **Step 5: Commit**

```bash
git add server/feedback
git commit -m "feat(feedback): token-gated admin API for listing, replying, and taxonomy updates"
```

---

### Task 8: Lint gate and CI for the server

**Files:**
- Create: `.github/workflows/feedback-server.yml`

**Interfaces:**
- Produces: a required check `feedback-server` on PRs touching `server/feedback/**` or `data/feedback_taxonomy.json`.

- [ ] **Step 1: Make clippy clean locally**

Run: `cd server/feedback && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: no output. Fix anything it reports (do not `allow` it away).

- [ ] **Step 2: Add the workflow**

`.github/workflows/feedback-server.yml`:

```yaml
name: feedback-server

on:
  pull_request:
    paths: ["server/feedback/**", "data/feedback_taxonomy.json", ".github/workflows/feedback-server.yml"]
  push:
    branches: [main]
    paths: ["server/feedback/**", "data/feedback_taxonomy.json"]

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:16-alpine
        env:
          POSTGRES_PASSWORD: test
          POSTGRES_DB: feedback_test
        ports: ["5432:5432"]
        options: >-
          --health-cmd "pg_isready -U postgres" --health-interval 5s --health-timeout 5s --health-retries 10
    env:
      DATABASE_URL: postgres://postgres:test@localhost:5432/feedback_test
    defaults:
      run:
        working-directory: server/feedback
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy, rustfmt
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: server/feedback
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/feedback-server.yml server/feedback
git commit -m "ci: test the feedback server against Postgres 16"
```

---

### Task 9: Container, compose, backups, deploy docs

**Files:**
- Create: `server/feedback/Dockerfile`
- Create: `server/feedback/compose.yml`
- Create: `server/feedback/.env.example`
- Create: `server/feedback/deploy/backup.sh`
- Modify: `server/feedback/deploy/README.md`

**Interfaces:**
- Consumes: `TRAEFIK_NETWORK`, `CERT_RESOLVER` from Task 0.
- Produces: an image that starts, migrates, and serves on 8080; compose that puts it behind Traefik at `feedback.robagentic.tech` with Postgres on an internal network only.

- [ ] **Step 1: Dockerfile**

```dockerfile
# syntax=docker/dockerfile:1
FROM rust:1-bookworm AS build
WORKDIR /src
COPY server/feedback/Cargo.toml server/feedback/Cargo.lock* ./server/feedback/
COPY data/feedback_taxonomy.json ./data/feedback_taxonomy.json
# Prime the dependency cache with a stub main so source edits don't rebuild deps.
RUN mkdir -p server/feedback/src && echo 'fn main(){}' > server/feedback/src/main.rs && echo '' > server/feedback/src/lib.rs \
 && cd server/feedback && cargo build --release && rm -rf src
COPY server/feedback ./server/feedback
RUN cd server/feedback && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/server/feedback/target/release/gw2bo-feedback /usr/local/bin/gw2bo-feedback
USER 65534:65534
EXPOSE 8080
ENTRYPOINT ["gw2bo-feedback"]
```

Build context is the **repo root** (the image needs `data/feedback_taxonomy.json`): `docker build -f server/feedback/Dockerfile -t gw2bo-feedback:local .`

- [ ] **Step 2: compose.yml**

```yaml
name: feedback

services:
  feedback:
    image: ghcr.io/special-place-ai-heaven/gw2bo-feedback:latest
    build:
      context: ../..
      dockerfile: server/feedback/Dockerfile
    restart: unless-stopped
    env_file: .env
    environment:
      DATABASE_URL: postgres://feedback:${POSTGRES_PASSWORD}@db:5432/feedback
      BIND_ADDR: 0.0.0.0:8080
      RUST_LOG: info
    depends_on:
      db:
        condition: service_healthy
    networks: [internal, ${TRAEFIK_NETWORK}]
    labels:
      traefik.enable: "true"
      traefik.docker.network: ${TRAEFIK_NETWORK}
      traefik.http.routers.feedback.rule: Host(`feedback.robagentic.tech`)
      traefik.http.routers.feedback.entrypoints: websecure
      traefik.http.routers.feedback.tls.certresolver: ${CERT_RESOLVER}
      traefik.http.services.feedback.loadbalancer.server.port: "8080"

  db:
    image: postgres:16-alpine
    restart: unless-stopped
    environment:
      POSTGRES_USER: feedback
      POSTGRES_DB: feedback
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
    volumes:
      - pgdata:/var/lib/postgresql/data
    networks: [internal]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U feedback -d feedback"]
      interval: 5s
      timeout: 5s
      retries: 10

networks:
  internal:
  ${TRAEFIK_NETWORK}:
    external: true

volumes:
  pgdata:
```

`db` has no `ports:` — it is reachable only from `feedback` on `internal`.

- [ ] **Step 3: .env.example**

```dotenv
# Copy to /docker/feedback/.env on the VPS and fill in. Never commit the real file.
POSTGRES_PASSWORD=<<FILL_IN: openssl rand -hex 24>>
FEEDBACK_ADMIN_TOKEN=<<FILL_IN: openssl rand -hex 32>>
FEEDBACK_IP_SALT=<<FILL_IN: openssl rand -hex 16>>
MIN_ADDON_VERSION=1.6.0
TRAEFIK_NETWORK=<<FILL_IN: from deploy/README.md>>
CERT_RESOLVER=<<FILL_IN: from deploy/README.md>>
```

- [ ] **Step 4: backup.sh**

```bash
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
```

Restore: `docker compose exec -T db pg_restore -U feedback -d feedback --clean --if-exists < backups/feedback-<stamp>.dump`.

- [ ] **Step 5: Extend deploy/README.md**

Append:

```markdown
## Deploy (first time)

On the VPS as the compose user:

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
```

Also create `server/feedback/deploy/sample-report.json` with the same JSON as `report_json(...)` in the tests (a fresh UUID for `report_id`), so the verify step is copy-paste.

- [ ] **Step 6: Build the image locally and smoke it**

```bash
docker build -f server/feedback/Dockerfile -t gw2bo-feedback:local .
docker run --rm --network host -e DATABASE_URL=postgres://postgres:test@localhost:5433/feedback_test \
  -e FEEDBACK_ADMIN_TOKEN=t -e FEEDBACK_IP_SALT=s gw2bo-feedback:local &
sleep 2 && curl -s localhost:8080/healthz && echo && curl -s localhost:8080/v1/taxonomy | head -c 120
```

Expected: `ok` then the taxonomy JSON prefix. Stop the container.

- [ ] **Step 7: Commit**

```bash
git add server/feedback
git commit -m "chore(feedback): Dockerfile, compose with Traefik labels, backups, deploy docs"
```

---

### Task 10: Go live — DNS, deploy, end-to-end verification

**Files:**
- Modify: `server/feedback/deploy/README.md` (record the go-live date and the first short id)

**Interfaces:**
- Produces: `https://feedback.robagentic.tech` answering all public and admin endpoints with TLS.

- [ ] **Step 1: DNS**

Via the Hostinger MCP (`DNS_updateDNSRecordsV1` on `robagentic.tech`): add `A feedback → 72.62.155.186`, TTL 300 — the same shape as the existing `memory` record. Verify: `nslookup feedback.robagentic.tech` resolves to that IP.

- [ ] **Step 2: Publish the image**

Either push `ghcr.io/special-place-ai-heaven/gw2bo-feedback:latest` from the developer machine (`docker login ghcr.io`, `docker build … -t ghcr.io/…:latest .`, `docker push`), or build on the VPS with `docker compose build` (compose.yml has the `build:` block; needs the repo checked out at `/docker/feedback/src`). The VPS build is the simpler first-time path.

- [ ] **Step 3: Deploy**

Follow "Deploy (first time)" in `deploy/README.md`. Generate secrets with the `openssl rand` commands from `.env.example`. Expected in logs: migrations applied, `feedback listening on 0.0.0.0:8080`.

- [ ] **Step 4: Verify TLS and the full loop**

```bash
curl -sI https://feedback.robagentic.tech/healthz | head -1          # HTTP/2 200
curl -s https://feedback.robagentic.tech/v1/taxonomy | jq .taxonomy_version   # 1
ID=$(curl -s -X POST https://feedback.robagentic.tech/v1/reports \
  -H 'content-type: application/json' -H 'x-addon-version: 1.6.0' \
  -d @server/feedback/deploy/sample-report.json | jq -r .id); echo "$ID"
curl -s -H "authorization: Bearer $FEEDBACK_ADMIN_TOKEN" "https://feedback.robagentic.tech/v1/admin/reports/$ID" | jq .status   # "read"
curl -s -X POST -H "authorization: Bearer $FEEDBACK_ADMIN_TOKEN" -H 'content-type: application/json' \
  "https://feedback.robagentic.tech/v1/admin/reports/$ID/reply" -d '{"reply":"Got it. Thanks!","status":"answered"}' | jq .status   # "answered"
curl -s "https://feedback.robagentic.tech/v1/reports/status?ids=$ID&client_id=11111111-1111-4111-8111-111111111111" | jq '.[0].status'   # "answered"
curl -s -o /dev/null -w '%{http_code}\n' https://feedback.robagentic.tech/v1/admin/reports   # 401
```

Expected: exactly the values in the comments. Any deviation is a blocker for the addon plan.

- [ ] **Step 5: Backup cron + first backup**

```bash
ssh ai-vps 'sudo chmod +x /docker/feedback/deploy/backup.sh && sudo /docker/feedback/deploy/backup.sh && (sudo crontab -l 2>/dev/null; echo "15 3 * * * /docker/feedback/deploy/backup.sh >> /docker/feedback/backups/cron.log 2>&1") | sudo crontab -'
```

Expected: `backup ok: /docker/feedback/backups/feedback-<stamp>.dump`.

- [ ] **Step 6: Record and commit**

Append to `deploy/README.md`: `Went live 2026-MM-DD. First report short id: <ID> (test, closed).` Close the test report: `POST /v1/admin/reports/$ID/status {"status":"closed","closing_note":"deploy smoke test"}`.

```bash
git add server/feedback/deploy/README.md
git commit -m "docs(feedback): record go-live and smoke test"
```

Then open the PR for the whole `feat/about-and-feedback` branch (do not merge):

```bash
git push -u origin feat/about-and-feedback
gh pr create --title "feat: feedback server (Postgres + axum) live at feedback.robagentic.tech" \
  --body "Server half of docs/superpowers/specs/2026-08-24-feedback-and-about-design.md. Addon follows in a separate PR."
```

---

## Self-review

**Spec coverage** (server half): §5 taxonomy embedded/served/replaceable → Tasks 3, 7. §6 payload + schema → Tasks 2, 4. §6a idempotency, statuses, `read`-on-fetch, 426, 429 + Retry-After, 413 → Tasks 4, 5, 7. §7 two containers, DB not exposed, sqlx migrations, backups, DNS → Tasks 2, 9, 10. §10 server-side errors → `ApiError` (Task 1) with distinct codes. §11 server tests → each task; live loop → Task 10. §12 rollout step 1 → Task 10. Not in this plan by design: anything under §3, §4, §8, §9 (addon) — next plan.

**Placeholder scan:** the only `<<FILL_IN>>` markers are in `.env.example` (secrets that must not be in the repo) and the two discovered Traefik names in Task 0, which the task itself fills in. No "TBD"/"add validation" language.

**Type consistency:** `StatusRow` (Task 6) is reused verbatim by admin `reply`/`set_status` (Task 7), with the same five columns in every `returning`. `Taxonomy::validate(&str, &[String])` in Task 3 matches the call in Task 4. `AppState::new(pool, Arc<Config>)` is the same signature from Task 1 through Task 7; fields are added, the constructor call in tests does not change. Route params use axum 0.8 `{id}` syntax throughout.
