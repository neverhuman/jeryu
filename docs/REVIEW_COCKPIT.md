# JeRyu Web Forge — Merge Cockpit

> The Merge Cockpit is the **one room** for code review: diff, checks,
> threads, agents, gates, and blockers on a single page bound to one
> exact commit SHA.
>
> **Route:** `/repos/:provider/*fullName/merge-requests/:iid` (frontend);
> `/api/v1/repos/{repo_id}/merge-requests/{iid}` (backend).
>
> **Source plan:** WEB_WORK_CLAUDE.md §35.2.4 (12-gate Passport list),
> §35.1.6 (exact-SHA enforcement), §35.1.14 (canonical 14-step action),
> §35.1.11 (structured error envelope), `W-FE-11` (frontend), `W-B-11′`
> and `W-B-13′` (backend revisions).
>
> Companions: [`docs/WEB_API.md`](WEB_API.md) §8 for the REST surface,
> [`docs/WEBSOCKET_PROTOCOL.md`](WEBSOCKET_PROTOCOL.md) §3 for the events
> the cockpit listens to.

---

## 1. Three-pane layout

```
┌──────────────────────────────────────────────────────────────────────────┐
│ Header — MR title, !iid, badges (mergeability, conflict, draft)          │
├──────────────────────┬────────────────────────────┬──────────────────────┤
│ Left rail            │ Center — diff + threads    │ Right rail           │
│ (288 px)             │ (flex 1)                   │ (320 px)             │
│                      │                            │                      │
│ • Files changed      │ Diff (file-by-file)        │ Merge Passport       │
│   (virtualized)      │  ├── inline comments       │  • 12 gates, each    │
│ • Conversations      │  ├── code-owner badges     │    with status icon  │
│   (open / resolved)  │  ├── new-comment composer  │    and blocker code  │
│ • CI checks summary  │  └── line-by-line blame    │ "Why blocked?"       │
│ • Agent evidence     │                            │  panel               │
│ • Settings shortcut  │ Threads (collapsed)        │                      │
│                      │ Suggested commits          │ Approval & merge     │
│                      │                            │  controls            │
└──────────────────────┴────────────────────────────┴──────────────────────┘
```

Both rails persist across route transitions inside the merge-request
context. The center pane swaps between **Conversation**, **Files**,
**Commits**, **Checks**, and **Agents** tabs via `[` / `]`.

The cockpit subscribes to the WebSocket scopes:

- `mr.{mr_id}` — verdicts, threads, approvals, merge result.
- `repo.{repo_id}.checks` — incoming CI status updates.
- `repo.{repo_id}.refs` — head-SHA changes after preview / approval.

When any of these triggers a `passport_*` re-evaluation, the right-rail
Passport widget animates to the new state without a page reload.

---

## 2. Merge Passport gates (12 checks)

The Merge Passport is the **single derived boolean** that gates merge.
Verbatim from WEB_WORK_CLAUDE.md §35.2.4; each gate has a stable blocker
code so the UI can render targeted "Why blocked?" copy and link to
remediation.

| # | Gate | Blocker code | Source of truth |
|---|---|---|---|
| 1 | Source SHA unchanged since preview/approval | `passport_blocked_source_sha` | `src/merge/guards.rs::compare_head_sha` |
| 2 | Target branch SHA checked | `passport_blocked_target_sha` | `src/merge/guards.rs::compare_target_sha` |
| 3 | Target policy SHA checked | `passport_blocked_policy_sha` | `src/git_host/codeowners.rs::fetch_target_policy_sha` |
| 4 | Required approvals satisfied | `passport_blocked_approvals` | `src/merge/review.rs::approvals_satisfied` |
| 5 | Code owners signed off where required | `passport_blocked_code_owners` | `src/git_host/codeowners.rs::ownership_for_changes` |
| 6 | All threads resolved | `passport_blocked_threads` | `src/merge/review.rs::threads_open` |
| 7 | Required CI green | `passport_blocked_ci` | `src/merge/merge_gate.rs::required_checks` |
| 8 | VTI / test plan acceptable | `passport_blocked_vti` | `src/merge/merge_gate.rs::vti_plan` |
| 9 | Agent evidence fresh and signed | `passport_blocked_agent_evidence` | `src/merge/merge_gate.rs::agent_evidence` |
| 10 | Branch protection rules satisfied | `passport_blocked_branch_protection` | `src/git_host/gitlab.rs::branch_protection` |
| 11 | Conflict status (no rebase needed) | `passport_blocked_conflict` | `src/git_host/gitlab.rs::mergeability` |
| 12 | Release window / deploy freeze respected | `passport_blocked_release_window` | `src/merge/merge_gate.rs::release_window` |

