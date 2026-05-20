# Database

Migrations live in `db/migrations/`. Optional constraint scripts in `db/constraints/`.

SQLite is the default embedded state-store backend for jeryu. RedlineDB remains
available only through the explicit `redlinedb-backend` feature and Redline URL
configuration. Keep SQLite wiring in the DB boundary and Cargo backend feature
configuration; do not add ad hoc database fixtures in application code.
