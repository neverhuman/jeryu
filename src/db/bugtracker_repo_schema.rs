pub fn bugtracker_schema_ddl() -> &'static str {
    r#"
    CREATE TABLE IF NOT EXISTS bug_projects (
        alias TEXT PRIMARY KEY,
        repo_root TEXT NOT NULL,
        repo_slug TEXT NOT NULL,
        provider_kind TEXT NOT NULL,
        provider_project_id TEXT,
        default_branch TEXT NOT NULL DEFAULT 'main',
        metadata_json TEXT NOT NULL DEFAULT '{}',
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS bug_project_edges (
        source_project TEXT NOT NULL,
        target_project TEXT NOT NULL,
        kind TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (source_project, target_project, kind)
    );
    CREATE TABLE IF NOT EXISTS bugs (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL,
        source_project TEXT NOT NULL,
        target_project TEXT NOT NULL,
        component TEXT,
        status TEXT NOT NULL,
        severity TEXT NOT NULL,
        priority TEXT NOT NULL,
        difficulty INTEGER NOT NULL,
        repro_state TEXT NOT NULL,
        impact TEXT NOT NULL,
        security INTEGER NOT NULL DEFAULT 0,
        owner TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        body_json TEXT NOT NULL,
        idempotency_key TEXT UNIQUE
    );
    CREATE INDEX IF NOT EXISTS idx_bugs_target_status ON bugs(target_project, status, severity, priority);
    CREATE INDEX IF NOT EXISTS idx_bugs_updated ON bugs(updated_at);
    CREATE TABLE IF NOT EXISTS bug_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bug_id TEXT NOT NULL,
        event_type TEXT NOT NULL,
        actor TEXT NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_bug_events_bug ON bug_events(bug_id, id);
    CREATE TABLE IF NOT EXISTS bug_attempts (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bug_id TEXT NOT NULL,
        agent TEXT,
        status TEXT NOT NULL,
        sandbox_path TEXT,
        branch TEXT,
        base_sha TEXT,
        head_sha TEXT,
        pr_url TEXT,
        ci_evidence TEXT,
        notes TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_bug_attempts_bug ON bug_attempts(bug_id, id);
    CREATE TABLE IF NOT EXISTS bug_links (
        bug_id TEXT NOT NULL,
        linked_bug_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (bug_id, linked_bug_id, kind)
    );
    CREATE TABLE IF NOT EXISTS bug_external_refs (
        bug_id TEXT NOT NULL,
        provider TEXT NOT NULL,
        external_id TEXT,
        url TEXT,
        labels_json TEXT NOT NULL DEFAULT '[]',
        sync_status TEXT NOT NULL DEFAULT 'local',
        updated_at TEXT NOT NULL,
        PRIMARY KEY (bug_id, provider)
    );
    CREATE TABLE IF NOT EXISTS bug_evidence (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        bug_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        summary TEXT NOT NULL,
        path TEXT,
        url TEXT,
        digest TEXT,
        redacted INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    );
    "#
}

#[cfg(test)]
pub(crate) async fn fresh_bugtracker_pool() -> crate::db::AnyPool {
    use super::BugTrackerRepo;
    use crate::db::{AnyPoolOptions, install_default_drivers};
    install_default_drivers();
    let tmp = tempfile::NamedTempFile::new().expect("tempfile for bugtracker pool");
    let url = crate::db::config::sqlite_url(tmp.path());
    let pool = AnyPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect bugtracker sqlite");
    BugTrackerRepo::new(pool.clone())
        .install_schema()
        .await
        .unwrap();
    std::mem::forget(tmp);
    pool
}
