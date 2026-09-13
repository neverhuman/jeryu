# Local audit attempt accounting

`jeryu-split audit-ledger` records planned audits, execution leases, reports,
failures, and retries in a private SQLite database. It does not run an auditor,
authenticate an executor or governing policy, publish evidence, or satisfy a
required audit gate. A structurally valid passing report is recorded as
`completed_unqualified`.

This command belongs to development and CI tooling. It is not required to
build, install, or serve the application.

## Storage and inputs

Choose an absolute database path in an existing physical directory owned by the
operator with mode 0700. Keep this directory outside source checkouts and public
artifacts; its database retains raw reports and receipts. The command creates
the database with mode 0600 and refuses unexpected existing database identity,
links, ownership, or permissions. Select the same database for each operation.

The examples use `/absolute/private/audit-ledger/ledger.sqlite`. Replace that
path with the chosen operator storage directory. The `jeryu-split` executable
is built by the existing `jeryu-split-tool` workspace package.

First obtain a successful `audit-plan` result using a complete local Git graph:

```sh
jeryu-split audit-plan \
  --source-repo /absolute/qualified/source \
  --request event.json > plan.json
```

Check this command's exit status before importing its output. A planner failure
must remain an unresolved intake event; the eventual hosted intake adapter still
needs durable accounting for events that fail before a plan can be produced.
The ledger does not fetch source or authenticate webhook events.

PR and release plans resolve the exact referenced commit, including an annotated
release tag's target, and include its entire ancestry through both merge parents.
The `exact_revision` disposition identifies that pinned endpoint; it does not
limit the plan to one commit. This covers history first observed through a forked
PR or release. The ledger deduplicates only identical source/auditor/policy and
execution inputs; an earlier plan that omitted ancestry must be backfilled.

Import the plan with its exact validation inputs:

```sh
jeryu-split audit-ledger \
  --database /absolute/private/audit-ledger/ledger.sqlite \
  import-plan \
  --plan plan.json \
  --execution-config execution.json \
  --governing-policy governing-policy.toml \
  --candidate-policy candidate-policy.toml
```

The plan's execution-configuration and governing-policy hashes must match the
supplied bytes. `execution.json` has this closed structure:

```json
{
  "schema_version": "jeryu.audit-ledger-execution/v1",
  "auditor_version": "1.6.11",
  "policy_path": "./agent/audit-policy.toml",
  "minimum": 85,
  "max_soft": 0,
  "candidate_policy_sha256": "<SHA-256 of candidate-policy.toml>",
  "executor_inputs": {
    "<immutable executor selection>": "<exact value>"
  }
}
```

Use the actual producer version and selection inputs. The policy path matches
the current full-audit executor. `executor_inputs` preserves remaining command,
compiler, platform, and dependency selections as data; the ledger never executes
this object or establishes that the declared inputs were used.

Candidate and governing policy bytes must currently match exactly. Reviewed
policy migrations need a separate governed comparison adapter. Existing policy
validation checks ownership, minimum 85, the 91-point Cache/Work/Runner floors,
required producer versions, and declared soft-finding limits. Root Jeryu requires
`max_soft: 0`. Matching bytes or JSON fields do not authenticate protected policy
or prove that a policy occurs in the claimed source tree.

Current scope names are `repository`, `standalone`, `dependency`, `optional`, and
`components/<name>` inside `neverhuman/jeryu`. Optional release eligibility
remains a census or hosted-adapter decision; the local ledger never certifies a
release.

## Attempts and reports

Select the desired job's `attempt_key` from the imported plan. This identifies
the scheduled request; a start receives its own immutable ledger attempt ID.

```sh
jeryu-split audit-ledger \
  --database /absolute/private/audit-ledger/ledger.sqlite \
  start --request-key '<plan job attempt_key>' --lease-seconds 300
```

The result contains `attempt_id`, `deduplication_key`, an ordinal, and a deadline.
Only one live lease is permitted for identical source/auditor/policy/execution
inputs. Repeated event imports preserve their scheduled requests while sharing
the same source job. Retries receive new attempt IDs and retain original failures.

After a separate executor has run, record its observed outcome:

```sh
jeryu-split audit-ledger \
  --database /absolute/private/audit-ledger/ledger.sqlite \
  finish --receipt observation.json --report report.json
```

The observation uses this closed structure:

```json
{
  "schema_version": "jeryu.audit-ledger-observation/v1",
  "attempt_id": "<ID returned by start>",
  "deduplication_key": "<key returned by start>",
  "source_commit": "<full planned commit>",
  "source_tree": "<full planned tree>",
  "identity": {"<every identity field from the plan>": "<exact value>"},
  "command_exit": 0,
  "outcome": "report",
  "reason": "<concrete observed outcome>",
  "report_sha256": "<SHA-256 of exact report.json bytes>"
}
```

Copy the complete identity object from the plan; the abbreviated object above is
not an accepted substitute. Supply the observed command exit rather than the
example's zero. Full reports use the same strict validator as the audit census.
Missing, truncated, contradictory, or mismatched reports cannot produce success.
A high score with hard findings or caps fails. A passing report following a
failed command is rejected. Valid passing reports remain
`completed_unqualified`; supplied exit codes are not authenticated execution.

For an attempt without a report, omit `--report` and use the actual outcome:
`tool_error`, `timed_out`, `source_unavailable`, or `canceled`. Set
`report_sha256` to null and `command_exit` to null if no command ran. Include a
concrete reason. All such outcomes retain a retry obligation.

An evidence ID binds the attempt, receipt bytes, and report bytes. Repeating the
same completion is idempotent. A different completion cannot overwrite the first
one; a retry uses a new attempt. Raw evidence remains private in the database.

## Closure, reconciliation, and status

Finishing an attempt does not release its lease. Neither does a wall-clock
expiry. After observing actual executor process closure, the operator or
executor records a separate acknowledgement:

```sh
jeryu-split audit-ledger \
  --database /absolute/private/audit-ledger/ledger.sqlite \
  close --acknowledgement closure.json
```

```json
{
  "schema_version": "jeryu.audit-ledger-closure/v1",
  "attempt_id": "<ID returned by start>",
  "deduplication_key": "<key returned by start>",
  "executor_closed": true,
  "reason": "<how the executor's closure was observed>"
}
```

The ledger records this local assertion and labels it unauthenticated. It cannot
prove process closure itself. An outcome must already be recorded; retries stay
blocked until the separate acknowledgement exists.

Reconcile incomplete attempts and inspect all stored obligations:

```sh
jeryu-split audit-ledger \
  --database /absolute/private/audit-ledger/ledger.sqlite reconcile

jeryu-split audit-ledger \
  --database /absolute/private/audit-ledger/ledger.sqlite status > status.json
```

Reconciliation records every expired live lease without truncation and releases
none. An overdue first finish records the expiry before its outcome, so timeout
history survives either ordering of finish and reconciliation. A late report
never erases that timeout.

Status opens the database read-only and includes every plan, request, attempt,
observation, evidence hash, closure acknowledgement, and pending retry. Pure
rewinds retain their event and withdrawn commits even when they introduce no new
source jobs. Original reports remain attached to their exact source identities.

**Status deliberately exits nonzero:** this layer has no trusted admission path,
so `accepted_full_audits` remains zero and `required_audit_satisfied` remains false.
Do not convert this exit into a passing audit check or publish an unqualified
completion as a verified score. Hosted intake, qualified execution, protected
policy admission, and evidence publication remain separate work.
