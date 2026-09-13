-- This table is part of Core's shared State, not an independently written
-- merge/CI journal. Future journal tables must survive ordinary State saves.
CREATE TABLE IF NOT EXISTS repository_mutation_blocks (
  repo_id TEXT PRIMARY KEY REFERENCES repositories(id) ON DELETE CASCADE,
  block_json TEXT NOT NULL CHECK (json_valid(block_json))
);
