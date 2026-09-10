//! Local, transactional attempt accounting. Imported JSON supplies no authority.
use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{audit_evidence, audit_scheduler as scheduler, audit_score::JsonObject};

#[path = "audit_ledger_store.rs"]
mod store;
#[path = "audit_ledger_validation.rs"]
mod validation;

const SCHEMA: i64 = 1;
const APPLICATION: i64 = 0x4a524c41;
const MAX_INPUT: u64 = 16 * 1024 * 1024;

#[derive(Debug, Subcommand)]
pub(super) enum Operation {
    /// Persist a planner output and its bound validation inputs, without authenticating them.
    ImportPlan {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        execution_config: PathBuf,
        #[arg(long)]
        governing_policy: PathBuf,
        #[arg(long)]
        candidate_policy: PathBuf,
    },
    /// Acquire a local execution lease for an imported scheduled request.
    Start {
        #[arg(long)]
        request_key: String,
        #[arg(long, default_value_t = 300)]
        lease_seconds: u64,
    },
    /// Append the executor's observed outcome; reports cannot grant trusted success.
    Finish {
        #[arg(long)]
        receipt: PathBuf,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Record the executor's separate closure acknowledgement; never inferred from time.
    Close {
        #[arg(long)]
        acknowledgement: PathBuf,
    },
    /// Append timeout observations for expired leases; do not release those leases.
    Reconcile,
    /// Print all attempts and unresolved obligations; exit nonzero while unqualified.
    Status,
}

fn now() -> Result<i64> {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
        .context("ledger clock overflow")
}

fn read_input(path: &Path) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.nlink() == 1 && before.len() <= MAX_INPUT,
        "ledger input must be a bounded ordinary file"
    );
    let mut bytes = Vec::new();
    file.by_ref().take(MAX_INPUT + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    ensure!(
        bytes.len() as u64 <= MAX_INPUT
            && before.len() == bytes.len() as u64
            && before.len() == after.len()
            && before.mtime() == after.mtime()
            && before.mtime_nsec() == after.mtime_nsec()
            && before.ctime() == after.ctime()
            && before.ctime_nsec() == after.ctime_nsec(),
        "ledger input changed while reading"
    );
    Ok(bytes)
}

fn owned_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && metadata.nlink() == 1
            && metadata.uid() == fs::metadata("/proc/self")?.uid()
            && metadata.mode() & 0o077 == 0,
        "ledger database must be an owner-only ordinary file"
    );
    Ok(())
}

fn open(path: &Path, read_only: bool) -> Result<Connection> {
    let parent = path
        .parent()
        .context("ledger database needs a parent directory")?;
    let metadata = fs::symlink_metadata(parent)?;
    ensure!(
        path.is_absolute()
            && parent.canonicalize()? == parent
            && metadata.is_dir()
            && metadata.uid() == fs::metadata("/proc/self")?.uid()
            && metadata.mode() & 0o077 == 0,
        "ledger parent must already be physical and owner-only"
    );
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let sidecar = PathBuf::from(format!(
            "{}{suffix}",
            path.to_str().context("UTF-8 ledger path required")?
        ));
        match fs::symlink_metadata(&sidecar) {
            Ok(_) => owned_file(&sidecar)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    if !read_only {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(file) => {
                file.sync_all()?;
                fs::File::open(parent)?.sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
    }
    owned_file(path)?;
    // Classify existing bytes through a read-only handle before any journaling
    // PRAGMA or write connection can alter a different application's database.
    let probe = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    probe.busy_timeout(Duration::from_secs(5))?;
    existing_kind(&probe)?;
    drop(probe);
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    } else {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    };
    let mut connection =
        Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NOFOLLOW)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON;")?;
    if read_only {
        check_schema(&connection)?;
    } else {
        connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")?;
        initialize(&mut connection)?;
    }
    Ok(connection)
}

fn check_schema(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let application: i64 = connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    ensure!(
        version == SCHEMA && application == APPLICATION,
        "unsupported audit ledger database"
    );
    Ok(())
}

fn existing_kind(connection: &Connection) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let application: i64 = connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if version == 0 && application == 0 {
        let tables: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        ensure!(tables == 0, "refusing a non-ledger database");
        return Ok(());
    }
    check_schema(connection)
}

