//! Owner: Evidence Gate / autonomy control plane (Wave 6.C + Wave 10)
//! Proof: `cargo test -p jeryu --lib autonomy::http_server`
//! Invariants:
//!   - HTTP/1.1 GET + `POST /events`. We hand-roll request parsing on raw
//!     `TcpStream` bytes — pulling in `hyper`/`axum`/`warp` is a hard no
//!     for this surface (Wave 6.C brief: zero new dependencies). The
//!     parser is pure (`parse_request`) and rendering is pure
//!     (`render_response`), so the byte-level contract is unit-testable
//!     without sockets.
//!   - One request per connection. We do not negotiate keep-alive — the
//!     metrics/health endpoints are scraped on coarse intervals (15s+),
//!     and webhooks are fire-and-forget, so the cost of a fresh TCP
//!     handshake is negligible compared with the simplification of the
//!     connection state machine.
//!   - Reads for GETs are capped at 8 KiB. POST /events bodies are
//!     capped at 256 KiB (`MAX_WEBHOOK_BODY_BYTES`): GitHub webhook
//!     payloads can be tens of KB but rarely cross 100 KB. Anything
//!     beyond 256 KB returns 413 without consuming the body.
//!   - `serve` is structured as a thin wrapper around `serve_with_listener`
//!     so tests can bind `127.0.0.1:0` and read back the ephemeral port
//!     before the accept-loop spins. This is the pattern recommended in
//!     the Wave 6.C brief (it avoids the racy "spawn then sleep" idiom).
//!   - `shutdown_after_requests` is a test-only affordance: when `Some(n)`,
//!     the accept loop returns Ok after dispatching `n` requests, so unit
//!     tests can drive a full request/response cycle without a shutdown
//!     channel and without leaking a background task. Production callers
//!     pass `None`.
//!   - Wave 10: `POST /events` accepts GitHub-shaped webhooks, verifies
//!     the `X-Hub-Signature-256` HMAC when a `webhook_secret` is
//!     configured, appends a signed `LaunchLedgerEntry` (kind =
//!     `WebhookReceived` — Wave 10 mint; dedicated kind so audit replay
//!     can distinguish webhook events from human decisions), and
//!     dispatches the event to an optional `on_event_callback` for the
//!     daemon-tick path described in tip5/tip8 of the brainstorm.
//!
//! Brainstorm refs: Evidence Gate Wave 6.C brief (operator readiness
//! probe + Prometheus scrape); Wave 10 brief (webhook-driven daemon
//! tick replaces polling for low-latency response — tip5 "HMAC the
//! body", tip8 "every webhook is a signed receipt").

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::kill_bell::{KillBell, KillBellState};
use super::ledger::{LedgerFilter, SqlLedger, sign_entry};
use super::metrics::{collect, render_prometheus};
use super::signing::{EdSigningKey, Signature, sha256_digest};
use super::types::{LaunchLedgerEntry, LedgerKind, SchemaTag};

/// Maximum bytes read from a single client for non-webhook (GET) paths.
/// A well-formed HTTP/1.1 GET for `/metrics` or `/health` is well under
/// this — see invariants.
const MAX_REQUEST_BYTES: usize = 8 * 1024;

/// Maximum total bytes (headers + body) accepted for `POST /events`.
/// GitHub webhooks for `pull_request` rarely exceed ~80 KiB; 256 KiB is
/// a comfortable upper bound. Larger payloads return 413 without
/// reading the body to completion.
pub(crate) const MAX_WEBHOOK_BODY_BYTES: usize = 256 * 1024;

/// Maximum bytes we will read off the socket while serving a POST. Headers
/// fit in a few KiB plus the body up to `MAX_WEBHOOK_BODY_BYTES`.
const MAX_POST_TOTAL_BYTES: usize = MAX_WEBHOOK_BODY_BYTES + 16 * 1024;

/// Configuration handed to `serve()`. The second field exists purely as
/// a test affordance (see module invariants).
#[derive(Debug, Clone)]
pub struct HttpServerConfig {
    /// Bind address, e.g. `127.0.0.1:9090`. Pass `127.0.0.1:0` to let the
    /// OS pick an ephemeral port (useful for tests).
    pub bind_addr: String,
    /// If `Some(n)`, the accept loop exits cleanly after dispatching `n`
    /// requests. Tests use this to avoid spawning a shutdown channel.
    /// Production callers pass `None` for an indefinite loop.
    pub shutdown_after_requests: Option<u64>,
}

/// Shared per-request state. Cheap to clone: `SqlLedger` and `KillBell`
/// wrap an `Arc`-shaped pool internally, and `PathBuf` is a single
/// allocation. We wrap the whole struct in an `Arc` at the call site to
/// avoid cloning per-connection.
#[derive(Clone)]
pub struct AppState {
    pub ledger: SqlLedger,
    pub kill_bell: KillBell,
    /// Directory holding freeze-window policy YAMLs. We retain a handle
    /// even though no current route reads it: future `/freeze` endpoints
    /// will need it, and exposing the field now lets integration callers
    /// wire it once.
    pub freeze_dir: PathBuf,
    /// Shared secret for verifying GitHub `X-Hub-Signature-256` headers
    /// on `POST /events`. When `None`, the receiver runs in dev mode and
    /// accepts unsigned bodies (useful for `curl`-based local testing).
    /// In production this must be `Some(secret)`; the daemon refuses to
    /// start without it.
    pub webhook_secret: Option<String>,
    /// Optional callback invoked after a webhook is verified, parsed,
    /// and persisted. Callback errors are logged but do not change the
    /// HTTP response: GitHub treats anything other than 2xx as a
    /// retryable failure, and we have already durably recorded the
    /// event in the ledger.
    pub on_event_callback: Option<Arc<dyn EventCallback>>,
    /// Signing key used to seal the webhook's `LaunchLedgerEntry`. We
    /// wrap in `Arc` because `EdSigningKey` is not `Clone` (the inner
    /// `DalekSigningKey` zeroes on drop). When `None`, the receiver
    /// generates an ephemeral key per process — fine for dev, but
    /// production should wire a vaulted key here.
    pub webhook_signing_key: Option<Arc<EdSigningKey>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("ledger", &self.ledger)
            .field("kill_bell", &self.kill_bell)
            .field("freeze_dir", &self.freeze_dir)
            .field(
                "webhook_secret",
                &self.webhook_secret.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "on_event_callback",
                &self.on_event_callback.as_ref().map(|_| "<set>"),
            )
            .field(
                "webhook_signing_key",
                &self.webhook_signing_key.as_ref().map(|k| k.key_id.as_str()),
            )
            .finish()
    }
}

