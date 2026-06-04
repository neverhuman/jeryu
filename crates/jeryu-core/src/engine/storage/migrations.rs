use rusqlite::Connection;

use super::{Result, storage_error};

const MIGRATION_0001: &str = include_str!("../../../../../db/migrations/0001_core_forge.sql");
const MIGRATION_0002: &str = include_str!("../../../../../db/migrations/0002_core_forge_aux.sql");
const MIGRATION_0003: &str =
    include_str!("../../../../../db/migrations/0003_core_forge_readmes.sql");
const MIGRATION_0004: &str =
    include_str!("../../../../../db/migrations/0004_core_forge_pull_request_source_repository.sql");
const MIGRATION_0004_BACKFILL: &str = r#"
UPDATE pull_requests
SET source_repository = (
  SELECT r.full_name
  FROM repositories r
  WHERE r.id = pull_requests.repo_id
)
WHERE source_repository = '';
"#;

pub(super) fn apply_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(MIGRATION_0001).map_err(storage_error)?;
    conn.execute_batch(MIGRATION_0002).map_err(storage_error)?;
    conn.execute_batch(MIGRATION_0003).map_err(storage_error)?;
    apply_migration_0004(conn)?;
    Ok(())
}

fn apply_migration_0004(conn: &Connection) -> Result<()> {
    if !pull_request_source_repository_exists(conn)? {
        conn.execute_batch(MIGRATION_0004).map_err(storage_error)?;
        return Ok(());
    }

    conn.execute_batch(MIGRATION_0004_BACKFILL)
        .map_err(storage_error)?;
    Ok(())
}

fn pull_request_source_repository_exists(conn: &Connection) -> Result<bool> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(pull_requests)")
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let name: String = row.get(1).map_err(storage_error)?;
        if name == "source_repository" {
            return Ok(true);
        }
    }
    Ok(false)
}