fn initialize(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    existing_kind(&transaction)?;
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 {
        let tables: i64 = transaction.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        ensure!(tables == 0, "refusing to initialize a nonempty database");
        transaction.execute_batch(r"
            CREATE TABLE plans (
                id TEXT PRIMARY KEY, event_key TEXT UNIQUE NOT NULL,
                bytes BLOB NOT NULL, imported_at INTEGER NOT NULL
            ) STRICT;
            CREATE TABLE jobs (
                key TEXT PRIMARY KEY, identity BLOB NOT NULL, source_commit TEXT NOT NULL,
                source_tree TEXT NOT NULL, config BLOB NOT NULL, governing_policy BLOB NOT NULL,
                candidate_policy BLOB NOT NULL
            ) STRICT;
            CREATE TABLE requests (
                key TEXT PRIMARY KEY, job_key TEXT NOT NULL REFERENCES jobs(key)
            ) STRICT;
            CREATE TABLE plan_jobs (
                plan_id TEXT NOT NULL REFERENCES plans(id),
                request_key TEXT NOT NULL REFERENCES requests(key),
                PRIMARY KEY(plan_id,request_key)
            ) STRICT;
            CREATE TABLE attempts (
                id TEXT PRIMARY KEY, job_key TEXT NOT NULL REFERENCES jobs(key),
                request_key TEXT NOT NULL REFERENCES requests(key), ordinal INTEGER NOT NULL,
                started_at INTEGER NOT NULL, deadline INTEGER NOT NULL,
                UNIQUE(job_key,ordinal), CHECK(ordinal > 0), CHECK(deadline > started_at)
            ) STRICT;
            CREATE TABLE observations (
                id TEXT PRIMARY KEY, attempt_id TEXT NOT NULL REFERENCES attempts(id),
                kind TEXT NOT NULL CHECK(kind IN ('finish','timeout')),
                outcome TEXT NOT NULL CHECK(outcome IN ('completed_unqualified','failed_policy','tool_error','timed_out','source_unavailable','canceled')),
                recorded_at INTEGER NOT NULL, receipt BLOB NOT NULL, report BLOB,
                receipt_sha256 TEXT NOT NULL, report_sha256 TEXT, summary BLOB,
                diagnostic TEXT NOT NULL, reason TEXT NOT NULL, UNIQUE(attempt_id,kind)
            ) STRICT;
            CREATE TABLE closures (
                attempt_id TEXT PRIMARY KEY REFERENCES attempts(id),
                acknowledged_at INTEGER NOT NULL, acknowledgement BLOB NOT NULL,
                acknowledgement_sha256 TEXT NOT NULL
            ) STRICT;
            CREATE INDEX attempt_job ON attempts(job_key,ordinal);
            CREATE INDEX observation_attempt ON observations(attempt_id);
        ")?;
        // All authoritative accounting rows are append-only. Derived state has no mutable table.
        for table in [
            "plans",
            "jobs",
            "requests",
            "plan_jobs",
            "attempts",
            "observations",
            "closures",
        ] {
            for operation in ["UPDATE", "DELETE"] {
                transaction.execute_batch(&format!(
                    "CREATE TRIGGER {table}_no_{operation} BEFORE {operation} ON {table} BEGIN SELECT RAISE(ABORT,'audit ledger is append-only'); END;"
                ))?;
            }
        }
        transaction.pragma_update(None, "application_id", APPLICATION)?;
        transaction.pragma_update(None, "user_version", SCHEMA)?;
    } else {
        check_schema(&transaction)?;
    }
    transaction.commit()?;
    Ok(())
}

pub(super) fn run(database: &Path, operation: Operation) -> Result<()> {
    let mut connection = open(database, matches!(operation, Operation::Status))?;
    let status_only = matches!(operation, Operation::Status);
    let result = match operation {
        Operation::ImportPlan {
            plan,
            execution_config,
            governing_policy,
            candidate_policy,
        } => store::import(
            &mut connection,
            &read_input(&plan)?,
            &read_input(&execution_config)?,
            &read_input(&governing_policy)?,
            &read_input(&candidate_policy)?,
            now()?,
        )?,
        Operation::Start {
            request_key,
            lease_seconds,
        } => store::start(&mut connection, &request_key, lease_seconds, now()?)?,
        Operation::Finish { receipt, report } => {
            let receipt = read_input(&receipt)?;
            // A supplied report that is unreadable is an explicit executor error, never a prior green result.
            let report = report.map(|path| read_input(&path));
            store::finish(&mut connection, &receipt, report, now()?)?
        }
        Operation::Close { acknowledgement } => {
            store::close(&mut connection, &read_input(&acknowledgement)?, now()?)?
        }
        Operation::Reconcile => store::reconcile(&mut connection, now()?)?,
        Operation::Status => store::status(&connection)?,
    };
    println!("{}", crate::canonical_json::pretty(result)?);
    if status_only {
        bail!("local ledger has no trusted audit admission; required audits remain unqualified");
    }
    Ok(())
}

#[cfg(test)]
#[path = "audit_ledger_tests.rs"]
mod tests;
