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
const MIGRATION_0006: &str = include_str!("../../../../../db/migrations/0006_forge_audit_log.sql");

const MIGRATION_0007: &str = include_str!("../../../../../db/migrations/0007_jankurai_scores.sql");
const MIGRATION_0008: &str = include_str!("../../../../../db/migrations/0008_user_auth_access.sql");
const MIGRATION_0009: &str =
    include_str!("../../../../../db/migrations/0009_repository_transfers.sql");
const MIGRATION_0010: &str =
    include_str!("../../../../../db/migrations/0010_account_lifecycle.sql");
const MIGRATION_0011: &str = include_str!("../../../../../db/migrations/0011_review_head_sha.sql");

pub(super) fn apply_migrations(conn: &Connection) -> Result<()> {
    apply_migrations_through_0010(conn)?;
    apply_migration_0011(conn)?;
    Ok(())
}

fn apply_migrations_through_0010(conn: &Connection) -> Result<()> {
    conn.execute_batch(MIGRATION_0001).map_err(storage_error)?;
    conn.execute_batch(MIGRATION_0002).map_err(storage_error)?;
    conn.execute_batch(MIGRATION_0003).map_err(storage_error)?;
    apply_migration_0004(conn)?;
    apply_migration_0005(conn)?;
    // 0006-0007 are pure CREATE TABLE/INDEX IF NOT EXISTS: idempotent, no guard.
    conn.execute_batch(MIGRATION_0006).map_err(storage_error)?;
    conn.execute_batch(MIGRATION_0007).map_err(storage_error)?;
    apply_migration_0008(conn)?;
    conn.execute_batch(MIGRATION_0009).map_err(storage_error)?;
    apply_migration_0010(conn)?;
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

fn apply_migration_0008(conn: &Connection) -> Result<()> {
    conn.execute_batch(MIGRATION_0008).map_err(storage_error)?;
    if !column_exists(conn, "user_accounts", "must_change_password")? {
        conn.execute_batch(
            "ALTER TABLE user_accounts ADD COLUMN must_change_password INTEGER NOT NULL DEFAULT 0;",
        )
        .map_err(storage_error)?;
    }
    if !column_exists(conn, "web_sessions", "csrf_token")? {
        conn.execute_batch(
            "ALTER TABLE web_sessions ADD COLUMN csrf_token TEXT NOT NULL DEFAULT '';",
        )
        .map_err(storage_error)?;
    }
    Ok(())
}

fn apply_migration_0010(conn: &Connection) -> Result<()> {
    validate_canonical_account_logins(conn)?;
    add_column_if_missing(
        conn,
        "user_accounts",
        "display_name",
        "ALTER TABLE user_accounts ADD COLUMN display_name TEXT NOT NULL DEFAULT '';",
    )?;
    conn.execute(
        "UPDATE user_accounts SET display_name = login WHERE display_name = ''",
        [],
    )
    .map_err(storage_error)?;
    add_column_if_missing(
        conn,
        "user_accounts",
        "status",
        "ALTER TABLE user_accounts ADD COLUMN status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('pending_activation', 'pending_mfa', 'active', 'disabled', 'locked'));",
    )?;
    add_column_if_missing(
        conn,
        "user_accounts",
        "auth_epoch",
        "ALTER TABLE user_accounts ADD COLUMN auth_epoch INTEGER NOT NULL DEFAULT 0 CHECK (auth_epoch >= 0);",
    )?;
    add_column_if_missing(
        conn,
        "web_sessions",
        "auth_epoch",
        "ALTER TABLE web_sessions ADD COLUMN auth_epoch INTEGER NOT NULL DEFAULT 0 CHECK (auth_epoch >= 0);",
    )?;
    add_column_if_missing(
        conn,
        "personal_access_tokens",
        "auth_epoch",
        "ALTER TABLE personal_access_tokens ADD COLUMN auth_epoch INTEGER NOT NULL DEFAULT 0 CHECK (auth_epoch >= 0);",
    )?;
    conn.execute_batch(MIGRATION_0010).map_err(storage_error)?;
    Ok(())
}

fn apply_migration_0011(conn: &Connection) -> Result<()> {
    add_column_if_missing(conn, "reviews", "head_sha", MIGRATION_0011)
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    statement: &str,
) -> Result<()> {
    if !column_exists(conn, table, column)? {
        conn.execute_batch(statement).map_err(storage_error)?;
    }
    Ok(())
}

fn validate_canonical_account_logins(conn: &Connection) -> Result<()> {
    let mut stmt = conn
        .prepare("SELECT login FROM user_accounts ORDER BY login")
        .map_err(storage_error)?;
    let mut rows = stmt.query([]).map_err(storage_error)?;
    let mut seen = std::collections::BTreeMap::<String, String>::new();
    let mut logins = Vec::new();
    while let Some(row) = rows.next().map_err(storage_error)? {
        let login: String = row.get(0).map_err(storage_error)?;
        let folded = login.to_ascii_lowercase();
        if let Some(previous) = seen.insert(folded.clone(), login.clone()) {
            return Err(super::ForgeError::Storage(format!(
                "account login case-fold collision: {previous} and {login}"
            )));
        }
        logins.push((login, folded));
    }
    for (login, folded) in logins {
        let canonical = login == folded
            && !login.is_empty()
            && login.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            });
        if !canonical {
            return Err(super::ForgeError::Storage(format!(
                "account login is not canonical lowercase ASCII: {login}"
            )));
        }
    }
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

    #[test]
    fn migration_0011_preserves_historical_reviews_as_stale() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        apply_migrations_through_0010(&conn).expect("migrate through 0010");
        assert!(!column_exists(&conn, "reviews", "head_sha").expect("inspect old shape"));
        conn.execute_batch(
            r#"
            INSERT INTO repositories (
              id, owner, name, full_name, private, default_branch, created_at, updated_at
            ) VALUES (
              '00000000-0000-0000-0000-000000000001', 'alice', 'demo', 'alice/demo', 1, 'main',
              '2026-08-26T00:00:00Z', '2026-08-26T00:00:00Z'
            );
            INSERT INTO issues (
              id, repo_id, number, title, state, author, created_at, updated_at
            ) VALUES (
              '00000000-0000-0000-0000-000000000002',
              '00000000-0000-0000-0000-000000000001',
              1, 'change', 'open', 'author',
              '2026-08-26T00:00:00Z', '2026-08-26T00:00:00Z'
            );
            INSERT INTO pull_requests (
              id, repo_id, number, issue_number, title, state, draft, author,
              head_json, base_json, mergeable, mergeable_state, merged,
              created_at, updated_at
            ) VALUES (
              '00000000-0000-0000-0000-000000000003',
              '00000000-0000-0000-0000-000000000001',
              1, 1, 'change', 'open', 0, 'author',
              '{"label":"feature","ref":"feature","sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}',
              '{"label":"main","ref":"main","sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}',
              0, 'blocked', 0, '2026-08-26T00:00:00Z', '2026-08-26T00:00:00Z'
            );
            INSERT INTO reviews (
              id, repo_id, pull_number, author, state, submitted_at
            ) VALUES (
              '00000000-0000-0000-0000-000000000004',
              '00000000-0000-0000-0000-000000000001',
              1, 'reviewer', 'APPROVED',
              '2026-08-26T00:00:00Z'
            );
            "#,
        )
        .expect("insert production-shape historical review");

        apply_migration_0011(&conn).expect("apply 0011");
        apply_migration_0011(&conn).expect("reapply 0011");
        let head_sha: Option<String> = conn
            .query_row(
                "SELECT head_sha FROM reviews WHERE id = '00000000-0000-0000-0000-000000000004'",
                [],
                |row| row.get(0),
            )
            .expect("read migrated review");
        assert_eq!(head_sha, None);
        let state = super::super::load_state(&conn).expect("load migrated production shape");
        let reviews = state
            .reviews
            .get(&("alice".to_string(), "demo".to_string(), 1))
            .expect("historical review retained");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].head_sha, None);
        assert!(
            crate::effective_reviews_for_head(reviews, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
                .is_empty()
        );
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
