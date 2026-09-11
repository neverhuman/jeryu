//! Durable local reception before planning. No HTTP, execution or publisher authority.
use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use hmac::{Hmac, Mac};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use std::path::{Path, PathBuf};

use crate::{audit_evidence, audit_ledger, audit_score::JsonObject};

#[path = "audit_intake_input.rs"]
mod input;
#[path = "audit_intake_store.rs"]
mod store;
#[path = "audit_intake_translate.rs"]
mod translate;

const MAX_BODY: usize = 25 * 1024 * 1024;
const MAX_CONFIG: usize = 64 * 1024;
const MAX_HEADERS: usize = 4096;

#[derive(Debug, Subcommand)]
pub(super) enum Operation {
    /// Store one bounded reception before payload parsing; no planner is invoked.
    Receive {
        #[arg(long)]
        route: PathBuf,
        #[arg(long)]
        headers: PathBuf,
        #[arg(long)]
        body: PathBuf,
    },
    /// Finish classifying receptions left pending by interruption; no source work.
    Reconcile,
    /// Append a worker-reported failure; cannot mark an event processed or passed.
    Failure {
        #[arg(long)]
        event_key: String,
        #[arg(long, value_enum)]
        kind: FailureKind,
        #[arg(long, allow_hyphen_values = true)]
        command_exit: Option<i32>,
        #[arg(long)]
        diagnostic: PathBuf,
    },
    /// Show all hashes, metadata and unresolved receptions; raw bodies remain private.
    Status,
}

#[derive(Clone, Copy, Debug, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(super) enum FailureKind {
    ParseError,
    SourceUnavailable,
    PlannerError,
    QueueError,
    TimedOut,
    Canceled,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Route {
    schema_version: String,
    route_id: String,
    repository_id: u64,
    repository: String,
    secret_file: PathBuf,
    secret_version: String,
    // These are owner-configured observations, not protected-policy certificates.
    receiver_source_commit: String,
    receiver_executable_sha256: String,
    governing_workflow_repository: String,
    governing_workflow_path: String,
    governing_workflow_commit: String,
    governing_workflow_blob: String,
    executor_source_commit: String,
    executor_executable_sha256: String,
    executor_receipt_sha256: String,
    governing_policy_sha256: String,
    execution_config_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Headers {
    delivery: String,
    event: String,
    #[serde(rename = "signature_256")]
    _signature_256: String,
}

// Authentication does not depend on unsigned delivery/event metadata.
// Unknown fields cannot grant authority; duplicate signature fields still fail.
#[derive(Deserialize)]
struct SignatureHeader {
    signature_256: String,
}

fn safe_id(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}
fn slug(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    parts.len() == 2
        && parts
            .iter()
            .all(|part| safe_id(part, 100) && !matches!(*part, "." | ".."))
}
fn oid(value: &str) -> bool {
    crate::audit_scheduler::hex(value, 40) && value.bytes().any(|byte| byte != b'0')
}
fn digest(value: &str) -> bool {
    crate::audit_scheduler::hex(value, 64) && value.bytes().any(|byte| byte != b'0')
}

impl Route {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let JsonObject(route): JsonObject<Self> = serde_json::from_slice(bytes)?;
        ensure!(
            route.schema_version == "jeryu.audit-intake-route/v1"
                && safe_id(&route.route_id, 100)
                && route.repository_id > 0
                && route.repository_id <= i64::MAX as u64
                && route.repository.starts_with("neverhuman/")
                && slug(&route.repository)
                && safe_id(&route.secret_version, 100),
            "invalid intake route"
        );
        ensure!(
            [
                &route.receiver_source_commit,
                &route.governing_workflow_commit,
                &route.governing_workflow_blob,
                &route.executor_source_commit
            ]
            .into_iter()
            .all(|value| oid(value)),
            "full configured source identities required"
        );
        ensure!(
            [
                &route.receiver_executable_sha256,
                &route.executor_executable_sha256,
                &route.executor_receipt_sha256,
                &route.governing_policy_sha256,
                &route.execution_config_sha256
            ]
            .into_iter()
            .all(|value| digest(value)),
            "full configured artifact identities required"
        );
        ensure!(
            slug(&route.governing_workflow_repository)
                && route
                    .governing_workflow_path
                    .starts_with(".github/workflows/")
                && route.governing_workflow_path.len() <= 512
                && route
                    .governing_workflow_path
                    .split('/')
                    .all(|part| safe_id(part, 100) && !matches!(part, "." | "..")),
            "invalid configured workflow identity"
        );
        ensure!(
            route.secret_file.is_absolute(),
            "secret file must be configured as an absolute path"
        );
        Ok(route)
    }
    fn public_context(&self) -> Value {
        json!({"route_id":self.route_id,"repository_id":self.repository_id,"repository":self.repository,
            "secret_version":self.secret_version,"receiver_source_commit":self.receiver_source_commit,
            "receiver_executable_sha256":self.receiver_executable_sha256,
            "governing_workflow_repository":self.governing_workflow_repository,
            "governing_workflow_path":self.governing_workflow_path,"governing_workflow_commit":self.governing_workflow_commit,
            "governing_workflow_blob":self.governing_workflow_blob,"executor_source_commit":self.executor_source_commit,
            "executor_executable_sha256":self.executor_executable_sha256,"executor_receipt_sha256":self.executor_receipt_sha256,
            "governing_policy_sha256":self.governing_policy_sha256,"execution_config_sha256":self.execution_config_sha256,
            "configuration_authenticated":false,"receiver_deployment_admitted":false,"headers_authenticated":false,
            "execution_verified":false,"governing_policy_authenticated":false,"publication_qualified":false})
    }
}

