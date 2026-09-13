-- No repository FK: receipts must survive deletion to prevent UUID resurrection.
CREATE TABLE IF NOT EXISTS repository_creation_journal (
  repository_id TEXT PRIMARY KEY,
  receipt_json TEXT NOT NULL CHECK (json_valid(receipt_json))
);
