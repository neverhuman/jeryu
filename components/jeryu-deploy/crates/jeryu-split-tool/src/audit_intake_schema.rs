//! One transactional v1-to-v2 ledger extension; existing accounting rows are untouched.
use super::*;

pub(super) fn install(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    transaction.execute_batch(r"
        CREATE TABLE intake_receptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            received_at INTEGER NOT NULL,
            route_bytes BLOB, route_sha256 TEXT,
            header_bytes BLOB, header_sha256 TEXT,
            body_bytes BLOB, body_sha256 TEXT,
            authentication TEXT NOT NULL CHECK(authentication IN (
                'matched','signature_mismatch','secret_unavailable','route_unavailable_or_invalid',
                'headers_unavailable_or_invalid','body_unavailable')),
            CHECK((route_bytes IS NULL) = (route_sha256 IS NULL)),
            CHECK((header_bytes IS NULL) = (header_sha256 IS NULL)),
            CHECK((body_bytes IS NULL) = (body_sha256 IS NULL)),
            CHECK(length(route_bytes) <= 65536), CHECK(length(header_bytes) <= 4096),
            CHECK(length(body_bytes) <= 26214400)
        ) STRICT;
        CREATE TABLE intake_events (
            key TEXT PRIMARY KEY, route_id TEXT NOT NULL, body_sha256 TEXT NOT NULL,
            first_reception_id INTEGER NOT NULL REFERENCES intake_receptions(id), metadata BLOB NOT NULL,
            UNIQUE(route_id,body_sha256)
        ) STRICT;
        CREATE TABLE intake_deliveries (
            key TEXT PRIMARY KEY, route_id TEXT NOT NULL, delivery_guid TEXT NOT NULL,
            body_sha256 TEXT NOT NULL, event_header TEXT NOT NULL,
            event_key TEXT NOT NULL REFERENCES intake_events(key),
            first_reception_id INTEGER NOT NULL REFERENCES intake_receptions(id),
            UNIQUE(route_id,delivery_guid)
        ) STRICT;
        CREATE TABLE intake_classifications (
            reception_id INTEGER PRIMARY KEY REFERENCES intake_receptions(id),
            event_key TEXT REFERENCES intake_events(key), metadata BLOB NOT NULL
        ) STRICT;
        CREATE TABLE intake_failures (
            id INTEGER PRIMARY KEY AUTOINCREMENT, event_key TEXT NOT NULL REFERENCES intake_events(key),
            kind TEXT NOT NULL CHECK(kind IN ('parse_error','source_unavailable','planner_error','queue_error','timed_out','canceled')),
            command_exit INTEGER, diagnostic BLOB, diagnostic_sha256 TEXT, recorded_at INTEGER NOT NULL,
            CHECK(length(diagnostic) <= 65536), CHECK((diagnostic IS NULL) = (diagnostic_sha256 IS NULL))
        ) STRICT;
        CREATE TABLE intake_processing_errors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            reception_id INTEGER NOT NULL REFERENCES intake_receptions(id),
            diagnostic BLOB NOT NULL CHECK(length(diagnostic) <= 16384),
            diagnostic_sha256 TEXT NOT NULL, recorded_at INTEGER NOT NULL
        ) STRICT;
        CREATE INDEX intake_reception_event ON intake_classifications(event_key,reception_id);
        CREATE INDEX intake_failure_event ON intake_failures(event_key,id);
    ")?;
    for table in [
        "intake_receptions",
        "intake_events",
        "intake_deliveries",
        "intake_classifications",
        "intake_failures",
        "intake_processing_errors",
    ] {
        for operation in ["UPDATE", "DELETE"] {
            transaction.execute_batch(&format!("CREATE TRIGGER {table}_no_{operation} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'audit intake is append-only'); END;"))?;
        }
    }
    Ok(())
}

/// Link received events to the existing queue without changing historical rows.
pub(super) fn install_plan_links(transaction: &rusqlite::Transaction<'_>) -> Result<()> {
    transaction.execute_batch(
        r"
        CREATE TABLE intake_plan_links (
            event_key TEXT NOT NULL REFERENCES intake_events(key),
            identity_sha256 TEXT NOT NULL,
            source_ref TEXT NOT NULL,
            plan_id TEXT NOT NULL REFERENCES plans(id),
            reception_id INTEGER NOT NULL REFERENCES intake_receptions(id),
            facts_sha256 TEXT NOT NULL,
            linked_at INTEGER NOT NULL,
            PRIMARY KEY(event_key,identity_sha256,source_ref)
        ) STRICT;
        CREATE TRIGGER intake_plan_links_no_UPDATE BEFORE UPDATE ON intake_plan_links
            BEGIN SELECT RAISE(ABORT,'audit intake is append-only'); END;
        CREATE TRIGGER intake_plan_links_no_DELETE BEFORE DELETE ON intake_plan_links
            BEGIN SELECT RAISE(ABORT,'audit intake is append-only'); END;
    ",
    )?;
    Ok(())
}