Each gate's data lives on the `MergePassport` DTO
(`contracts/generated/MergePassport.ts`):

```ts
export interface MergePassport {
  status: MergePassportStatus; // "ready" | "blocked"
  blockers: MergePassportBlocker[];
  evaluated_at: string;
  passport_hash: string;        // persisted; lets us detect drift
}

export interface MergePassportBlocker {
  code: string;                 // e.g. "passport_blocked_threads"
  gate: string;                 // human label
  detail: string;
  remediation_url?: string;     // e.g. /repos/.../merge-requests/42#thread-7
  severity: "info" | "warn" | "error";
}
```

The backend persists `passport_hash` on `web_merge_requests` after every
Passport recomputation (`W-B-11′`) so re-evaluation drift is detectable
in audit log search.

---

## 3. Exact-SHA semantics (TOCTOU prevention)

Approve and merge are the two routes that operate on a *commit* rather
than a *branch*. Both refetch the live source / target / policy SHAs
from the host inside the handler before producing the action receipt.

```
client                                    BFF                               host
  │                                        │                                  │
  │ POST .../approve                       │                                  │
  │  { expected_head_sha: "abc123" }       │                                  │
  │ ──────────────────────────────────────►│                                  │
  │                                        │ fetch live source SHA            │
  │                                        │ ───────────────────────────────► │
  │                                        │ ◄─── { sha: "def456" }           │
  │                                        │                                  │
  │                                        │ expected != live  →  409          │
  │ ◄─────── 409 merge_sha_stale            │                                  │
  │  { details: { expected: "abc...",      │                                  │
  │               live: "def..." } }       │                                  │
```

On `409 merge_sha_stale` the UI:

1. Highlights the head-SHA badge in the header (red).
2. Surfaces a banner: "The source branch changed. Reload to review the
   latest commits before approving."
3. Offers a one-click "Refresh and review changes" action that runs
   `GET .../diff?base=expected&head=live` so the reviewer sees exactly
   what changed.

The approval is **never** applied to the stale commit. Reviewers must
explicitly re-approve after reviewing the new content.

Same logic applies to the merge endpoint, with the additional contract
that gates 1, 2, 3 in §2 must all be satisfied at the live SHAs. The
merge handler treats `expected_head_sha` as authoritative; mismatch is
always `409 merge_sha_stale`, never an implicit re-evaluation.

---

## 4. "Why blocked?" surface

The right-rail Passport renders, for each blocked gate:

- The gate label (column 2 of the §2 table).
- A short reason (`MergePassportBlocker.detail`).
- A link to remediate, where one exists (`remediation_url`).
- A `code` tooltip for support / debugging (`passport_blocked_<gate>`).

When the merge button is clicked while blocked, the click is **not**
sent to the server. Instead, the UI focuses the first blocker in the
Passport and announces it via `aria-live="polite"`:

> "Merge blocked: required approvals missing (1/2)."

Screen readers receive the same announcement; the keyboard focus moves
to the relevant blocker so a Tab keypress walks the user to the
remediation link.

---

## 5. Approval flow with idempotency

```
1.  reviewer clicks "Approve"
2.  composer opens with optional summary
3.  UI generates Idempotency-Key (uuid v4)
4.  POST .../approve { expected_head_sha, summary? }
       Headers: X-CSRF-Token, Idempotency-Key
5.  on 200 → optimistic update + WS event arrives → store invalidates
6.  on 409 merge_sha_stale → §3 recovery flow
7.  on network error → composer reopens with the Idempotency-Key
     preserved; retry replays the call against the same receipt
```

The Idempotency-Key is bound to the **action + target + key** triple in
`web_action_receipts`, so a retry against the same MR with the same key
returns the prior `200` body (the approval already exists). Retrying
with the same key against a different MR returns `409
idempotency_conflict` instead of silently confusing receipts.

---

