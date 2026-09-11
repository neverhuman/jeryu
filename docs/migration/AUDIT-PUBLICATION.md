# Private audit publication preparation

`jeryu-split audit-package` validates and stores an immutable private bundle of
audit observations and their inputs. Hosted execution, protected-policy
authentication, qualified SVG rendering, sanitization and publication remain
unimplemented admission gates. This command always returns a nonzero result
for publication, including when private storage succeeds.

The command belongs to CI tooling. Building, installing and serving Jeryu do
not require it.

## Inputs and source identity

The local context uses `jeryu.audit-package-local-context/v1`; the executor
observation uses `jeryu.audit-publication-observation/v1`. Both contain the
complete identity, observed command exit and optional full report digest.
Context files are data and cannot authenticate themselves. A future trusted
adapter must obtain expected identities independently of submitted artifacts.

Identity binds full source commit and tree, policy and dependency-lock hashes,
auditor source/executable/receipt hashes, execution configuration, workflow
source and run/job/attempt identities. Logical ownership and physical source
identity are separate:

| Form | Logical `repository` | Actual `source_repository` | Additional identity |
| --- | --- | --- | --- |
| Root | `neverhuman/jeryu` | `neverhuman/jeryu` | Root commit/tree |
| Component | Owning component slug | `neverhuman/jeryu` | Exact component path/tree |
| Standalone mirror | Owning component slug | Owning component slug | Originating monorepo commit and export provenance hash |
| Supporting dependency | Supporting repository slug | Same repository slug | Exact dependency commit/tree |

A component result cannot substitute for a standalone mirror result. The
existing full-report validator and owning auditor pin grammar are reused.
Candidate and governing policy bytes must currently match. Minimum 85,
stronger owning floors, root soft-finding limits, hard findings, caps, ratchets
and conformance remain enforced during consistency checks. Matching inputs do
not establish protected-policy or execution authority.

Use an existing physical owner-only output directory outside source:

```sh
jeryu-split audit-package \
  --context context.json \
  --receipt executor-observation.json \
  --report report.json \
  --candidate-policy candidate-policy.toml \
  --governing-policy governing-policy.toml \
  --dependency-lock Cargo.lock \
  --execution-config execution.json \
  --auditor-receipt auditor-receipt.json \
  --output-root /absolute/private/audit-packages
```

Check the JSON result and preserve the nonzero exit. Do not turn successful
private storage into a passing required publication check. Required input
files must be bounded regular files; links and concurrent changes are refused.
An unreadable requested report becomes an ERROR observation when the remaining
required context can be read. Invalid required context cannot create a bundle.

## Decisions and retained bytes

| Observation | Display state | Displayed score |
| --- | --- | --- |
| Internally consistent policy pass | PENDING | None |
| Valid policy failure | FAIL | Failing observed score, explicitly unqualified |
| Missing, inconsistent or rejected evidence | ERROR | None |
| Required renderer failure or identity mismatch | ERROR | None |

Optional `--renderer-output` bytes require a matching renderer observation in
the context. They remain opaque `renderer-output.bin`, with SVG safety and
renderer qualification set to false. They are never executed or published.
The saved renderer request describes the attempted rendering; a subsequent
renderer error may change the final preparation state to ERROR.

Destinations contain actual source repository, scope, logical owner, full
source commit and a digest of the complete expected observation. Identical
replays are accepted. Different bytes at the same address are refused without
replacing the original; a retry needs its own attempt identity.

The bundle retains provenance, Markdown, raw report, binding inputs, renderer
request and optional opaque output with a hash/length manifest. Directories
are owner-only and files use mode 0600. Creation uses an atomic non-overwriting
rename; incomplete or conflicting staging remains available for inspection.
Storage assumes one trusted writer and does not provide isolation from another
malicious process with the same Unix owner.

Raw reports and diagnostics may be sensitive. These private bundles are not
sanitized public artifacts. The generated evidence branch, Pages deployment,
live README cards and race-safe current-head pointers remain separate work.
