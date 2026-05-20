# Install jankurai

Run `jankurai init --profile rust-ts-redlinedb --ide all --mode advisory --dry-run`, review the plan, then rerun with `--yes`.

For JeRyu local/CI parity, install the pinned RedlineDB v1.0.1 host binary with `bash scripts/install-redlinedb.sh`. The installer selects the platform tarball from `neverhuman/RedlineDB`, requires the matching `.sha256` asset, verifies the archive before extraction, and installs `redlinedb` under `$HOME/.local/bin`. Set `REDLINEDB_VERSION` only when intentionally moving to another RedlineDB release; use `REDLINEDB_INSTALL_MODE=verify` only for offline checks of an already-installed binary. This tooling backs embedded `redline:` file-backed state; it does not start a RedlineDB Docker service.

For Rust services that want runtime repair packets, an optional `witness-rt` crate can emit packets that feed the Rust witness and diagnose flows.
