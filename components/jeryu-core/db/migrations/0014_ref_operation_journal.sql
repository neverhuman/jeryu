-- Independent recovery custody, deliberately outside the State save set.
-- Repository UUIDs are immutable historical values: no catalog FK may erase
-- intents, audit, or undelivered effects when a repository is renamed/deleted.
-- timeout-guard: lock_timeout = '5s'; statement_timeout = '60s'
CREATE TABLE IF NOT EXISTS forge_ref_operations (
  id TEXT PRIMARY KEY,
  repo_id TEXT NOT NULL,
  idempotency_key TEXT NOT NULL CHECK (length(trim(idempotency_key)) > 0),
  intent_json TEXT NOT NULL CHECK (json_valid(intent_json)),
  intent_sha256 TEXT NOT NULL CHECK (length(intent_sha256) = 64),
  qualification_sha256 TEXT NOT NULL CHECK (length(qualification_sha256) = 64),
  marker_ref TEXT NOT NULL UNIQUE,
  marker_oid TEXT NOT NULL,
  prepared_at TEXT NOT NULL,
  prepared_audit_id TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL CHECK (state IN (
    'prepared', 'committed', 'aborted_not_applied', 'reconciliation_required'
  )),
  outcome_json TEXT CHECK (outcome_json IS NULL OR json_valid(outcome_json)),
  UNIQUE (repo_id, idempotency_key),
  CHECK ((state = 'prepared' AND outcome_json IS NULL)
      OR (state <> 'prepared' AND outcome_json IS NOT NULL))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_forge_ref_operations_unresolved
  ON forge_ref_operations (repo_id)
  WHERE state IN ('prepared', 'reconciliation_required');

CREATE TABLE IF NOT EXISTS forge_ref_operation_outbox (
  id TEXT PRIMARY KEY,
  operation_id TEXT NOT NULL UNIQUE REFERENCES forge_ref_operations(id) ON DELETE RESTRICT,
  repo_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
  delivered_at TEXT,
  delivery_receipt TEXT,
  CHECK ((delivered_at IS NULL AND delivery_receipt IS NULL)
      OR (delivered_at IS NOT NULL AND delivery_receipt IS NOT NULL AND length(delivery_receipt) = 64))
);

CREATE INDEX IF NOT EXISTS idx_forge_ref_operation_outbox_pending
  ON forge_ref_operation_outbox (repo_id, created_at) WHERE delivered_at IS NULL;

CREATE TRIGGER IF NOT EXISTS forge_ref_operation_immutable
BEFORE UPDATE ON forge_ref_operations
WHEN OLD.state <> 'prepared'
  OR NEW.id IS NOT OLD.id OR NEW.repo_id IS NOT OLD.repo_id
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.intent_json IS NOT OLD.intent_json OR NEW.intent_sha256 IS NOT OLD.intent_sha256
  OR NEW.qualification_sha256 IS NOT OLD.qualification_sha256
  OR NEW.marker_ref IS NOT OLD.marker_ref OR NEW.marker_oid IS NOT OLD.marker_oid
  OR NEW.prepared_at IS NOT OLD.prepared_at OR NEW.prepared_audit_id IS NOT OLD.prepared_audit_id
BEGIN SELECT RAISE(ABORT, 'immutable ref operation binding or outcome'); END;

CREATE TRIGGER IF NOT EXISTS forge_ref_operation_outbox_committed
BEFORE INSERT ON forge_ref_operation_outbox
WHEN NOT EXISTS (SELECT 1 FROM forge_ref_operations
                 WHERE id = NEW.operation_id AND repo_id = NEW.repo_id AND state = 'committed')
BEGIN SELECT RAISE(ABORT, 'outbox requires committed operation'); END;

CREATE TRIGGER IF NOT EXISTS forge_ref_operation_outbox_immutable
BEFORE UPDATE ON forge_ref_operation_outbox
WHEN OLD.delivered_at IS NOT NULL
  OR NEW.id IS NOT OLD.id OR NEW.operation_id IS NOT OLD.operation_id
  OR NEW.repo_id IS NOT OLD.repo_id OR NEW.created_at IS NOT OLD.created_at
  OR NEW.payload_json IS NOT OLD.payload_json
BEGIN SELECT RAISE(ABORT, 'immutable outbox event or delivery'); END;