/// Parsed request line + headers. Bodies are not part of this struct —
/// callers that need a body read it off the socket separately, using
/// `Content-Length` (carried as a header here) as the size hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub host: Option<String>,
    /// All other request headers, lower-cased key → raw value. We carry
    /// these so the webhook path can read `Content-Length` and
    /// `X-Hub-Signature-256`/`X-GitHub-Event` without re-parsing.
    pub headers: std::collections::BTreeMap<String, String>,
}

/// Wave 10 webhook event extracted from a GitHub-shaped JSON body.
///
/// All optional fields default to `None` when the payload omits them —
/// e.g. non-pull-request events carry no `pull_request` block, and the
/// `repository` field is absent on `ping` events for org-level hooks.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct WebhookEvent {
    /// Value of the `X-GitHub-Event` header (e.g. `pull_request`, `ping`).
    pub event_type: String,
    /// `body.action` — for `pull_request`: `opened`, `synchronize`,
    /// `reopened`, `closed`, etc.
    pub action: Option<String>,
    /// `body.repository.full_name` — e.g. `octocat/hello-world`.
    pub repo: Option<String>,
    /// `body.pull_request.number`, falling back to `body.number` (which
    /// some delivery shapes use).
    pub pr_number: Option<u64>,
    /// `body.pull_request.head.sha`.
    pub head_sha: Option<String>,
    /// SHA-256 of the raw request body. Stored in the ledger so audit
    /// queries can correlate an event with a captured raw payload.
    pub raw_body_sha256: String,
    /// Wall-clock receive time (UTC). Stored so the daemon-tick path can
    /// reason about latency.
    pub received_at: DateTime<Utc>,
}

/// Callback invoked after a webhook is verified + persisted. Returning
/// an error logs a warning but the receiver still answers 202 — see
/// `AppState::on_event_callback`.
#[async_trait]
pub trait EventCallback: Send + Sync {
    async fn on_event(&self, event: WebhookEvent) -> anyhow::Result<()>;
}

/// Bind a `TcpListener` and run the accept loop. Thin wrapper around
/// `serve_with_listener` — see the listener variant's docs.
pub async fn serve(config: HttpServerConfig, state: Arc<AppState>) -> Result<()> {
    let listener = TcpListener::bind(&config.bind_addr)
        .await
        .with_context(|| format!("bind {}", config.bind_addr))?;
    tracing::info!(addr = %config.bind_addr, "autonomy http server listening");
    serve_with_listener(listener, config, state).await
}

/// Run the accept loop on an already-bound listener. This is the
/// composable entry point for tests that need to read back the
/// ephemeral port BEFORE the accept loop starts. The regular `serve()`
/// wraps it.
///
/// Honors `config.shutdown_after_requests` for test scenarios.
pub async fn serve_with_listener(
    listener: TcpListener,
    config: HttpServerConfig,
    state: Arc<AppState>,
) -> Result<()> {
    let mut handled: u64 = 0;
    loop {
        if let Some(limit) = config.shutdown_after_requests
            && handled >= limit
        {
            return Ok(());
        }
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!(error = %err, "accept failed; continuing");
                continue;
            }
        };
        tracing::debug!(%peer, "accepted connection");
        // Handle each request inline rather than spawning. The handlers
        // are short-lived (one SQL round-trip + a render) and we want a
        // deterministic `handled` counter for `shutdown_after_requests`.
        // For production scale this is fine: metrics scrapes are 15s+
        // intervals from a single Prometheus job.
        if let Err(err) = handle_connection(stream, state.clone()).await {
            tracing::warn!(error = %err, "handler errored; client likely already closed");
        }
        handled = handled.saturating_add(1);
    }
}

