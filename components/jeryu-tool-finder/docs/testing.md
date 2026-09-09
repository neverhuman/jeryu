# Testing

`jeryu-tool-finder` is a compiled Rust CLI with unit, property, and integration
tests over scan → dossier → propose. Deterministic lane entrypoints live under
`ops/ci/`; the protected hosted check and local commands invoke the same
scripts.

## Local gate

Run the full gate with one command:

```
just
```

or, with no `just` installed, the same lanes directly:

```
bash scripts/ci-local.sh required   # check → score → security → contract → artifact
```

`scripts/ci-local.sh` accepts exactly one governed lane. `required`, `security`,
`score`, `contract-drift`, and `artifact-support` each delegate exactly once to
their canonical absolute script. Missing, extra, unknown, option-shaped, legacy,
and injection-shaped inputs fail before delegation.

Run `scripts/ci-doctor.sh` first to confirm your environment carries every tool
the lanes depend on (`bash`, Cargo/Rust, `git`, `jq`, `jankurai`, `gitleaks`,
`actionlint`, `cargo-audit`, `cargo-deny`, `shellcheck`, and `syft`; `just` is an
optional wrapper).

## Lanes

There is **no `fast` lane** here — the jankurai pin is owned by `jeryu-tool`, so
this repo has no pin-drift lane (see `agent/proof-lanes.toml`).

- `just check` (`ops/ci/check.sh`) — the Rust finder passes fmt, warnings-denied
  locked/offline Clippy and tests; locked metadata proves both Intelligence
  crates resolve from immutable `split.1`; every shell entrypoint parses and
  passes ShellCheck; and hostile dispatch, governed-auditor, and
  source-authority tests pass.
- `just score` (`ops/ci/score.sh`) — runs the pinned jankurai audit over this
  exact clean head. It fails below the policy floor or on any cap/hard finding,
  then writes `target/jankurai/evidence.json` binding head, tree, policy, raw
  report digest, tracked-input digest, report fingerprints, and the exact
  governed auditor binary/receipt identity.
- `just security` (`ops/ci/security.sh`) — runs source/workflow/environment,
  locked metadata, mandatory Cargo license/ban/source policy, cached Cargo
  audit, and SPDX generation. Its evidence binds the exact clean head/tree and
  subordinate artifact digests; a missing scanner or malformed output fails.
- `just contract-drift` (`ops/ci/contract-drift.sh`) — runs the Rust integration
  test that compares compiled top-level help byte-for-byte with
  `contracts/cli-help.txt`.
- `just artifact-support` (`ops/ci/artifact-support.sh`) — requires current
  passing score evidence, clean Cargo-audit evidence, a valid generated SPDX
  document, the tracked CLI/version contracts, and a locked release build in a
  new private target. Only then does it publish `status=ready`; its hostile test
  rejects linked, preseeded, substituted, build-overridden, missing, or
  non-ready evidence while preserving any prior valid pair.

## The dossier selftest

The Rust suite carries hermetic dossier enrichment and end-to-end pipeline
fixtures. It asserts dossier shape, LOC-saved math, proposal idempotency, comment
preservation, and dry-run behavior without a network dependency.

## CI parity & repair evidence

Release authority is the protected hosted Jeryu
`jeryu-tool-finder/required` check. `.github/workflows/ci.yml` is only a checked,
non-authoritative parity artifact. The pre-push hook delegates to `required`, so
it cannot silently omit the product contract or artifact lane.

The scripts surface failures as non-zero exits with a one-line cause on stderr
(missing tool, failed selftest, committed `.env`); the audit writes its full,
fingerprinted evidence to `.jankurai/repo-score.json`.

## Repair receipts

Every failure leaves a repair receipt that tells the next agent where to rerun
proof. A failed lane prints its cause on stderr and exits non-zero; the score
lane additionally writes a structured receipt to `.jankurai/repo-score.json`,
where each finding carries `path`, `agent_fix`, and `rerun_command` (and the
`agent_fix_queue` orders them). The repair loop is therefore: read the receipt,
fix the named `path`, and rerun the mapped command in `agent/test-map.json`
until the finding clears. See
`agent/JANKURAI_STANDARD.md` for the repair-receipt contract.