impl Headers {
    fn parse(bytes: &[u8]) -> Result<Self> {
        let JsonObject(headers): JsonObject<Self> = serde_json::from_slice(bytes)?;
        ensure!(
            headers.delivery.len() == 36
                && headers.delivery.bytes().enumerate().all(|(index, byte)| {
                    if matches!(index, 8 | 13 | 18 | 23) {
                        byte == b'-'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                }),
            "invalid delivery GUID"
        );
        ensure!(safe_id(&headers.event, 64), "invalid event header");
        Ok(headers)
    }
}

fn signature_matches(secret: &[u8], body: &[u8], header: &str) -> bool {
    let Some(hex) = header
        .strip_prefix("sha256=")
        .filter(|value| crate::audit_scheduler::hex(value, 64))
    else {
        return false;
    };
    let signature: Vec<u8> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| {
                if byte <= b'9' {
                    byte - b'0'
                } else {
                    byte - b'a' + 10
                }
            };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect();
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&signature).is_ok()
}

fn capture(
    connection: &mut Connection,
    route: Result<Vec<u8>>,
    headers: Result<Vec<u8>>,
    body: Result<Vec<u8>>,
    received_at: i64,
) -> Result<i64> {
    let route = route.ok();
    let headers = headers.ok();
    let body = body.ok();
    let parsed_route = route.as_deref().and_then(|bytes| Route::parse(bytes).ok());
    let parsed_headers = headers.as_deref().and_then(|bytes| {
        serde_json::from_slice::<JsonObject<SignatureHeader>>(bytes)
            .ok()
            .map(|value| value.0)
    });
    let authentication = match (&parsed_route, &parsed_headers, &body) {
        (Some(route), Some(headers), Some(body)) => {
            match input::read_private(&route.secret_file, 4096) {
                Ok(mut secret) if !secret.is_empty() => {
                    let matched = signature_matches(&secret, body, &headers.signature_256);
                    secret.fill(0); // Never retain, print or hash the configured secret.
                    if matched {
                        "matched"
                    } else {
                        "signature_mismatch"
                    }
                }
                _ => "secret_unavailable",
            }
        }
        (None, _, _) => "route_unavailable_or_invalid",
        (_, None, _) => "headers_unavailable_or_invalid",
        _ => "body_unavailable",
    };
    // This commit precedes every body parse and all later source/queue work.
    store::capture(
        connection,
        route.as_deref(),
        headers.as_deref(),
        body.as_deref(),
        authentication,
        received_at,
    )
}

pub(super) fn status_snapshot(connection: &Connection) -> Result<Value> {
    if connection.is_autocommit() {
        let transaction = connection.unchecked_transaction()?;
        let result = status_snapshot(&transaction)?;
        transaction.commit()?;
        return Ok(result);
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 1 {
        return Ok(
            json!({"schema_version":"jeryu.audit-intake-status/v1","migration_required":true,
        "receptions":null,"pending_received_events":null,"receiver_deployment_admitted":false,"publication_qualified":false}),
        );
    }
    store::status(connection)
}

pub(super) fn run(database: &Path, operation: Operation) -> Result<()> {
    let status_only = matches!(operation, Operation::Status);
    let mut connection = audit_ledger::open(database, status_only)?;
    let result = match operation {
        Operation::Receive {
            route,
            headers,
            body,
        } => {
            let id = capture(
                &mut connection,
                input::read_private(&route, MAX_CONFIG),
                input::read_private(&headers, MAX_HEADERS),
                input::read_private(&body, MAX_BODY),
                audit_ledger::now()?,
            )?;
            store::classify(&mut connection, id)?
        }
        Operation::Reconcile => store::reconcile(&mut connection)?,
        Operation::Failure {
            event_key,
            kind,
            command_exit,
            diagnostic,
        } => store::failure(
            &mut connection,
            &event_key,
            kind,
            command_exit,
            input::read_private(&diagnostic, MAX_CONFIG).ok(),
            audit_ledger::now()?,
        )?,
        Operation::Status => status_snapshot(&connection)?,
    };
    println!("{}", crate::canonical_json::pretty(result.clone())?);
    if status_only {
        bail!("local intake has no deployment, execution or publication admission");
    }
    if result.get("reception_accepted") == Some(&json!(false))
        || result
            .get("classification_errors")
            .is_some_and(|errors| errors.as_u64() != Some(0))
    {
        bail!("reception or classification failed; durable private records remain available");
    }
    Ok(())
}

#[cfg(test)]
#[path = "audit_intake_tests.rs"]
mod tests;
