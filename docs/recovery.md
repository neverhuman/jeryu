# Standalone SQLite operations and recovery

This procedure concerns the candidate `jeryu serve` process on Linux x86_64.
It does not change an installed service or qualify an upgrade. The release
admission requirements remain in [release status](release.md). Use the
[README](../README.md) for first installation and login.

## Data and permissions

Resolve the actual data directory before maintenance: `--data-dir` takes
precedence over `JERYU_DATA_DIR`, then XDG storage. Record its absolute physical
path, device/inode, owner, permissions and mounts. Run Jeryu as one dedicated
unprivileged account. For a **new** directory, create it as that account with
`umask 077` and mode `0700`; keep archives and credential receipts private.
Do not apply recursive ownership or permission changes to existing state
without inspecting its paths, links and active consumers first.

The complete directory is the backup unit. It includes `forge.sqlite`
(accounts, sessions, tokens, repository metadata, issues, reviews and checks),
`work.sqlite`, `codegraph.sqlite`, any SQLite journals/WAL/SHM sidecars, and
`git/` (managed repositories, hooks, LFS objects and creation receipts).
Do not select only `*.sqlite` or omit hidden entries. Record separately any
configuration, proxy certificates, optional external stores or credentials
outside this directory, and the exact executable/source/build receipt needed
to reopen the snapshot. Keep this record private; do not copy credentials to
an issue or public audit artifact.

Use storage with working SQLite locking, atomic rename and sufficient space
for a full rehearsal. Read-only storage, permission failures and disk
exhaustion must stop maintenance. Active agent runs and workcell leases are
not durable backup contents; finish or stop those consumers separately.

## Stopped-server backup and restore rehearsal

There is no online whole-application snapshot command. Copying live database
and Git files can capture different points in time. Use a maintenance window:

1. Stop accepting writes and stop every process that uses this data directory,
   including Git/runner consumers. Stop through the process's actual supervisor
   and confirm the PID has exited and been reaped; a closed HTTP port alone
   does not prove that all writers stopped. Inspect open handles and mounts.
2. Record the executable checksum, source revision, launch settings and data
   directory identity. Preserve logs privately. Keep the stopped original and
   previous executable throughout the rehearsal.
3. As the owning account, archive the **whole** stopped data directory to a new
   owner-only location outside it. Include SQLite sidecars and hidden Git
   state. Fail if archive creation reports an error or the source changes.
4. Record and verify the archive checksum. Restore only a trusted, verified
   archive into a newly created empty owner-only runtime directory. Validate
   archive paths, file types, links and mount boundaries before extraction;
   never restore over existing data or into a source checkout. Compare the
   restored file inventory, contents and permissions with the stopped source.
5. Start the same verified binary against the restored directory on an unused
   loopback port. Explicitly choose that directory with `--data-dir`; do not
   supply a new bootstrap password or share the directory with another server.
6. Verify saved account login, session/PAT access, anonymous and private access
   rules, repository identities and Git refs/content, issues, Work items and
   comments, and applicable reviews/checks. Make one controlled write, restart
   the restored instance, and read it back. A healthy `/health` alone is not a
   restore proof. Record failures without printing stored secrets.
7. Stop and reap the rehearsal process. Preserve the archive, inventory and
   private evidence until the retention owner approves retirement. Reopen the
   original only after confirming its identity and that it remained unchanged.

For the archiving step, GNU tar can preserve a stopped local tree. These
commands assume the directories have already passed the checks above and
that `JERYU_BACKUP` is a **new, empty, owner-only** backup directory outside
`JERYU_DATA`. They deliberately do not stop a service or delete any state:

```bash
(
  set -euC
  umask 077
  tar --create --file=- --directory="$JERYU_DATA" . > "$JERYU_BACKUP/data.tar"
  cd "$JERYU_BACKUP"
  sha256sum data.tar > data.tar.sha256
  sha256sum --check data.tar.sha256
)
```

Continue only if every command succeeds. A checksum detects changed bytes;
it does not authenticate an archive received from somebody else or prove a
consistent live backup. The procedure requires stopped writers and a restore
rehearsal. For the trusted archive above, after reviewing its entries and
creating a new empty `JERYU_RESTORE` directory with mode `0700`, extract as the
owning unprivileged account:

```bash
(
  set -eu
  cd "$JERYU_BACKUP"
  sha256sum --check data.tar.sha256
  tar --extract --file=data.tar --directory="$JERYU_RESTORE" \
    --same-permissions --no-same-owner
)
```

Only after extraction and the file comparison succeed, start the rehearsal:

```bash
jeryu serve --bind 127.0.0.1:9318 --data-dir "$JERYU_RESTORE"
```

Use the recorded executable explicitly if `jeryu` on `PATH` has changed.
Keep the rehearsal port private and stop it before any activation decision.
Archives include authentication state: restoring one can reinstate credentials
or sessions revoked after its creation. Review post-snapshot revocations and
apply them to the restored instance before exposing it.

## Upgrade and rollback

