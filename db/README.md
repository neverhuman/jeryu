# Database

Migrations live in `db/migrations/`. Optional constraint scripts in `db/constraints/`.

RedlineDB is the only embedded state-store backend for jeryu. Do not use
SQLite, SQLx SQLite features, or SQLite URLs for fixtures, alternate paths, or local
workarounds; fix RedlineDB or the RedlineDB adapter surface instead.
