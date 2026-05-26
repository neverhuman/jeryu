-- Migration: 202606010001 — web forge core schema (W-F-05)
--
-- Introduces the SQLite tables that back the JeRyu Web Forge BFF
-- (browser-facing service that talks to internal GitLab and GitHub).
--
-- Source spec sections (canonical):
--   tips/web/JERYU_FULL_WEB_FORGE_ENGINEERING_SPEC_FINAL.md §6.11
--     Baseline table layout (vcs_hosts, web_repositories, …).
--   WEB_WORK_CLAUDE.md §35.1.4
--     web_markdown_cache key includes BOTH renderer_version and
--     sanitizer_version so bumping the ammonia allowlist invalidates
--     cached HTML without bumping the pulldown-cmark parser version.
--   WEB_WORK_CLAUDE.md §35.1.16
--     Dedicated web_action_receipts table (separate from audit_events)
--     with UNIQUE(action_kind, target_id, idempotency_key) enforcing
--     idempotency at the DB layer for the 14-step action algorithm.
--   WEB_WORK_CLAUDE.md §35.1.17
--     provider_etag column on web_repositories for conditional
--     If-None-Match GETs against the upstream host.
--   WEB_WORK_CLAUDE.md §22.2 / §22.3
--     sessions table for opaque cookie-based auth (32-byte hex IDs,
--     rolling 30-day expiry, last_seen_at).
--
-- Idempotency contract:
--   Every CREATE TABLE / CREATE INDEX uses IF NOT EXISTS so this file is
--   safe to re-apply against a database that already has some or all of
--   these objects (consistent with the inline-schema convention used by
--   db/state.rs, which applies its DDL at startup).
--
-- NOTE on discovery:
--   At the time of writing, db/migrations/*.sql files are NOT
--   auto-discovered by the runtime. The live schema is embedded as a
--   Rust string in `db/state.rs` and applied at startup
--   (see db/migrations/0001_inline_schema.sql:1-19 for the rationale).
--   Registering this file with the runtime is part of the W-B-* wiring
--   work, not W-F-05.

-- ─── VCS hosts ─────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS vcs_hosts (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  base_url TEXT NOT NULL,
  display_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- ─── Repositories (web-facing projection) ─────────────────────────────
-- Richer column set than spec §6.11 to support pretty URLs (`full_name`,
-- `slug`), local checkouts (`local_path`), and conditional sync
-- (`provider_etag`, `pushed_at`, `refreshed_at`).
CREATE TABLE IF NOT EXISTS web_repositories (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  provider_repo_id TEXT,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  full_name TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  family TEXT,
  description TEXT,
  visibility TEXT NOT NULL,
  default_branch TEXT NOT NULL,
  local_path TEXT,
  remote_url TEXT,
  clone_https_url TEXT,
  clone_ssh_url TEXT,
  web_url TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  fork INTEGER NOT NULL DEFAULT 0,
  template INTEGER NOT NULL DEFAULT 0,
  topics_json TEXT NOT NULL DEFAULT '[]',
  settings_json TEXT,
  provider_etag TEXT,
  pushed_at TEXT,
  refreshed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(provider, owner, name)
);

-- ─── Repository refs (branches and tags as known to the host) ─────────
CREATE TABLE IF NOT EXISTS web_repo_refs (
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  ref_kind TEXT NOT NULL,
  name TEXT NOT NULL,
  sha TEXT NOT NULL,
  protected INTEGER NOT NULL DEFAULT 0,
  default_ref INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (repo_id, ref_kind, name)
);

-- ─── Repository-level settings + protection rules ─────────────────────
CREATE TABLE IF NOT EXISTS repository_settings_cache (
  repository_id TEXT PRIMARY KEY REFERENCES web_repositories(id),
  settings_json TEXT NOT NULL,
  settings_hash TEXT NOT NULL,
  synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS branch_protection_rules (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES web_repositories(id),
  pattern TEXT NOT NULL,
  rule_json TEXT NOT NULL,
  synced_at TEXT NOT NULL
);

-- ─── Merge / pull requests + review threads ───────────────────────────
CREATE TABLE IF NOT EXISTS web_merge_requests (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL REFERENCES web_repositories(id),
  iid TEXT NOT NULL,
  provider_id TEXT,
  title TEXT NOT NULL,
  body TEXT,
  state TEXT NOT NULL,
  draft INTEGER NOT NULL DEFAULT 0,
  author_login TEXT NOT NULL,
  source_branch TEXT NOT NULL,
  target_branch TEXT NOT NULL,
  head_sha TEXT NOT NULL,
  base_sha TEXT,
  merge_status TEXT,
  passport_hash TEXT,
  web_url TEXT,
  labels_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(repo_id, iid)
);

CREATE TABLE IF NOT EXISTS review_threads (
  id TEXT PRIMARY KEY,
  merge_request_id TEXT NOT NULL REFERENCES web_merge_requests(id),
  host_thread_id TEXT NOT NULL,
  path TEXT,
  line INTEGER,
  resolved INTEGER NOT NULL DEFAULT 0,
  comments_json TEXT NOT NULL DEFAULT '[]',
  synced_at TEXT NOT NULL
);

-- ─── CI status surface ────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS status_checks (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES web_repositories(id),
  ref_sha TEXT NOT NULL,
  name TEXT NOT NULL,
  status TEXT NOT NULL,
  conclusion TEXT,
  details_url TEXT,
  synced_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS workflow_runs (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES web_repositories(id),
  host_run_id TEXT NOT NULL,
  name TEXT,
  ref_name TEXT,
  head_sha TEXT,
  status TEXT,
  conclusion TEXT,
  url TEXT,
  started_at TEXT,
  updated_at TEXT,
  synced_at TEXT NOT NULL
);

-- ─── Rendered markdown cache ──────────────────────────────────────────
-- Per WEB_WORK_CLAUDE.md §35.1.4 the cache key MUST include BOTH the
-- renderer (pulldown-cmark options) version AND the sanitizer (ammonia
-- allowlist) version so a sanitizer-policy bump invalidates cached HTML
-- without bumping the parser version.
CREATE TABLE IF NOT EXISTS web_markdown_cache (
  id TEXT PRIMARY KEY,
  repository_id TEXT NOT NULL REFERENCES web_repositories(id),
  ref_sha TEXT NOT NULL,
  path TEXT NOT NULL,
  blob_sha TEXT NOT NULL,
  renderer_version TEXT NOT NULL,
  sanitizer_version TEXT NOT NULL,
  html TEXT NOT NULL,
  toc_json TEXT NOT NULL DEFAULT '[]',
  links_json TEXT NOT NULL DEFAULT '[]',
  warnings_json TEXT NOT NULL DEFAULT '[]',
  rendered_at TEXT NOT NULL,
  UNIQUE(repository_id, ref_sha, path, blob_sha, renderer_version, sanitizer_version)
);

-- ─── Action receipts (machine-facing execution log) ───────────────────
-- Per WEB_WORK_CLAUDE.md §35.1.16. The UNIQUE constraint enforces
-- idempotency at the DB layer for the 14-step action algorithm: a retry
-- with the same Idempotency-Key on the same (action_kind, target_id)
-- collides and the handler returns the stored receipt instead of
-- re-executing the provider call.
CREATE TABLE IF NOT EXISTS web_action_receipts (
  id TEXT PRIMARY KEY,
  actor_login TEXT NOT NULL,
  action_kind TEXT NOT NULL,
  target_kind TEXT NOT NULL,
  target_id TEXT NOT NULL,
  idempotency_key TEXT,
  expected_state_hash TEXT,
  resulting_state_hash TEXT,
  expected_sha TEXT,
  provider_calls_json TEXT NOT NULL,
  risk_tier TEXT NOT NULL,
  status TEXT NOT NULL,
  error TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(action_kind, target_id, idempotency_key)
);

-- ─── Audit + durable event log ────────────────────────────────────────
CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  actor_kind TEXT NOT NULL,
  actor_id TEXT NOT NULL,
  action_id TEXT NOT NULL,
  target_entity TEXT NOT NULL,
  risk_tier TEXT NOT NULL,
  preview_json TEXT,
  result_json TEXT,
  created_at TEXT NOT NULL
);

-- web_events is the durable, monotonically-ordered event journal that
-- backs the WebSocket bus. `seq` is the canonical cursor clients use to
-- detect gaps and resume after reconnect (see WEB_WORK_CLAUDE.md §35
-- WS event_cursor semantics).
CREATE TABLE IF NOT EXISTS web_events (
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  scope TEXT NOT NULL,
  kind TEXT NOT NULL,
  severity TEXT,
  entity_kind TEXT,
  entity_id TEXT,
  summary TEXT,
  payload_json TEXT NOT NULL,
  actor_login TEXT,
  created_at TEXT NOT NULL
);

-- ─── Sessions (opaque cookie auth, see §22.2) ─────────────────────────
CREATE TABLE IF NOT EXISTS sessions (
  id TEXT PRIMARY KEY,
  actor_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  user_agent TEXT,
  ip TEXT
);

-- ─── Supporting indexes ───────────────────────────────────────────────
CREATE INDEX IF NOT EXISTS idx_web_repositories_family
  ON web_repositories(family);
CREATE INDEX IF NOT EXISTS idx_web_merge_requests_repo_state
  ON web_merge_requests(repo_id, state);
CREATE INDEX IF NOT EXISTS idx_status_checks_sha
  ON status_checks(repository_id, ref_sha);
CREATE INDEX IF NOT EXISTS idx_audit_events_created
  ON audit_events(created_at);
CREATE INDEX IF NOT EXISTS idx_web_events_scope_seq
  ON web_events(scope, seq);
CREATE INDEX IF NOT EXISTS idx_sessions_actor
  ON sessions(actor_id);
