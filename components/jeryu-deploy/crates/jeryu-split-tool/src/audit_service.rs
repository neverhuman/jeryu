//! Maintainer HTTP intake. The product process never starts or depends on it.
use super::*;
use axum::{
    Router,
    body::to_bytes,
    extract::{Path as RoutePath, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::{
    collections::BTreeMap,
    fs,
    net::SocketAddr,
    os::unix::fs::MetadataExt,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{net::TcpListener, sync::Semaphore};

#[derive(Debug, clap::Args)]
pub(crate) struct Arguments {
    /// Existing physical owner-only data directory; never put state in source.
    #[arg(long)]
    database: PathBuf,
    /// Private route configuration; repeat for each enrolled repository.
    #[arg(long, required = true)]
    route: Vec<PathBuf>,
    /// Loopback HTTP endpoint for a maintainer-managed TLS reverse proxy.
    #[arg(long, default_value = "127.0.0.1:8789")]
    listen: SocketAddr,
}

struct ReceiverRoute {
    bytes: Vec<u8>,
    secret: Vec<u8>,
}

impl Drop for ReceiverRoute {
    fn drop(&mut self) {
        self.secret.fill(0);
    }
}

struct Database {
    path: PathBuf,
    identity: (u64, u64, u32),
    connection: Connection,
}

impl Database {
    fn check_name(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.path)?;
        ensure!(
            metadata.is_file()
                && metadata.nlink() == 1
                && metadata.mode() & 0o077 == 0
                && (metadata.dev(), metadata.ino(), metadata.uid()) == self.identity,
            "receiver database custody changed"
        );
        Ok(())
    }
}

pub(super) struct Receiver {
    routes: BTreeMap<String, ReceiverRoute>,
    database: Mutex<Database>,
    slots: Arc<Semaphore>,
}

impl Receiver {
    pub(super) fn open(database: &Path, routes: &[PathBuf]) -> Result<Arc<Self>> {
        ensure!(
            !routes.is_empty() && routes.len() <= 64,
            "one to 64 routes required"
        );
        for ancestor in database.ancestors().skip(1) {
            ensure!(
                !ancestor.join(".git").try_exists()?,
                "receiver state must remain outside source checkouts"
            );
        }
        let mut prepared = BTreeMap::new();
        for path in routes {
            let bytes = input::read_private(path, MAX_CONFIG)?;
            let route = Route::parse(&bytes)?;
            let secret = input::read_private(&route.secret_file, 4096)?;
            ensure!(!secret.is_empty(), "empty webhook key");
            ensure!(
                prepared
                    .insert(route.route_id, ReceiverRoute { bytes, secret })
                    .is_none(),
                "duplicate receiver route"
            );
        }
        let mut connection = audit_ledger::open(database, false)?;
        // Raw receptions committed before an interrupted classification survive
        // restart. No planner, subprocess, network source or publisher runs here.
        let replay = store::reconcile(&mut connection)?;
        ensure!(
            replay["classification_errors"] == 0,
            "intake replay requires repair"
        );
        let metadata = fs::symlink_metadata(database)?;
        let database = Database {
            path: database.to_path_buf(),
            identity: (metadata.dev(), metadata.ino(), metadata.uid()),
            connection,
        };
        database.check_name()?;
        Ok(Arc::new(Self {
            routes: prepared,
            database: Mutex::new(database),
            slots: Arc::new(Semaphore::new(8)),
        }))
    }

    fn receive(&self, route_id: &str, headers: Vec<u8>, body: Option<Vec<u8>>) -> Result<Value> {
        let route = self
            .routes
            .get(route_id)
            .context("unknown receiver route")?;
        let signature = serde_json::from_slice::<JsonObject<SignatureHeader>>(&headers)
            .ok()
            .map(|value| value.0);
        let authentication = match (&signature, &body) {
            (Some(header), Some(body))
                if signature_matches(&route.secret, body, &header.signature_256) =>
            {
                "matched"
            }
            (Some(_), Some(_)) => "signature_mismatch",
            (None, _) => "headers_unavailable_or_invalid",
            (_, None) => "body_unavailable",
        };
        let mut database = self
            .database
            .lock()
            .map_err(|_| anyhow::anyhow!("receiver database unavailable"))?;
        database.check_name()?;
        let id = store::capture(
            &mut database.connection,
            Some(&route.bytes),
            Some(&headers),
            body.as_deref(),
            authentication,
            audit_ledger::now()?,
        )?;
        // The raw payload is durably committed before classification and before
        // any HTTP success. Classification failure leaves it for startup replay.
        let result = store::classify(&mut database.connection, id)?;
        database.check_name()?;
        Ok(result)
    }
}

fn metadata(headers: &HeaderMap) -> Result<Vec<u8>> {
    let mut fields = serde_json::Map::new();
    for (wire, field) in [
        ("x-github-delivery", "delivery"),
        ("x-github-event", "event"),
        ("x-hub-signature-256", "signature_256"),
    ] {
        let values: Vec<_> = headers.get_all(wire).iter().collect();
        let value = match values.as_slice() {
            [value] if value.to_str().is_ok() => json!(value.to_str()?),
            _ => json!(
                values
                    .iter()
                    .map(|value| value.as_bytes())
                    .collect::<Vec<_>>()
            ),
        };
        // Duplicate or non-text metadata remains data, and cannot become a
        // single valid signature/header through implicit first-value selection.
        fields.insert(field.into(), value);
    }
    let bytes = serde_json::to_vec(&fields)?;
    ensure!(
        bytes.len() <= MAX_HEADERS,
        "webhook metadata exceeds retention limit"
    );
    Ok(bytes)
}

fn reply(code: StatusCode, state: &str, reception: Option<&Value>) -> Response {
    (
        code,
        axum::Json(json!({
            "schema_version":"jeryu.audit-receiver-response/v1",
            "state":state,
            "reception_id":reception.and_then(|value| value.get("reception_id")),
            "audit_execution_admitted":false,
            "publication_qualified":false
        })),
    )
        .into_response()
}

async fn receive(
    State(receiver): State<Arc<Receiver>>,
    RoutePath(route_id): RoutePath<String>,
    request: Request,
) -> Response {
    if !receiver.routes.contains_key(&route_id) {
        return reply(StatusCode::NOT_FOUND, "unknown_route", None);
    }
    let Ok(slot) = Arc::clone(&receiver.slots).try_acquire_owned() else {
        return reply(StatusCode::SERVICE_UNAVAILABLE, "receiver_busy", None);
    };
    let headers = match metadata(request.headers()) {
        Ok(bytes) => bytes,
        Err(_) => {
            return reply(
                StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                "metadata_too_large",
                None,
            );
        }
    };
    let (body, transport_error) = match tokio::time::timeout(
        Duration::from_secs(10),
        to_bytes(request.into_body(), MAX_BODY),
    )
    .await
    {
        Ok(Ok(bytes)) => (Some(bytes.to_vec()), None),
        Ok(Err(_)) => (None, Some(StatusCode::PAYLOAD_TOO_LARGE)),
        Err(_) => (None, Some(StatusCode::REQUEST_TIMEOUT)),
    };
    let result = tokio::task::spawn_blocking(move || {
        // Retain the concurrency permit until the transaction finishes, even
        // when the HTTP connection disappears after sending its raw body.
        let _slot = slot;
        receiver.receive(&route_id, headers, body)
    })
    .await;
    match result {
        Ok(Ok(result)) => {
            if let Some(status) = transport_error {
                reply(status, "incomplete_body_retained", Some(&result))
            } else if result["reception_accepted"] == true {
                reply(
                    StatusCode::ACCEPTED,
                    "received_pending_audit",
                    Some(&result),
                )
            } else {
                reply(StatusCode::BAD_REQUEST, "reception_rejected", Some(&result))
            }
        }
        _ => reply(
            StatusCode::SERVICE_UNAVAILABLE,
            "durable_reception_unavailable",
            None,
        ),
    }
}

pub(super) fn router(receiver: Arc<Receiver>) -> Router {
    Router::new()
        .route("/hooks/:route_id", post(receive))
        .route(
            "/health",
            get(|| async { reply(StatusCode::OK, "listener_available", None) }),
        )
        .with_state(receiver)
}

pub(crate) fn run(arguments: Arguments) -> Result<()> {
    ensure!(
        arguments.listen.ip().is_loopback(),
        "receiver requires loopback; terminate remote TLS at a trusted proxy"
    );
    ensure!(
        fs::metadata("/proc/self")?.uid() != 0,
        "receiver must run as an unprivileged maintainer identity"
    );
    let receiver = Receiver::open(&arguments.database, &arguments.route)?;
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?
        .block_on(async {
            let listener = TcpListener::bind(arguments.listen).await?;
            eprintln!("audit receiver listening on {}", listener.local_addr()?);
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            axum::serve(listener, router(receiver))
                .with_graceful_shutdown(async move {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {},
                        _ = terminate.recv() => {},
                    }
                })
                .await
                .context("audit receiver transport failed")
        })
}
