-- Append-only forge audit trail for privileged mutations (repository deletes).
--
-- Deliberately decoupled from the full-state-rewrite persistence path:
-- `SqliteStore::persist` rewrites every state table (DELETE FROM + reinsert)
-- on each mutation, so this table carries NO foreign key to repositories and
-- is never touched by delete_all/persist_state — rows are appended through a
-- fresh connection in `ForgeCore::append_audit` and survive every rewrite.
-- `subject` is the denormalized "owner/name" string so the trail outlives the
-- repository row it describes.
--
-- timeout-guard:
--   lock_timeout = '5s'
--   statement_timeout = '60s'
CREATE TABLE IF NOT EXISTS forge_audit_log (
  id TEXT PRIMARY KEY,
  occurred_at TEXT NOT NULL,
  actor TEXT NOT NULL DEFAULT 'local',
  action TEXT NOT NULL CHECK (length(trim(action)) > 0),
  subject TEXT NOT NULL,
  phase TEXT NOT NULL CHECK (phase IN ('requested', 'completed', 'failed')),
  detail_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(detail_json))
);

CREATE INDEX IF NOT EXISTS idx_forge_audit_log_subject_occurred_at
  ON forge_audit_log (subject, occurred_at);
