# Maintainer audit receiver

`jeryu-split audit-service` runs a separate HTTP receiver for the existing private
audit ledger. Building, installing and serving Jeryu do not start or require it.
This receiver does not yet dispatch audits, reconcile missed GitHub deliveries,
authenticate completed executions or publish evidence. Deployment and enrollment
remain pending until those boundaries and the recovery drills are qualified.

Build the maintenance executable with `cargo build --locked -p jeryu-split-tool`.
Run it under a dedicated unprivileged account, with its data outside source:

```sh
jeryu-split audit-service \
  --database /var/lib/jeryu-audit/ledger.sqlite \
  --route /etc/jeryu-audit/jeryu.json \
  --listen 127.0.0.1:8789
```

Both parent directories must already be physical directories owned by that
account with mode 0700. Route and key files require mode 0600 and one hard link.
The receiver creates its database with mode 0600. It refuses a replaced database
name before acknowledging a reception. Keep the account separate from executor
and publisher identities; same-account hostile filesystem mutation is outside
this storage boundary.

Each `--route` file uses the existing closed `jeryu.audit-intake-route/v1` schema:

| Fields | Meaning |
| --- | --- |
| `schema_version`, `route_id` | Schema and unique endpoint identifier |
| `repository_id`, `repository` | GitHub numeric repository identity and `neverhuman/name` slug |
| `secret_file`, `secret_version` | Absolute private key path and operator rotation identifier |
| `receiver_source_commit`, `receiver_executable_sha256` | Configured receiver observations |
| `governing_workflow_repository`, `governing_workflow_path`, `governing_workflow_commit`, `governing_workflow_blob` | Configured governing workflow observations |
| `executor_source_commit`, `executor_executable_sha256`, `executor_receipt_sha256` | Configured executor observations |
| `governing_policy_sha256`, `execution_config_sha256` | Configured policy and execution input hashes |

All commit/blob IDs must be full nonzero lowercase SHA-1 values; content hashes
must be full nonzero lowercase SHA-256 values. These configured observations
cannot authenticate themselves or grant execution/publication approval. Do not
invent qualifying identities merely to start the receiver.

Configure the reverse proxy to preserve raw request bodies and the GitHub
signature, event and delivery headers. Only loopback binding is accepted here;
remote HTTPS termination and access to `/hooks/<route_id>` belong to the
maintainer deployment. `/health` reports listener availability, never audit or
database qualification. Configure GitHub to send JSON payloads with the matching
webhook key. HMAC-SHA256 covers the exact body bytes; event and delivery headers
remain unsigned metadata, following GitHub's
[validation contract](https://docs.github.com/en/webhooks/using-webhooks/validating-webhook-deliveries).

Route bytes and keys are held at startup. Key rotation requires a new key version,
updated route configuration, receiver restart and matching GitHub configuration.
Editing a key file does not silently change the running receiver's identity.

The receiver bounds retained bodies to 25 MiB, selected header metadata to 4 KiB,
body reads to ten seconds and concurrent receptions to eight. Missing or
duplicate signatures cannot be treated as one authenticated header. Rejected
bounded requests retain their raw bytes privately. An incomplete/oversized body
retains the reception metadata with unavailable body status; it cannot be
authenticated or scheduled. Requests refused before body reception require
delivery reconciliation. Storage failures and saturation never return success.

HTTP 202 means the raw reception and classification have committed durably. It
does not mean an audit ran or passed. Every response keeps execution and
publication admission false. Retries preserve every reception while sharing the
same signed event identity. On restart, the receiver finishes classifications
interrupted after their raw bytes committed. TERM/INT stop accepting requests and
drain in-flight transactions; incomplete work remains available for replay after
a crash. No failed evidence or runtime state is automatically deleted.

Run the transport and restart regressions with:

```sh
cargo test --locked --offline -p jeryu-split-tool audit_intake::tests::service_http
```

These tests use real loopback HTTP, retain their synthetic private databases,
and cover exact-byte storage, retries across restart, interrupted classification,
signature/body mismatches, duplicate headers, replaced databases, key lifetime
and unsafe configuration. They do not establish remote TLS, durable executor
dispatch, outage backfill, publication or deployed enrollment.
