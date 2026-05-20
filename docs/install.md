# Install jankurai

Run `jankurai init --profile rust-ts-redlinedb --ide all --mode advisory --dry-run`, review the plan, then rerun with `--yes`.

For JeRyu local/CI parity, install the pinned Jankurai v1.5.1 host binary with `bash scripts/install-jankurai.sh`. The installer reads the checked-in `scripts/jankurai-manifest.json`, downloads the platform tarball from `neverhuman/jankurai`, verifies the archive against its pinned SHA256 before extraction, and installs `jankurai` under `$HOME/.local/bin`. Use `JANKURAI_INSTALL_MODE=verify` only for offline checks of an already-installed binary.

Install the pinned RedlineDB v1.0.1 host binary with `bash scripts/install-redlinedb.sh`. The installer reads the checked-in `scripts/redlinedb-manifest.json`, downloads the platform tarball from `neverhuman/RedlineDB`, verifies the archive against its pinned SHA256 before extraction, and installs `redlinedb` under `$HOME/.local/bin`. Use `REDLINEDB_INSTALL_MODE=verify` only for offline checks of an already-installed binary. This tooling backs embedded `redline:` file-backed state; it does not start a RedlineDB Docker service.

For Rust services that want runtime repair packets, an optional `witness-rt` crate can emit packets that feed the Rust witness and diagnose flows.
