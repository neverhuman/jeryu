# Contracts Agent Instructions

Owns generated and authored contract artifacts for Tool Finder
(`contracts/` and the CLI help contract checked by `ops/ci/contract-drift.sh`).

Allowed edits:
- Refresh generated contract files through the documented producer.
- Update the help-contract fixture when a CLI flag change is intentional.

Forbidden edits:
- Do not hand-edit generated contract bytes to silence drift.
- Do not add a second contract authority outside this directory.

Proof lane:
- `just contract-drift`
- `bash ops/ci/contract-drift.sh`
