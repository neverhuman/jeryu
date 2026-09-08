use rusqlite::Connection;

use crate::{Result, WorkError};

pub(super) const MIGRATION_0001: &str =
    include_str!("../../../../db/migrations/0001_work_tracker.sql");

pub(super) fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(MIGRATION_0001)
        .map_err(WorkError::storage)
}
