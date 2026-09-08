# Phase 11 runbooks

## `hold-merges`

Use when required health signals are absent. No merge should proceed until the readiness report emits a non-blocked state.

## `revoke-runner-lease`

Use when runner heartbeat is stale. The action is safe to automate because stale leases are already invalid for job trust.

## `scale-read-path`

Use when API, cache, or artifact read latency is degraded but not failing.

## `shed-noncritical-work`

Use when queue depth threatens protected CI, release, or audit work.

## `page-owner`

Use when error rate is critical. This is not auto-remediated because root cause may require human authorization.
