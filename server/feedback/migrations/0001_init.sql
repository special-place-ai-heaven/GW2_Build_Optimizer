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
