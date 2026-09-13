-- Dedicated append-only restoration journal. Full State persistence does not
-- own these tables. A barrier survives every completed step and process exit.
CREATE TABLE IF NOT EXISTS forge_commissioning_operations (
    id TEXT PRIMARY KEY,
    contract_id TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    pair_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    closed INTEGER NOT NULL DEFAULT 0 CHECK (closed IN (0,1)),
    UNIQUE(contract_id, idempotency_key)
);
CREATE UNIQUE INDEX IF NOT EXISTS forge_commissioning_one_active_pair
ON forge_commissioning_operations(pair_id) WHERE closed = 0;
-- One Core storage database belongs to one actual backing pair. A changed or
-- falsely supplied pair UUID must not allow a second active restoration here.
CREATE UNIQUE INDEX IF NOT EXISTS forge_commissioning_one_active_runtime
ON forge_commissioning_operations((1)) WHERE closed = 0;
CREATE TABLE IF NOT EXISTS forge_commissioning_records (
    operation_id TEXT NOT NULL REFERENCES forge_commissioning_operations(id),
    revision INTEGER NOT NULL CHECK (revision > 0 AND revision <= 9007199254740991),
    record_sha256 TEXT NOT NULL,
    record_json TEXT NOT NULL,
    terminal INTEGER NOT NULL CHECK (terminal IN (0,1)),
    PRIMARY KEY(operation_id, revision)
);
CREATE TRIGGER IF NOT EXISTS forge_commissioning_record_no_update
BEFORE UPDATE ON forge_commissioning_records
BEGIN SELECT RAISE(ABORT, 'immutable commissioning record'); END;
CREATE TRIGGER IF NOT EXISTS forge_commissioning_record_no_delete
BEFORE DELETE ON forge_commissioning_records
BEGIN SELECT RAISE(ABORT, 'immutable commissioning record'); END;
CREATE TRIGGER IF NOT EXISTS forge_commissioning_operation_no_delete
BEFORE DELETE ON forge_commissioning_operations
BEGIN SELECT RAISE(ABORT, 'retained commissioning operation'); END;
CREATE TRIGGER IF NOT EXISTS forge_commissioning_operation_guard
BEFORE UPDATE ON forge_commissioning_operations
WHEN OLD.id IS NOT NEW.id OR OLD.contract_id IS NOT NEW.contract_id
  OR OLD.repo_id IS NOT NEW.repo_id OR OLD.pair_id IS NOT NEW.pair_id
  OR OLD.idempotency_key IS NOT NEW.idempotency_key OR OLD.closed != 0 OR NEW.closed != 1
  OR NOT EXISTS (
      SELECT 1 FROM forge_commissioning_records r
      WHERE r.operation_id = OLD.id AND r.terminal = 1
      AND r.revision = (SELECT MAX(revision) FROM forge_commissioning_records WHERE operation_id = OLD.id)
  )
BEGIN SELECT RAISE(ABORT, 'immutable commissioning scope or active barrier'); END;
