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
const MIGRATION_0005: &str =
    include_str!("../../../../../db/migrations/0005_repository_family.sql");
// First-run seed only: known name-prefix groups whose rows carry no import
// provenance (created via the REST edge). Family is plain data afterwards,
// edited through the API.
const MIGRATION_0005_PREFIX_SEED: &str = r#"
UPDATE repositories SET family = 'jmcp-split'
WHERE family IS NULL AND (name = 'jmcp' OR name LIKE 'jmcp-%');
UPDATE repositories SET family = 'veox-split'
WHERE family IS NULL AND name LIKE 'veox-%';
"#;

pub(super) fn apply_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(MIGRATION_0001).map_err(storage_error)?;
    conn.execute_batch(MIGRATION_0002).map_err(storage_error)?;
    conn.execute_batch(MIGRATION_0003).map_err(storage_error)?;
    apply_migration_0004(conn)?;
    apply_migration_0005(conn)?;
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
    column_exists(conn, "pull_requests", "source_repository")
}

fn apply_migration_0005(conn: &Connection) -> Result<()> {
    if column_exists(conn, "repositories", "family")? {
        return Ok(());
    }
    conn.execute_batch(MIGRATION_0005).map_err(storage_error)?;
    seed_repository_families(conn)?;
    conn.execute_batch(MIGRATION_0005_PREFIX_SEED)
        .map_err(storage_error)?;
    Ok(())
}

/// First-run seed: derive a family from import provenance. Import stamps the
/// description as `imported from <path>`; a repository checked out under a
/// dedicated `*-split` grouping directory (the established repo-family
/// convention, e.g. `/home/ubuntu/veox-split/veox-nht`) belongs to the family
/// named after that directory. Direct home-dir imports stay standalone.
fn seed_repository_families(conn: &Connection) -> Result<()> {
    let mut updates: Vec<(String, String)> = Vec::new();
    {
        let mut stmt = conn
            .prepare("SELECT id, description FROM repositories WHERE family IS NULL")
            .map_err(storage_error)?;
        let mut rows = stmt.query([]).map_err(storage_error)?;
        while let Some(row) = rows.next().map_err(storage_error)? {
            let id: String = row.get(0).map_err(storage_error)?;
            let description: Option<String> = row.get(1).map_err(storage_error)?;
            if let Some(family) = family_from_import_provenance(description.as_deref()) {
                updates.push((id, family));
            }
        }
    }
    for (id, family) in updates {
        conn.execute(
            "UPDATE repositories SET family = ?1 WHERE id = ?2",
            rusqlite::params![family, id],
        )
        .map_err(storage_error)?;
    }
    Ok(())
}

fn family_from_import_provenance(description: Option<&str>) -> Option<String> {
    let path = description?.strip_prefix("imported from ")?.trim();
    let parent = std::path::Path::new(path).parent()?;
    let dir = parent.file_name()?.to_str()?;
    if dir.ends_with("-split") {
        Some(dir.to_string())
    } else {
        None
    }
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    while let Some(row) = rows.next().map_err(storage_error)? {
        let name: String = row.get(1).map_err(storage_error)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pre_0005_connection() -> Connection {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(MIGRATION_0001).expect("0001");
        conn.execute_batch(MIGRATION_0002).expect("0002");
        conn.execute_batch(MIGRATION_0003).expect("0003");
        apply_migration_0004(&conn).expect("0004");
        conn
    }

    fn insert_repo(conn: &Connection, id: &str, name: &str, description: Option<&str>) {
        conn.execute(
            r#"
            INSERT INTO repositories (
              id, owner, name, full_name, private, description, default_branch,
              archived, disabled, created_at, updated_at
            ) VALUES (?1, 'jeryu', ?2, 'jeryu/' || ?2, 1, ?3, 'main', 0, 0,
                      '2026-06-01T00:00:00Z', '2026-06-01T00:00:00Z')
            "#,
            rusqlite::params![id, name, description],
        )
        .expect("insert repo");
    }

    fn family_of(conn: &Connection, name: &str) -> Option<String> {
        conn.query_row(
            "SELECT family FROM repositories WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .expect("query family")
    }

    /// Upgrading a pre-0005 database adds the column and seeds families from
    /// `-split` import provenance and the known name-prefix groups, while
    /// direct home-dir imports stay standalone. Re-applying is a no-op that
    /// must not overwrite operator edits.
    #[test]
    fn sqlite_open_seeds_repository_families() {
        let conn = pre_0005_connection();
        insert_repo(
            &conn,
            "00000000-0000-0000-0000-000000000001",
            "veox-nht",
            Some("imported from /home/ubuntu/veox-split/veox-nht"),
        );
        insert_repo(
            &conn,
            "00000000-0000-0000-0000-000000000002",
            "jmcp-core",
            None,
        );
        insert_repo(
            &conn,
            "00000000-0000-0000-0000-000000000003",
            "jmcp",
            Some(""),
        );
        insert_repo(
            &conn,
            "00000000-0000-0000-0000-000000000004",
            "openQG",
            Some("imported from /home/ubuntu/openQG"),
        );

        apply_migration_0005(&conn).expect("0005");
        assert_eq!(family_of(&conn, "veox-nht").as_deref(), Some("veox-split"));
        assert_eq!(family_of(&conn, "jmcp-core").as_deref(), Some("jmcp-split"));
        assert_eq!(family_of(&conn, "jmcp").as_deref(), Some("jmcp-split"));
        assert_eq!(family_of(&conn, "openQG"), None);

        // Idempotent: a second apply leaves operator edits alone.
        conn.execute(
            "UPDATE repositories SET family = 'custom' WHERE name = 'openQG'",
            [],
        )
        .expect("operator edit");
        apply_migration_0005(&conn).expect("0005 reapply");
        assert_eq!(family_of(&conn, "openQG").as_deref(), Some("custom"));
    }
}