Build and verify the proposed binary separately. Preserve the running binary,
its source/build receipt, launch settings and a rehearsed stopped backup.
Before activation, repeat the restore exercise with the proposed binary on a
new restored runtime directory. Check schema/opening errors, expected data,
permissions and the complete advertised workflow. Review release-specific
migration notes; no cross-version downgrade compatibility is promised.

Activation is a separate approved maintenance action. Stop all writers before
changing the selected executable or data directory. Record exactly which
binary and data snapshot become active, then repeat authenticated read/write,
Git, Work and restart checks. Do not resume traffic on a partial result.

If the new version has accepted writes, preserve that complete stopped state
before rollback. Running an older binary against a newer schema or restoring
an older snapshot can lose accepted work. Establish a reviewed migration or
write-reconciliation plan first. A safe rehearsal rollback pairs the previous
binary with its matching pre-upgrade snapshot in another new runtime directory;
it does not erase the failed candidate or move an immutable release tag.

## Interrupted repository creation

The browser creation API records the authenticated actor, request digest and
`Idempotency-Key` before allocating the database record and Git storage.
A completed replay with the same actor, key and request returns the existing
repository. A different request with the same key returns
`idempotency_conflict`; an unfinished or unreadable receipt returns
`creation_incomplete`; a missing completed repository returns
`repository_changed`. Creation can stop between database and Git operations.

For `creation_failed`, `creation_incomplete` or inconsistent readback, stop
retries and preserve the stopped data directory, private logs, original actor,
key and request. Compare the repository API record, managed Git directory,
refs and receipt against that evidence without modifying them. Do not delete
the receipt, retry under a new key, edit SQLite rows, or adopt an orphan Git
directory to bypass the failure. There is currently no admitted repair CLI.
Restore a verified consistent snapshot into a new directory, or obtain a
reviewed owning repair that preserves all subsequent accepted writes. The
creation tests cover fail-closed replay and orphan refusal; they do not prove
an automated interrupted-creation repair procedure.

## Remote binding and TLS

The default loopback listener serves plain HTTP for local use. Keep
`JERYU_WEB_TRUST_LOCAL` unset outside an intentional local development fixture.
The server refuses that bypass with a non-loopback bind. `--bind` selects a
listener; it does not enable TLS.

For remote access, use an independently configured TLS reverse proxy and
restrict the backend listener to that proxy. Preserve the request Host, Git
paths and WebSocket upgrade behavior. Configure trusted forwarding explicitly:
`JERYU_TRUSTED_PROXIES` accepts comma-separated exact proxy IP addresses for
client-IP rate limiting; the proxy must overwrite untrusted forwarded headers.
Do not expose the plaintext backend as a second public entry point.

Cookie security currently follows the backend bind address. A non-loopback
bind emits `__Host-jeryu-session` with `Secure`; loopback emits `jeryu-session`
without `Secure`. Placing a loopback backend behind HTTPS does **not** change
that setting. In that topology the proxy must add `Secure` to the session
cookie, preserving `HttpOnly`, `SameSite=Lax`, `Path=/` and logout expiry;
verify the actual browser response. Alternatively use a restricted private
non-loopback backend whose Secure cookie travels only through TLS externally.

Before remote activation, exercise login/logout/password changes, cookie flags,
CSRF and authorization denials, anonymous/public and private Git clone/push,
WebSockets, forwarded-client rate limits and restart through the actual TLS
endpoint. Proxy certificates/configuration need their own private backup and
renewal procedure. No particular reverse-proxy deployment is qualified by the
loopback runtime tests.

## Evidence boundaries

`startup_cli_and_restart_use_durable_state_from_any_directory` in the
[standalone process tests](../components/jeryu-deploy/crates/jeryu-cli/tests/standalone.rs)
is the owning same-binary stopped-backup/restore scenario. It archives runtime
data only, compares restored bytes/modes, reuses saved authentication, reads
Git/issues/Work/comments, accepts a post-restore write and verifies it after
restart while keeping the stopped original unchanged. GNU tar is required.
The fixture retains failures under a private `jeryu-recovery-test-*` root and
reports only its path and original identity. The fixture stops the server
with `Child::kill` (SIGKILL), reaps it, and preserves surviving SQLite sidecars;
this scenario does not exercise a graceful shutdown hook. Successful cleanup
requires an explicit completion call after owned processes are reaped, with
identity, link, mount and same-user process reference checks. Only the fixture's
own exact held root descriptor is exempt; unreadable process state retains
the fixture. Retained failure state needs separate reviewed retirement.

This document and test source are not execution evidence. Run the owning
`bash scripts/ci.sh runtime` lane at the exact candidate and retain its receipt.
A targeted existing-binary check can use the same standalone test filter, but
must record which executable it actually exercised. Same-binary restoration
does not qualify cross-version upgrades, backup under concurrent writes,
external dependencies, a remote TLS deployment or installed-service activation.
Those obligations remain visible in [current status](migration/STATUS.md).