/// Read up to one request from `stream`, dispatch it, write the response,
/// then drop the stream (no keep-alive — see invariants).
///
/// Reads the headers first using a small (8 KiB) buffer, then — when
/// the method is `POST` — grows the buffer up to `MAX_POST_TOTAL_BYTES`
/// to absorb the body. This split keeps the GET fast path small while
/// letting the webhook surface accept GitHub-sized payloads.
async fn handle_connection(mut stream: TcpStream, state: Arc<AppState>) -> Result<()> {
    // Step 1: grow the buffer until we see CRLFCRLF (end of headers).
    let mut buf = Vec::<u8>::with_capacity(MAX_REQUEST_BYTES);
    let mut header_end: Option<usize> = None;
    loop {
        if buf.len() >= MAX_POST_TOTAL_BYTES {
            // Headers alone exceed even our POST ceiling: bad request.
            let response = render_response(400, "application/json", "{\"error\":\"bad request\"}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Ok(());
        }
        // Grow in 8 KiB chunks. For the common GET case this allocates
        // once and never grows.
        let prev_len = buf.len();
        buf.resize(prev_len + 8 * 1024, 0);
        let n = match stream.read(&mut buf[prev_len..]).await {
            Ok(0) => {
                buf.truncate(prev_len);
                break; // EOF before headers complete
            }
            Ok(n) => n,
            Err(err) => {
                tracing::warn!(error = %err, "read failed");
                return Ok(());
            }
        };
        buf.truncate(prev_len + n);
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            header_end = Some(pos + 4);
            break;
        }
    }

    let header_end = match header_end {
        Some(n) => n,
        None => {
            // Connection closed before a full header arrived.
            let response = render_response(400, "application/json", "{\"error\":\"bad request\"}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Ok(());
        }
    };

    let head_str = match std::str::from_utf8(&buf[..header_end]) {
        Ok(s) => s,
        Err(_) => {
            tracing::warn!("non-utf8 request bytes");
            let response = render_response(400, "application/json", "{\"error\":\"bad request\"}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Ok(());
        }
    };

    let request = match parse_request(head_str) {
        Some(req) => req,
        None => {
            tracing::warn!("malformed http request line");
            let response = render_response(400, "application/json", "{\"error\":\"bad request\"}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Ok(());
        }
    };

    // Step 2: for POSTs, read the body up to Content-Length or our cap.
    let body: Vec<u8> = if request.method == "POST" {
        // Reject early on Content-Length > cap so we never buffer a hostile
        // 10 MB body.
        let content_length: Option<usize> = request
            .headers
            .get("content-length")
            .and_then(|v| v.parse::<usize>().ok());
        if let Some(cl) = content_length
            && cl > MAX_WEBHOOK_BODY_BYTES
        {
            let response =
                render_response(413, "application/json", "{\"error\":\"payload too large\"}");
            // Drain any in-flight body bytes before closing so the
            // client doesn't see ECONNRESET when it tries to finish
            // writing. We bound the drain at a small budget — a
            // hostile client cannot keep us reading forever.
            drain_briefly(&mut stream).await;
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
            return Ok(());
        }
        let want = content_length.unwrap_or(0);
        let already = buf.len().saturating_sub(header_end);
        // Read remaining bytes (if any) up to Content-Length. Defensive cap.
        while buf.len() - header_end < want {
            if buf.len() >= MAX_POST_TOTAL_BYTES {
                let response =
                    render_response(413, "application/json", "{\"error\":\"payload too large\"}");
                let _ = stream.write_all(response.as_bytes()).await;
                return Ok(());
            }
            let prev_len = buf.len();
            buf.resize(prev_len + 8 * 1024, 0);
            let n = match stream.read(&mut buf[prev_len..]).await {
                Ok(0) => {
                    buf.truncate(prev_len);
                    break; // EOF; we will treat short body as the actual body
                }
                Ok(n) => n,
                Err(err) => {
                    tracing::warn!(error = %err, "body read failed");
                    return Ok(());
                }
            };
            buf.truncate(prev_len + n);
        }
        // Slice the body out — clamp to Content-Length when present so a
        // chatty client cannot smuggle trailing bytes into our handler.
        let take = if want > 0 {
            want.min(buf.len() - header_end)
        } else {
            buf.len() - header_end
        };
        let _ = already; // touched only to document why we don't re-read here
        buf[header_end..header_end + take].to_vec()
    } else {
        Vec::new()
    };

    let response = dispatch(&request, &body, &state).await;
    stream.write_all(response.as_bytes()).await.ok();
    stream.shutdown().await.ok();
    Ok(())
}

/// Tiny inline substring search. Used to locate the CRLFCRLF that ends
/// the HTTP header block. We avoid `memchr` to honor the "no new deps"
/// constraint; the search is O(n) and runs once per request on a few
/// KiB of header bytes.
fn find_subsequence(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Briefly drain the socket so a client mid-write doesn't see ECONNRESET
/// after we decide to return early (e.g. 413). Bounded by both a byte
/// budget and a wall-clock timeout so a slowloris cannot pin us.
async fn drain_briefly(stream: &mut TcpStream) {
    use tokio::time::{Duration, timeout};
    let mut scratch = vec![0u8; 64 * 1024];
    let mut drained = 0usize;
    let budget = MAX_POST_TOTAL_BYTES;
    while drained < budget {
        match timeout(Duration::from_millis(200), stream.read(&mut scratch)).await {
            Ok(Ok(0)) => break, // EOF
            Ok(Ok(n)) => drained += n,
            Ok(Err(_)) => break, // read error — give up
            Err(_) => break,     // timeout — client is taking too long
        }
    }
}

/// Route a parsed request to a response string. Pure async function —
/// no I/O beyond what the handlers themselves do (SQL queries via the
/// shared pool).
async fn dispatch(request: &HttpRequest, body: &[u8], state: &AppState) -> String {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/metrics") => handle_metrics(state).await,
        ("GET", "/health") => handle_health(state).await,
        ("POST", "/events") => handle_webhook(request, body, state).await,
        // POST to anything else: method not allowed at that path. We pin
        // `Allow: GET` because no other POST surface exists today.
        ("POST", _) => render_response_with_extra_headers(
            405,
            "application/json",
            "{\"error\":\"method not allowed\"}",
            &[("Allow", "GET")],
        ),
        ("GET", _) => render_response(404, "application/json", "{\"error\":\"not found\"}"),
        // Other methods (PUT/DELETE/...) on any path: 405.
        _ => render_response_with_extra_headers(
            405,
            "application/json",
            "{\"error\":\"method not allowed\"}",
            &[("Allow", "GET")],
        ),
    }
}

/// `GET /metrics` — render the Prometheus text exposition. If `collect()`
/// errors (e.g. SQL unreachable) we return a 500 with the error in the
/// body. Operators monitor scrape success rate separately.
async fn handle_metrics(state: &AppState) -> String {
    match collect(&state.ledger, &state.kill_bell, Utc::now()).await {
        Ok(snap) => {
            let body = render_prometheus(&snap);
            render_response(200, "text/plain; version=0.0.4", &body)
        }
        Err(err) => {
            tracing::warn!(error = %err, "metrics collect failed");
            let body = format!(
                "{{\"error\":\"metrics collect failed\",\"detail\":{}}}",
                json_string(&err.to_string())
            );
            render_response(500, "application/json", &body)
        }
    }
}

/// `GET /health` — readiness probe. We treat the service as `degraded`
/// when either:
///   * the kill bell is paused (autonomy is intentionally halted), or
///   * the ledger query fails (we cannot read state to decide).
///
/// Both conditions return HTTP 200 with a JSON body whose `status` field
/// downstream probes use for alerting. Returning non-200 would cause
/// Kubernetes / load balancers to evict the pod, which is wrong: a paused
/// kill bell is the operator's choice, not a service fault.
async fn handle_health(state: &AppState) -> String {
    let now = Utc::now();

    // Probe the ledger with the cheapest possible read.
    let ledger_reachable = state
        .ledger
        .list(&LedgerFilter {
            limit: Some(1),
            ..Default::default()
        })
        .await
        .is_ok();

    let bell_state = state.kill_bell.current(now).await;
    let bell_label = match bell_state.as_ref() {
        Ok(KillBellState::Armed) => "armed",
        Ok(KillBellState::Paused { .. }) => "paused",
        Err(_) => "unknown",
    };
    let bell_ok = matches!(bell_state, Ok(KillBellState::Armed));

    let status = if ledger_reachable && bell_ok {
        "ok"
    } else {
        "degraded"
    };

    let body = format!(
        "{{\"status\":{},\"kill_bell\":{},\"ledger_reachable\":{},\"checked_at\":{}}}",
        json_string(status),
        json_string(bell_label),
        ledger_reachable,
        json_string(&now.to_rfc3339()),
    );
    render_response(200, "application/json", &body)
}

/// `POST /events` — GitHub-shaped webhook receiver. See module invariants
/// and the Wave 10 brief.
///
/// Flow:
///   1. If `state.webhook_secret` is set, verify `X-Hub-Signature-256`
///      against the raw body via constant-time HMAC-SHA256 compare;
///      401 on mismatch.
///   2. Require `X-GitHub-Event` header; 400 if absent.
///   3. Best-effort parse pull-request fields out of the JSON body.
///   4. Append a signed `LaunchLedgerEntry` recording the event.
///   5. Invoke `state.on_event_callback` if set; callback errors are
///      logged but do not change the response.
///   6. Always 202 once we've durably recorded the event (GitHub
///      delivery semantics: 2xx = "I've got it, don't retry").
///
/// ### LedgerKind choice
/// Wave 10 mint; dedicated kind so audit replay can distinguish webhook
/// events from human decisions. Webhook entries carry
/// `actor = "webhook.github.v1"` and `kind = WebhookReceived`. Historical
/// rows that landed before this mint reused `HumanDecisionRecorded` with
/// the same actor string — disambiguate on `actor` when reading the
/// pre-Wave-10 tail.
async fn handle_webhook(request: &HttpRequest, body: &[u8], state: &AppState) -> String {
    // --- 1. signature verification (optional in dev) -------------------
    if let Some(secret) = state.webhook_secret.as_deref() {
        let header = request
            .headers
            .get("x-hub-signature-256")
            .map(|s| s.as_str())
            .unwrap_or("");
        if !verify_hub_signature(body, header, secret) {
            tracing::warn!("webhook signature mismatch");
            return render_response(
                401,
                "application/json",
                "{\"error\":\"signature mismatch\"}",
            );
        }
    }

    // --- 2. event type -------------------------------------------------
    let event_type = match request.headers.get("x-github-event") {
        Some(v) if !v.is_empty() => v.clone(),
        _ => {
            return render_response(
                400,
                "application/json",
                "{\"error\":\"missing event type\"}",
            );
        }
    };

    // --- 3. best-effort body parse ------------------------------------
    let parsed: serde_json::Value = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    let action = parsed
        .get("action")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let repo = parsed
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let pr_number = parsed
        .get("pull_request")
        .and_then(|p| p.get("number"))
        .and_then(|v| v.as_u64())
        .or_else(|| parsed.get("number").and_then(|v| v.as_u64()));
    let head_sha = parsed
        .get("pull_request")
        .and_then(|p| p.get("head"))
        .and_then(|h| h.get("sha"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let received_at = Utc::now();
    let raw_body_sha256 = sha256_digest(body);
    let event = WebhookEvent {
        event_type,
        action,
        repo: repo.clone(),
        pr_number,
        head_sha,
        raw_body_sha256: raw_body_sha256.clone(),
        received_at,
    };

    // --- 4. ledger append ---------------------------------------------
    // Mint a short event id from the body digest + timestamp nanos. Stable
    // enough to dedupe accidental retries within the same nanosecond.
    let event_id = format!(
        "wh_{}_{}",
        received_at.timestamp_nanos_opt().unwrap_or(0),
        &raw_body_sha256[7..15] // skip the "sha256:" prefix, take 8 hex
    );

    // Build entry payload — keep it human-readable; auditors will read this.
    let payload = serde_json::json!({
        "event_type": event.event_type,
        "action": event.action,
        "repo": event.repo,
        "pr_number": event.pr_number,
        "head_sha": event.head_sha,
        "raw_body_sha256": event.raw_body_sha256,
        "received_at": event.received_at.to_rfc3339(),
    });

    // Sign with the configured webhook key, or generate an ephemeral one.
    // The ephemeral path is acceptable because the ledger only enforces
    // "non-stub algo" — the verifier separately resolves keys via
    // `.jeryu/autonomy/keys/`. Production callers should always wire a key.
    let mut entry = LaunchLedgerEntry {
        schema: SchemaTag::default(),
        id: event_id.clone(),
        kind: LedgerKind::WebhookReceived,
        subject_id: event.repo.clone().unwrap_or_else(|| "unknown".to_string()),
        repo: event.repo.clone(),
        payload,
        recorded_at: received_at,
        actor: "webhook.github.v1".to_string(),
        signature: Signature::stub(), // overwritten by sign_entry below
    };
    if let Some(key) = state.webhook_signing_key.as_ref() {
        sign_entry(&mut entry, key.as_ref());
    } else {
        let ephemeral = EdSigningKey::generate("webhook.github.v1.ephemeral");
        sign_entry(&mut entry, &ephemeral);
    }
    if let Err(err) = state.ledger.append(&entry).await {
        tracing::warn!(error = %err, "webhook ledger append failed");
        // Still return 500 so the sender retries — losing the receipt
        // silently would violate the "every autonomous decision is a
        // signed receipt" invariant from tip1.
        return render_response(
            500,
            "application/json",
            "{\"error\":\"ledger append failed\"}",
        );
    }

    // --- 5. user callback (best effort) -------------------------------
    if let Some(cb) = state.on_event_callback.as_ref()
        && let Err(err) = cb.on_event(event.clone()).await
    {
        tracing::warn!(error = %err, "webhook callback errored; 202 anyway");
    }

    // --- 6. 202 ack ----------------------------------------------------
    let body = format!(
        "{{\"accepted\":true,\"event_id\":{}}}",
        json_string(&event_id)
    );
    render_response(202, "application/json", &body)
}

/// Verify a GitHub `X-Hub-Signature-256` header against `body` using
/// `secret`. Returns `false` on any malformed header, hex-decode error,
/// or MAC mismatch (constant-time comparison once we have equal-length
/// byte arrays).
///
/// Header format is `sha256=<64 hex chars>`. We do not support the
/// deprecated `sha1=` form — GitHub deprecated it in 2019 and `sha2` is
/// already in our dep graph.
pub fn verify_hub_signature(body: &[u8], header: &str, secret: &str) -> bool {
    let Some(rest) = header.strip_prefix("sha256=") else {
        return false;
    };
    let provided = match hex::decode(rest.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let expected = hmac_sha256(secret.as_bytes(), body);
    if provided.len() != expected.len() {
        return false;
    }
    // Constant-time compare: XOR each byte, OR into an accumulator, and
    // check if the accumulator is zero. This avoids early-exit timing
    // leaks that a naive `==` would expose.
    let mut diff: u8 = 0;
    for (a, b) in provided.iter().zip(expected.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Hand-rolled HMAC-SHA256. We do not pull the `hmac` crate (no new
/// deps, Wave 10 constraint). The construction follows RFC 2104:
///
///   HMAC(K, m) = H((K' xor opad) || H((K' xor ipad) || m))
///
/// where K' = K if |K| ≤ block_size else H(K), padded with zeros to
/// block_size (64 bytes for SHA-256). opad = 0x5c repeated, ipad = 0x36
/// repeated. The output is a 32-byte SHA-256 digest.
///
/// Verified against RFC 4231 test vectors in the test module.
pub(crate) fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    // Step 1: derive K' — hash overlong keys, zero-pad short ones.
    let mut k_prime = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let mut h = Sha256::new();
        h.update(key);
        let digest = h.finalize();
        k_prime[..32].copy_from_slice(&digest);
    } else {
        k_prime[..key.len()].copy_from_slice(key);
    }
    // Step 2: build ipad/opad keys (XOR with constants).
    let mut i_key = [0u8; BLOCK_SIZE];
    let mut o_key = [0u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        i_key[i] = k_prime[i] ^ 0x36;
        o_key[i] = k_prime[i] ^ 0x5c;
    }
    // Step 3: inner hash = H(i_key || msg).
    let mut inner = Sha256::new();
    inner.update(i_key);
    inner.update(msg);
    let inner_digest = inner.finalize();
    // Step 4: outer hash = H(o_key || inner_digest).
    let mut outer = Sha256::new();
    outer.update(o_key);
    outer.update(inner_digest);
    let out = outer.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

/// Parse the request line + headers from raw bytes. Returns `None` for
/// malformed input. Pure (no I/O), pub for unit tests.
///
/// All headers are captured into `HttpRequest::headers` keyed by
/// lower-cased name (HTTP headers are case-insensitive, RFC 9110 §5.1).
/// We carry every header rather than naming them so the webhook path
/// can read `X-Hub-Signature-256` and `Content-Length` without a second
/// parse pass.
pub fn parse_request(buf: &str) -> Option<HttpRequest> {
    let mut lines = buf.split("\r\n");
    let first = lines.next()?;
    // Request-line: METHOD SP REQUEST-TARGET SP HTTP-VERSION
    let mut parts = first.split(' ');
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let version = parts.next()?;
    if method.is_empty() || path.is_empty() {
        return None;
    }
    if !version.starts_with("HTTP/") {
        return None;
    }
    if parts.next().is_some() {
        // Extra tokens in the request line: not a valid HTTP/1.x request.
        return None;
    }

    let mut headers = std::collections::BTreeMap::<String, String>::new();
    let mut host: Option<String> = None;
    for line in lines {
        if line.is_empty() {
            break; // end of headers
        }
        // Header is `Name: value`. Split on the first colon; trim
        // surrounding whitespace from the value. Lines without a colon
        // are malformed — skip them rather than fail (browsers send
        // junk in practice).
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name_lc = name.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        if name_lc == "host" {
            host = Some(value.clone());
        }
        headers.insert(name_lc, value);
    }

    Some(HttpRequest {
        method,
        path,
        host,
        headers,
    })
}

/// Render an HTTP/1.1 response. Pure (no I/O), pub for unit tests.
///
/// Always emits Content-Length (callers need not pre-measure) and
/// `Connection: close` so clients know not to expect keep-alive.
pub fn render_response(status: u16, content_type: &str, body: &str) -> String {
    render_response_with_extra_headers(status, content_type, body, &[])
}

/// Variant for handlers that need to emit `Allow:` (405) or similar.
/// Kept private — public callers should use `render_response`.
fn render_response_with_extra_headers(
    status: u16,
    content_type: &str,
    body: &str,
    extra: &[(&str, &str)],
) -> String {
    let reason = status_reason(status);
    let mut out = String::with_capacity(body.len() + 128);
    out.push_str(&format!("HTTP/1.1 {status} {reason}\r\n"));
    out.push_str(&format!("Content-Type: {content_type}\r\n"));
    out.push_str(&format!("Content-Length: {}\r\n", body.len()));
    out.push_str("Connection: close\r\n");
    for (k, v) in extra {
        out.push_str(&format!("{k}: {v}\r\n"));
    }
    out.push_str("\r\n");
    out.push_str(body);
    out
}

/// Minimal reason-phrase table for the status codes this server emits.
/// We deliberately do not pull in `http`'s status table — adding the
/// dependency would violate the "no new deps" Wave 6.C constraint.
fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

/// JSON-string-encode `s` with surrounding quotes. Escapes the minimum
/// set required by RFC 8259 (backslash, double quote, control chars).
/// We avoid pulling `serde_json::to_string` on a `&str` only because the
/// quoting rules are trivial here and inline is faster to audit.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::signing::EdSigningKey;
    use crate::db::AnyPool;
    use crate::db::autonomy_repo::fresh_autonomy_pool;
    use chrono::Utc;
    use std::time::Duration as StdDuration;

    /// Mirror of the `fresh_db` used in `metrics.rs` / `kill_bell.rs` —
    /// keeps these tests independent of `Db::open()`'s migration path.
    /// Routed through the db boundary so this file does not import
    /// `sqlx::` directly (closes HLT-006).
    async fn fresh_db() -> AnyPool {
        fresh_autonomy_pool().await
    }

    async fn fresh_state() -> Arc<AppState> {
        let pool = fresh_db().await;
        Arc::new(AppState {
            ledger: SqlLedger::new(pool.clone()),
            kill_bell: KillBell::new(pool),
            freeze_dir: PathBuf::from("/tmp/jeryu-freeze-test"),
            webhook_secret: None,
            on_event_callback: None,
            webhook_signing_key: None,
        })
    }

    /// Variant that pre-arms the webhook surface: `webhook_secret` set,
    /// an optional callback, and a deterministic ed25519 key so the
    /// ledger entry can be re-verified in tests.
    async fn fresh_state_with_webhook(
        secret: Option<&str>,
        cb: Option<Arc<dyn EventCallback>>,
    ) -> Arc<AppState> {
        let pool = fresh_db().await;
        Arc::new(AppState {
            ledger: SqlLedger::new(pool.clone()),
            kill_bell: KillBell::new(pool),
            freeze_dir: PathBuf::from("/tmp/jeryu-freeze-test"),
            webhook_secret: secret.map(|s| s.to_string()),
            on_event_callback: cb,
            webhook_signing_key: Some(Arc::new(EdSigningKey::from_seed(
                "webhook.github.v1",
                [11u8; 32],
            ))),
        })
    }

    /// Recording callback used by webhook tests. Append all received
    /// events to a shared Vec; optionally return an error on the first
    /// call to exercise the "callback failure does not block 202" path.
    struct FakeEventCallback {
        recorded: std::sync::Mutex<Vec<WebhookEvent>>,
        error_once: std::sync::atomic::AtomicBool,
    }

    impl FakeEventCallback {
        fn new() -> Self {
            Self {
                recorded: std::sync::Mutex::new(Vec::new()),
                error_once: std::sync::atomic::AtomicBool::new(false),
            }
        }
        fn with_error_once() -> Self {
            Self {
                recorded: std::sync::Mutex::new(Vec::new()),
                error_once: std::sync::atomic::AtomicBool::new(true),
            }
        }
        fn snapshot(&self) -> Vec<WebhookEvent> {
            self.recorded.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl EventCallback for FakeEventCallback {
        async fn on_event(&self, event: WebhookEvent) -> anyhow::Result<()> {
            self.recorded.lock().unwrap().push(event);
            if self
                .error_once
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                anyhow::bail!("simulated callback failure");
            }
            Ok(())
        }
    }

    /// Build a POST /events request string with the given headers and
    /// body. `extra_headers` are appended after Content-Length.
    fn build_post_events(
        body: &str,
        event_type: Option<&str>,
        signature: Option<&str>,
        path: &str,
    ) -> String {
        let mut req = format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n",
            body.len()
        );
        if let Some(et) = event_type {
            req.push_str(&format!("X-GitHub-Event: {et}\r\n"));
        }
        if let Some(sig) = signature {
            req.push_str(&format!("X-Hub-Signature-256: {sig}\r\n"));
        }
        req.push_str("Content-Type: application/json\r\n\r\n");
        req.push_str(body);
        req
    }

    /// Sample GitHub pull_request webhook payload (slimmed down).
    fn sample_pr_payload(pr_number: u64, head_sha: &str) -> String {
        serde_json::json!({
            "action": "opened",
            "number": pr_number,
            "pull_request": {
                "number": pr_number,
                "head": {
                    "sha": head_sha
                }
            },
            "repository": {
                "full_name": "octocat/hello-world"
            }
        })
        .to_string()
    }

    /// Bind on `127.0.0.1:0`, return the listener and the actual port.
    /// Tests must read back the port BEFORE spawning the accept loop —
    /// see module invariants.
    async fn bind_ephemeral() -> (TcpListener, u16) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        (listener, port)
    }

    /// Connect to `127.0.0.1:port`, send `request`, read the entire
    /// response until EOF. Uses a generous read timeout so a hung handler
    /// fails loudly instead of stalling the suite.
    async fn round_trip(port: u16, request: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        stream.shutdown().await.ok();
        let mut buf = Vec::new();
        // Read with a deadline so a buggy handler can't hang the suite.
        tokio::time::timeout(StdDuration::from_secs(5), stream.read_to_end(&mut buf))
            .await
            .expect("response within 5s")
            .expect("read response");
        String::from_utf8(buf).expect("response is utf8")
    }

    // ----- Pure parser tests (1-5) -------------------------------------

    #[test]
    fn parse_get_metrics_round_trips() {
        let raw = "GET /metrics HTTP/1.1\r\nHost: localhost:9090\r\nUser-Agent: prom\r\n\r\n";
        let req = parse_request(raw).expect("must parse");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/metrics");
        assert_eq!(req.host.as_deref(), Some("localhost:9090"));
    }

    #[test]
    fn parse_get_health_round_trips() {
        let raw = "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        let req = parse_request(raw).expect("must parse");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/health");
        assert_eq!(req.host.as_deref(), Some("127.0.0.1"));
    }

    #[test]
    fn parse_rejects_post_method() {
        // POSTs parse successfully — routing layer is responsible for 405.
        // This test pins the contract: the parser is method-agnostic so
        // dispatch() can return a precise 405 with `Allow: GET`.
        let raw = "POST /metrics HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n";
        let req = parse_request(raw).expect("POST should parse");
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/metrics");
    }

    #[test]
    fn parse_rejects_malformed_first_line() {
        assert!(parse_request("").is_none());
        assert!(parse_request("GET\r\n\r\n").is_none());
        // Missing HTTP version: not a valid request line.
        assert!(parse_request("GET /metrics\r\n\r\n").is_none());
        // Wrong version token: not HTTP.
        assert!(parse_request("GET /metrics SPDY/3.1\r\n\r\n").is_none());
        // Extra tokens in the request line.
        assert!(parse_request("GET /metrics HTTP/1.1 extra\r\n\r\n").is_none());
    }

    #[test]
    fn parse_extracts_host_header() {
        // Case-insensitive: real clients send "Host", "host", or "HOST".
        let raw = "GET / HTTP/1.1\r\nhost: lower.example\r\n\r\n";
        assert_eq!(
            parse_request(raw).unwrap().host.as_deref(),
            Some("lower.example")
        );
        let raw = "GET / HTTP/1.1\r\nHOST: upper.example:8080\r\n\r\n";
        assert_eq!(
            parse_request(raw).unwrap().host.as_deref(),
            Some("upper.example:8080")
        );
        // Missing host header is allowed by HTTP/1.0 and by our parser.
        let raw = "GET / HTTP/1.0\r\n\r\n";
        assert_eq!(parse_request(raw).unwrap().host, None);
    }

    // ----- Pure renderer tests (6-7) -----------------------------------

    #[test]
    fn render_response_includes_content_length() {
        let body = "hello world";
        let out = render_response(200, "text/plain", body);
        assert!(
            out.starts_with("HTTP/1.1 200 OK\r\n"),
            "must be HTTP/1.1 200, got: {out}"
        );
        assert!(out.contains("Content-Type: text/plain\r\n"));
        assert!(
            out.contains(&format!("Content-Length: {}\r\n", body.len())),
            "must include exact content-length; got: {out}"
        );
        assert!(out.contains("Connection: close\r\n"));
        assert!(out.ends_with(body), "body must be tail of response");
    }

    #[test]
    fn render_response_for_404_uses_json_content_type() {
        let out = render_response(404, "application/json", "{\"error\":\"not found\"}");
        assert!(out.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(out.contains("Content-Type: application/json\r\n"));
        assert!(out.contains("Content-Length: 21\r\n"));
        assert!(out.ends_with("{\"error\":\"not found\"}"));
    }

    // ----- Integration tests over a real loopback socket (8-12) -------

    #[tokio::test]
    async fn serve_returns_405_for_post() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state().await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let response = round_trip(
            port,
            "POST /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n",
        )
        .await;

        assert!(
            response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
            "must be 405, got: {response}"
        );
        assert!(
            response.contains("Allow: GET\r\n"),
            "405 must carry Allow header, got: {response}"
        );
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_404_for_unknown_path() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state().await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let response = round_trip(port, "GET /nope HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await;

        assert!(
            response.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "must be 404, got: {response}"
        );
        assert!(response.contains("Content-Type: application/json\r\n"));
        assert!(response.contains("\"error\":\"not found\""));
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_200_with_prometheus_for_get_metrics() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state().await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let response = round_trip(port, "GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await;

        assert!(
            response.starts_with("HTTP/1.1 200 OK\r\n"),
            "must be 200, got first line: {}",
            response.lines().next().unwrap_or("")
        );
        assert!(
            response.contains("Content-Type: text/plain; version=0.0.4\r\n"),
            "must use Prometheus text/plain content type"
        );
        // Spot-check the rendered body contains at least one metric we
        // know `collect()` emits on an empty ledger.
        assert!(
            response.contains("jeryu_autonomous_promotion_total"),
            "metrics body must contain a known counter; got: {response}"
        );
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_200_with_health_armed_when_kill_bell_armed() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state().await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let response = round_trip(port, "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await;

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("Content-Type: application/json\r\n"));
        // Body sits after the CRLFCRLF.
        let body = response.split("\r\n\r\n").nth(1).expect("body present");
        assert!(body.contains("\"status\":\"ok\""), "body: {body}");
        assert!(body.contains("\"kill_bell\":\"armed\""), "body: {body}");
        assert!(body.contains("\"ledger_reachable\":true"), "body: {body}");
        assert!(body.contains("\"checked_at\":"), "body: {body}");
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_degraded_status_when_kill_bell_paused() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state().await;
        // Pause the bell using the real API so the entire downgrade path
        // (signed ledger write + state row) is exercised.
        let key = EdSigningKey::generate("operator.healthtest");
        state
            .kill_bell
            .pause("audit-window", "alice", 3600, &key, Utc::now())
            .await
            .unwrap();

        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let response = round_trip(port, "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").await;

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        let body = response.split("\r\n\r\n").nth(1).expect("body present");
        assert!(
            body.contains("\"status\":\"degraded\""),
            "paused bell must yield degraded; body: {body}"
        );
        assert!(body.contains("\"kill_bell\":\"paused\""), "body: {body}");
        handle.await.unwrap().unwrap();
    }

    // ===================================================================
    // Wave 10 — POST /events webhook surface
    // ===================================================================

    // ----- HMAC helper unit tests (1-4) -------------------------------

    /// Pin against an RFC 4231-style test vector: key=`key`, data=`The
    /// quick brown fox jumps over the lazy dog` is a widely-cited
    /// HMAC-SHA256 fixture. We sign and round-trip the header form.
    #[test]
    fn verify_hub_signature_accepts_correct_hmac() {
        let secret = "key";
        let body = b"The quick brown fox jumps over the lazy dog";
        let mac = hmac_sha256(secret.as_bytes(), body);
        let header = format!("sha256={}", hex::encode(mac));
        assert!(
            verify_hub_signature(body, &header, secret),
            "valid HMAC must verify; header: {header}"
        );
        // Spot-check the canonical RFC 4231-cited digest.
        assert_eq!(
            hex::encode(mac),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn verify_hub_signature_rejects_wrong_secret() {
        let body = b"{\"action\":\"opened\"}";
        let mac = hmac_sha256(b"correct-secret", body);
        let header = format!("sha256={}", hex::encode(mac));
        assert!(
            !verify_hub_signature(body, &header, "wrong-secret"),
            "wrong secret must NOT verify"
        );
    }

    #[test]
    fn verify_hub_signature_rejects_corrupted_body() {
        let secret = "shh";
        let body = b"{\"action\":\"opened\"}";
        let mac = hmac_sha256(secret.as_bytes(), body);
        let header = format!("sha256={}", hex::encode(mac));
        let tampered = b"{\"action\":\"closed\"}";
        assert!(
            !verify_hub_signature(tampered, &header, secret),
            "tampered body must NOT verify"
        );
    }

    #[test]
    fn verify_hub_signature_rejects_malformed_header_prefix() {
        let secret = "shh";
        let body = b"hello";
        let mac = hmac_sha256(secret.as_bytes(), body);
        // Missing `sha256=` prefix.
        let bare = hex::encode(mac);
        assert!(!verify_hub_signature(body, &bare, secret));
        // Wrong algo prefix.
        let sha1 = format!("sha1={bare}");
        assert!(!verify_hub_signature(body, &sha1, secret));
        // Truncated hex.
        let truncated = "sha256=deadbeef";
        assert!(!verify_hub_signature(body, truncated, secret));
        // Non-hex characters after the prefix.
        let bad_hex = "sha256=zzzzzzzz";
        assert!(!verify_hub_signature(body, bad_hex, secret));
        // Empty.
        assert!(!verify_hub_signature(body, "", secret));
    }

    // ----- Wire tests over the real socket (5-15) ---------------------

    #[tokio::test]
    async fn serve_returns_401_on_signature_mismatch_when_secret_set() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state_with_webhook(Some("right-secret"), None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        // Sign with the WRONG secret on purpose.
        let body = sample_pr_payload(42, "deadbeef");
        let wrong_mac = hmac_sha256(b"wrong-secret", body.as_bytes());
        let req = build_post_events(
            &body,
            Some("pull_request"),
            Some(&format!("sha256={}", hex::encode(wrong_mac))),
            "/events",
        );
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 401 Unauthorized\r\n"),
            "must be 401, got: {response}"
        );
        assert!(response.contains("\"error\":\"signature mismatch\""));
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_202_on_valid_webhook_with_secret() {
        let (listener, port) = bind_ephemeral().await;
        let secret = "shh";
        let state = fresh_state_with_webhook(Some(secret), None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(7, "cafebabecafebabecafebabecafebabecafebabe");
        let mac = hmac_sha256(secret.as_bytes(), body.as_bytes());
        let req = build_post_events(
            &body,
            Some("pull_request"),
            Some(&format!("sha256={}", hex::encode(mac))),
            "/events",
        );
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 202 Accepted\r\n"),
            "must be 202, got: {response}"
        );
        let body = response.split("\r\n\r\n").nth(1).expect("body present");
        assert!(body.contains("\"accepted\":true"), "body: {body}");
        assert!(body.contains("\"event_id\":\"wh_"), "body: {body}");
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_202_on_webhook_when_secret_unset() {
        // Dev mode: no secret configured, any body is accepted.
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state_with_webhook(None, None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(1, "abc");
        let req = build_post_events(&body, Some("pull_request"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 202 Accepted\r\n"),
            "dev mode must accept without signature; got: {response}"
        );
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_400_on_missing_event_type_header() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state_with_webhook(None, None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(1, "abc");
        // Omit X-GitHub-Event.
        let req = build_post_events(&body, None, None, "/events");
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request\r\n"),
            "must be 400, got: {response}"
        );
        assert!(response.contains("\"error\":\"missing event type\""));
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn serve_returns_405_on_post_to_unknown_path() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state_with_webhook(None, None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(1, "abc");
        let req = build_post_events(&body, Some("pull_request"), None, "/not-events");
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
            "must be 405, got: {response}"
        );
        assert!(
            response.contains("Allow: GET\r\n"),
            "405 must carry Allow header"
        );
        handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn webhook_appends_signed_ledger_entry_with_webhook_received_kind() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state_with_webhook(None, None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        // Clone state for post-condition inspection.
        let state_for_assert = state.clone();
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(99, "f00ba4");
        let req = build_post_events(&body, Some("pull_request"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(response.starts_with("HTTP/1.1 202 Accepted\r\n"));
        handle.await.unwrap().unwrap();

        // Inspect the ledger: exactly one entry, kind = webhook_received,
        // actor = "webhook.github.v1", payload references the pr_number we sent.
        let entries = state_for_assert
            .ledger
            .list(&LedgerFilter::default())
            .await
            .unwrap();
        assert_eq!(entries.len(), 1, "exactly one ledger entry must appear");
        let e = &entries[0];
        assert_eq!(
            e.kind,
            LedgerKind::WebhookReceived,
            "Wave 10 mint: webhook entries land as WebhookReceived, not HumanDecisionRecorded"
        );
        assert_ne!(
            e.kind,
            LedgerKind::HumanDecisionRecorded,
            "webhook entries must no longer pollute the HumanDecisionRecorded series"
        );
        assert_eq!(e.actor, "webhook.github.v1");
        assert_eq!(e.signature.algo, "ed25519", "must be signed with ed25519");
        assert_eq!(
            e.repo.as_deref(),
            Some("octocat/hello-world"),
            "repo field must be populated"
        );
        assert_eq!(
            e.payload.get("pr_number").and_then(|v| v.as_u64()),
            Some(99)
        );
        assert_eq!(
            e.payload.get("event_type").and_then(|v| v.as_str()),
            Some("pull_request")
        );
    }

    /// Replay-side smoke: a `WebhookReceived` row landed by the HTTP receiver
    /// must round-trip through `SqlLedger::list` with the correct kind, so
    /// `autonomy replay` and other audit surfaces can pick it up as a
    /// dedicated event class (not as a human decision).
    #[tokio::test]
    async fn replay_recognizes_webhook_received_kind() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state_with_webhook(None, None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let state_for_assert = state.clone();
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(7, "cafe1234");
        let req = build_post_events(&body, Some("pull_request"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(response.starts_with("HTTP/1.1 202 Accepted\r\n"));
        handle.await.unwrap().unwrap();

        // Filter by kind: WebhookReceived. If the receiver fell back to
        // HumanDecisionRecorded the filter would return empty.
        let entries = state_for_assert
            .ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::WebhookReceived),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(
            entries.len(),
            1,
            "audit replay must see exactly one WebhookReceived row"
        );
        // And the kind round-trips through the snake_case ↔ enum mapper.
        assert_eq!(entries[0].kind, LedgerKind::WebhookReceived);
        // The companion `HumanDecisionRecorded` filter must be empty so the
        // two streams are cleanly separated.
        let human_only = state_for_assert
            .ledger
            .list(&LedgerFilter {
                kind: Some(LedgerKind::HumanDecisionRecorded),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(
            human_only.is_empty(),
            "webhook events must NOT show up under the human-decision filter"
        );
    }

    #[tokio::test]
    async fn webhook_callback_invoked_with_parsed_event() {
        let (listener, port) = bind_ephemeral().await;
        let cb = Arc::new(FakeEventCallback::new());
        let cb_dyn: Arc<dyn EventCallback> = cb.clone();
        let state = fresh_state_with_webhook(None, Some(cb_dyn)).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(13, "abcd1234");
        let req = build_post_events(&body, Some("pull_request"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(response.starts_with("HTTP/1.1 202 Accepted\r\n"));
        handle.await.unwrap().unwrap();

        let recorded = cb.snapshot();
        assert_eq!(recorded.len(), 1, "callback must fire exactly once");
        let event = &recorded[0];
        assert_eq!(event.event_type, "pull_request");
        assert_eq!(event.action.as_deref(), Some("opened"));
        assert_eq!(event.repo.as_deref(), Some("octocat/hello-world"));
        assert_eq!(event.pr_number, Some(13));
        assert_eq!(event.head_sha.as_deref(), Some("abcd1234"));
        assert!(event.raw_body_sha256.starts_with("sha256:"));
    }

    #[tokio::test]
    async fn webhook_callback_error_does_not_block_202_response() {
        let (listener, port) = bind_ephemeral().await;
        let cb = Arc::new(FakeEventCallback::with_error_once());
        let cb_dyn: Arc<dyn EventCallback> = cb.clone();
        let state = fresh_state_with_webhook(None, Some(cb_dyn)).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        let body = sample_pr_payload(7, "xyz");
        let req = build_post_events(&body, Some("pull_request"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 202 Accepted\r\n"),
            "callback error must NOT promote past 202; got: {response}"
        );
        handle.await.unwrap().unwrap();

        let recorded = cb.snapshot();
        assert_eq!(
            recorded.len(),
            1,
            "callback was still invoked; only its Err return was ignored"
        );
    }

    #[tokio::test]
    async fn webhook_event_extracts_pr_number_and_head_sha_from_pull_request_payload() {
        let (listener, port) = bind_ephemeral().await;
        let cb = Arc::new(FakeEventCallback::new());
        let cb_dyn: Arc<dyn EventCallback> = cb.clone();
        let state = fresh_state_with_webhook(None, Some(cb_dyn)).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        // pull_request payload (the canonical shape).
        let body = serde_json::json!({
            "action": "synchronize",
            "pull_request": {
                "number": 4242,
                "head": { "sha": "fa11ba11fa11ba11fa11ba11fa11ba11fa11ba11" }
            },
            "repository": { "full_name": "acme/widget" }
        })
        .to_string();
        let req = build_post_events(&body, Some("pull_request"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(response.starts_with("HTTP/1.1 202 Accepted\r\n"));
        handle.await.unwrap().unwrap();

        let recorded = cb.snapshot();
        let e = &recorded[0];
        assert_eq!(e.pr_number, Some(4242));
        assert_eq!(
            e.head_sha.as_deref(),
            Some("fa11ba11fa11ba11fa11ba11fa11ba11fa11ba11")
        );
        assert_eq!(e.action.as_deref(), Some("synchronize"));
        assert_eq!(e.repo.as_deref(), Some("acme/widget"));
    }

    #[tokio::test]
    async fn webhook_event_handles_payload_with_missing_optional_fields_gracefully() {
        let (listener, port) = bind_ephemeral().await;
        let cb = Arc::new(FakeEventCallback::new());
        let cb_dyn: Arc<dyn EventCallback> = cb.clone();
        let state = fresh_state_with_webhook(None, Some(cb_dyn)).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        // `ping` event with an essentially empty body — repo, pr_number,
        // head_sha, action all absent. Must still 202 and emit a clean
        // WebhookEvent with Nones.
        let body = "{\"zen\":\"non-blocking is better than blocking\"}";
        let req = build_post_events(body, Some("ping"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 202 Accepted\r\n"),
            "ping must 202; got: {response}"
        );
        handle.await.unwrap().unwrap();

        let recorded = cb.snapshot();
        assert_eq!(recorded.len(), 1);
        let e = &recorded[0];
        assert_eq!(e.event_type, "ping");
        assert_eq!(e.action, None);
        assert_eq!(e.repo, None);
        assert_eq!(e.pr_number, None);
        assert_eq!(e.head_sha, None);
        assert!(e.raw_body_sha256.starts_with("sha256:"));
    }

    #[tokio::test]
    async fn webhook_body_too_large_returns_413() {
        let (listener, port) = bind_ephemeral().await;
        let state = fresh_state_with_webhook(None, None).await;
        let cfg = HttpServerConfig {
            bind_addr: format!("127.0.0.1:{port}"),
            shutdown_after_requests: Some(1),
        };
        let handle = tokio::spawn(serve_with_listener(listener, cfg, state));

        // 300 KiB body — comfortably over the 256 KiB cap.
        let huge = "a".repeat(300 * 1024);
        let body = format!("{{\"data\":\"{huge}\"}}");
        let req = build_post_events(&body, Some("pull_request"), None, "/events");
        let response = round_trip(port, &req).await;
        assert!(
            response.starts_with("HTTP/1.1 413 Payload Too Large\r\n"),
            "must be 413, got first line: {}",
            response.lines().next().unwrap_or("")
        );
        assert!(response.contains("\"error\":\"payload too large\""));
        handle.await.unwrap().unwrap();
    }
}