## 6. Merge flow

The merge button has three sub-actions surfaced as a dropdown when the
Passport is `status: "ready"`:

- **Merge** — standard merge commit.
- **Squash and merge** — collapse to one commit; uses the MR title +
  description by default, editable in a preview panel.
- **Rebase and merge** — applies the source branch on top of the
  target, then fast-forwards.

The request body always carries `method` and `expected_head_sha`:

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  --header 'X-CSRF-Token: ...' \\
  --header 'Idempotency-Key: ...' \\
  --header 'Content-Type: application/json' \\
  --data '{ "expected_head_sha": "abc123", "method": "squash", "delete_source_branch": true }' \\
  http://127.0.0.1:8787/api/v1/repos/repo-uuid/merge-requests/42/merge
```

Possible terminal states emitted on `mr.{mr_id}`:

| Event | Means |
|---|---|
| `mr.merged` | Server-side merge succeeded; `payload.merge_commit_sha` is set. |
| `mr.merge.blocked` | The handler ran but a Passport gate flipped between preview and execute; the response is `409 conflict` with `details.code = "passport_blocked_<gate>"`. |
| `mr.updated` | A non-terminal state change (e.g. branch updated mid-flight); the UI re-evaluates and the user resumes. |

The Merge Passport is **re-evaluated server-side immediately before the
merge call**. The handler refuses to call the host if the in-process
Passport is `blocked`, even if the SPA presented a green button (e.g.
because of a stale live update).

---

## 7. Agent evidence integration (read-only v1)

The "Agents" tab in the cockpit (W-B-31 read-only v1) shows:

- Active agent sessions on this MR (id, model, allowed tools).
- Proposed patches (linked to the MR's diff).
- Evidence packets (signed Jankurai receipts).
- Outstanding blockers raised by the agent runtime.

The merge gate `passport_blocked_agent_evidence` is satisfied when the
latest evidence packet is:

1. Signed by an agent identity allowed by `RepositorySettings.agents.allowed_agents`.
2. Newer than the source SHA (so evidence cannot be replayed across commits).
3. Marked `status: "passed"` in the packet body.

Writing agent evidence from the cockpit (run-an-agent, attach-receipt)
is **v1.5+**. v1 surfaces the evidence read-only with deep links to the
Jankurai dashboard for actions.

The "Why blocked?" copy for this gate links to the evidence pack viewer
inside the SPA at `/repos/:provider/*fullName/merge-requests/:iid/agents`.

---

## 8. Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `[` / `]` | Previous / next file in the diff. |
| `j` / `k` | Next / previous thread in the current file. |
| `n` | Jump to the next unresolved blocker in the right rail. |
| `r` | Open the inline reply composer on the focused thread. |
| `g a` | Jump to the Agents tab. |
| `g c` | Jump to the Checks tab. |
| `g f` | Jump to the Files tab. |
| `Enter` | Submit the focused composer (with optional `Shift` to add a newline). |
| `Esc` | Close the composer or the "Why blocked?" panel. |
| `?` | Show the global shortcut overlay (`KeyboardShortcutsOverlay`). |

These shortcuts are registered with the same `useKeyboardShortcut`
primitive used by the rest of the app (see `apps/web/src/hooks/useKeyboard.ts`).

---

## 9. Observability and audit

Every cockpit interaction emits one of:

| Event | When |
|---|---|
| `action.previewed` | A preview API call returns (low priority). |
| `action.executed` | A mutating handler succeeds (medium/high priority). |
| `mr.review.submitted` | Approve / request-changes / comment review verdicts. |
| `mr.approved` | Exact-SHA approval landed (high priority). |
| `mr.thread.created` / `mr.thread.resolved` | Threads. |
| `mr.merged` | Terminal success (high priority). |
| `mr.merge.blocked` | Passport flipped between preview and execute (high priority). |
| `audit.event.created` | Mirror of the above into the human-facing audit log. |

The `web_action_receipts` rows for an MR are linkable from the audit
viewer; they carry the `expected_state_hash` (passport hash at preview
time), `resulting_state_hash` (passport hash post-merge), and the full
`provider_calls_json` (GitLab call trace).

This makes a forensic "did the gate change?" question answerable by a
single SQL query against `web_action_receipts WHERE target_id = '<mr_id>'`.
