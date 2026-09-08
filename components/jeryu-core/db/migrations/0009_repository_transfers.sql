-- UUID-preserving repository transfers and read-only legacy aliases.

CREATE TABLE IF NOT EXISTS repository_transfer_journal (
  transaction_id TEXT PRIMARY KEY,
  idempotency_key TEXT NOT NULL UNIQUE,
  request_fingerprint TEXT NOT NULL,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  source_owner TEXT NOT NULL,
  source_name TEXT NOT NULL,
  destination_owner TEXT NOT NULL,
  destination_name TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('prepared', 'committed', 'failed')),
  prepared_at TEXT NOT NULL,
  completed_at TEXT,
  failure TEXT,
  receipt_json TEXT CHECK (receipt_json IS NULL OR json_valid(receipt_json))
);

CREATE TABLE IF NOT EXISTS repository_aliases (
  old_owner TEXT NOT NULL,
  old_name TEXT NOT NULL,
  repository_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
  canonical_owner TEXT NOT NULL,
  canonical_name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  transaction_id TEXT NOT NULL REFERENCES repository_transfer_journal(transaction_id),
  PRIMARY KEY (old_owner, old_name),
  UNIQUE (repository_id, old_owner, old_name)
);

CREATE INDEX IF NOT EXISTS repository_aliases_canonical_idx
  ON repository_aliases(canonical_owner, canonical_name);
