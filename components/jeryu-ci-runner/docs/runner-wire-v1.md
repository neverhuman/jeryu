# Jeryu runner wire v1

`jeryu-runner-protocol::wire` defines the endpoint-neutral JSON contract for a future
AtomicSoul-native runner. It is a pure protocol leaf: this repository does not yet contain an
AtomicSoul HTTP route, network client, credential loader, durable runner registry, service unit,
or live xbabe3 registration.

The checked draft 2020-12 structural mirror is
[`schemas/jeryu.runner.v1.schema.json`](../schemas/jeryu.runner.v1.schema.json),
with consumer guidance in [`contracts/README.md`](../contracts/README.md). Rust
serde plus `ValidateWire` remain the runtime authority, including cross-field,
reserved environment-name, and byte-length checks that are intentionally not
delegated to a generic schema validator.

## Contract

Every top-level message carries exact `protocol_version = "jeryu.runner.v1"` and one fixed
`message_type`. The four exchanges are:

| Request | Acknowledgement | Binding |
| --- | --- | --- |
| `RegisterRequest` | `RegisterAck` | request ID and runner ID; the acknowledgement issues a nonzero fencing epoch |
| `HeartbeatRequest` | `HeartbeatAck` | request ID, runner ID/epoch, and the complete active lease context when present |
| `LeaseRequest` | `LeaseAck` | request ID and runner ID/epoch; an assigned grant binds the full job digest and immutable execution provenance |
| `ResultRequest` | `ResultAck` | the complete lease context, full job digest, execution provenance, and deterministic receipt ID |

The wire job and result types convert to and from the existing pure `JobRequest` and `JobResult`.
Registration and heartbeat likewise convert to and from `RunnerHello` and `Heartbeat`. These
conversions do not perform I/O.

All structs use closed serde schemas. Decode rejects unknown fields, malformed values, a wrong
version/type, bodies larger than 1 MiB, zero epochs, malformed or overlong IDs, noncanonical
labels, capacities outside 1–1024, out-of-range Unix-millisecond timestamps, invalid lease/result
time order, and oversized or duplicate body collections. Steps, environment entries, cache
mounts, artifact declarations, result digests, and cache receipts have explicit count and size
bounds.

Repository identity preserves the hosted forge's exact case. Owners remain canonical lowercase;
repository components use the same closed ASCII punctuation rules while admitting case-sensitive
names such as `jeryu/redlineDB`. An assigned lease acknowledgement is invalid when the server's
own timestamp falls outside the half-open grant interval, and a result cannot be submitted before
its recorded finish time.

`WireJobRequest::execution_digest` uses an explicit length-framed SHA-256 encoding of every job
field: all request and fencing IDs, runner class, ordered steps (including commands, reusable
actions, environment, and working directories), ordered cache declarations (including mode),
ordered artifact declarations (including collection timing and retention), job environment, and
timeout. An assigned `ExecutionContext` also carries the protected-policy Git SHA, toolchain
digest, runner-class policy identity and digest, and immutable image and rootfs digests. The lease
fails validation if its job no longer matches the bound full-job digest.

Working directories, cache paths, and artifact paths must be relative forward-slash paths. Empty,
absolute, Windows-drive, backslash, NUL, repeated-separator, `.` component, and `..` component
forms are rejected. This lexical check is not a symlink defense: the execution adapter must open
and resolve paths beneath its workspace using descriptor-relative, no-follow semantics and reject
symlink escapes or replacement races before execution, cache access, or artifact collection.

Built-in runner classes use only their exact kebab-case names. Custom classes use
`custom:<lowercase-token>`; aliases such as `docker` and noncanonical case are rejected. This
prevents a JSON round trip from changing `RunnerClass::Custom` into a built-in class.

## Authentication boundary

Authentication is not part of the JSON schema. A future HTTP adapter must obtain its runner
credential from a protected HTTP Authorization header supplied by systemd credential custody,
authenticate before decoding a mutation, and isolate that credential from every JSON payload,
child environment, receipt, diagnostic, and log.

Commands and ordinary environment values are opaque application data. This schema cannot decide
whether an arbitrary value is secret, so encoded job bodies must be treated as sensitive and must
not be logged. Case-insensitive auth, credential, token, secret, actor/spoof, askpass, SSH-auth,
Git-config, and runner-control environment name classes are rejected as a defense in depth; this
guardrail is not secret classification. Every public payload-bearing type has a redacted `Debug`,
and wire errors retain only a static field name plus error category.

The fencing epoch is not a credential. It prevents a superseded runner incarnation from
heartbeating or submitting results, but it does not authenticate the caller.

## Result idempotency

`ResultRequest::new` derives a SHA-256 receipt identity over the protocol version, full
runner/epoch/run/lease/job/repository/head/check context, full-job digest, protected policy,
toolchain, runner-class policy, image/rootfs provenance, outcome, exit code, timestamps, ordered
artifact digests, ordered cache receipts, and log digest. Repeating an identical submission yields
the same ID. Changing any bound context produces a different ID. A result or acknowledgement with
a stale context or mismatched receipt fails before it can be handed to scheduler mutation logic.

## Work still required before xbabe3 can register

1. Jeryu Core must add authenticated, durable runner enrollment, epoch fencing, heartbeat,
   lease, and idempotent result application around these messages, with repository/check
   authorization and audit receipts.
2. Jeryu Deploy must expose the reviewed Core operations through bounded HTTPS routes, preserve
   Authorization-header custody, reject redirects and ambiguous mutation retries, and enforce
   read-only-mirror mode.
3. `jeryu-runnerd` needs a separate reviewed client tranche for TLS-pinned calls, bounded polling,
   drain/fence behavior, local credential-descriptor handling, and crash-safe result replay.
4. A signed immutable release must be deployed to AtomicSoul and independently verified before
   an xbabe3 service unit or registration credential is created.
5. xbabe3 registration, capacity allocation, cgroup limits, end-to-end lease execution, required
   check publication, reboot/linger behavior, credential renewal, and revocation tests remain host
   operations. The retired GitHub Actions units must remain disabled and must not be reused.

Until those steps land through protected releases, this module is a tested contract only and must
not be reported as a running or registered AtomicSoul runner.
