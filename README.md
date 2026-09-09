# Jeryu

Jeryu is a self-hosted forge with repositories, issues, pull requests,
protected merges, checks, a browser interface, and optional CI runners.
It is licensed under Apache-2.0.

**This branch is a monorepo migration candidate. Anonymous installation and
release qualification are incomplete.** See
[migration status](docs/migration/STATUS.md) for the remaining gates. The
currently installed service and existing release tags have not changed.

The intended public source installation is:

```bash
git clone https://github.com/neverhuman/jeryu.git
cd jeryu
./scripts/build.sh
./scripts/install.sh --from-source
jeryu serve
```

Source builds initially target Linux x86_64. Install Git, a C compiler,
pkg-config, OpenSSL development headers, rustup with the toolchain specified
in `rust-toolchain.toml`, and Node.js 22 or newer with npm. On Ubuntu, native
prerequisites are provided by `build-essential pkg-config libssl-dev git`.
The exact external Redline and governed auditor dependencies still need public
publication before a credential-free build can be qualified.

`build.sh` installs locked npm dependencies, builds the web application, and
builds the locked Rust CLI with embedded assets. It records source and binary
digests. The source installer rejects missing, modified, or stale artifacts.
It defaults to `~/.local/bin`; override this with
`--install-dir PATH` or `JERYU_INSTALL_DIR`. Add that directory to `PATH`.
The binary installer remains closed until central signed releases qualify.

Run `jeryu serve` from any directory, then open `http://127.0.0.1:8787`.
Storage selection is `--data-dir`, then `JERYU_DATA_DIR`, then
`$XDG_DATA_HOME/jeryu`, or `~/.local/share/jeryu` when XDG storage is unset.
SQLite and Git repositories persist there across restarts. `--spa-dir PATH`
explicitly serves a development bundle. Without it, the server uses embedded
assets and does not trust files in the current directory.

The first start creates only `jeryu-admin`. Its one-time password is written
to `bootstrap-credentials.json` in the data directory with owner-only access;
log in, change the password, and remove that credential receipt. An explicit
`JERYU_BOOTSTRAP_ADMIN_PASSWORD` retains the operator provisioning/reset flow;
unset it after provisioning. No personal accounts are created automatically.

CLI HTTP operations use `--api-url`, then `JERYU_API_URL`, then
`http://127.0.0.1:8787`. Create a personal access token in the authenticated
web interface and provide it through `JERYU_TOKEN_FILE` or `JERYU_TOKEN`.
Connection and authorization failures return errors. Historical commands
without server transports also return errors; their remaining implementation
is tracked in the migration status.

All 65 Rust packages live in the root Cargo workspace. The web application
and UX tooling use the root npm workspace. Component ownership remains under
`components/jeryu-core`, `jeryu-cache`, `jeryu-ci-runner`,
`jeryu-intelligence`, `jeryu-jira` (Work), `jeryu-web`, `jeryu-tool`,
`jeryu-tool-finder`, `jeryu-deploy`, and `jeryu-release-ops`.
Original manifests and locks are archived as provenance; their paths are not
active workspace configuration.

Read [AGENTS.md](AGENTS.md) before contributing. Changes target this monorepo;
the split repositories will be maintained as deterministic downstream mirrors
after qualification. The root manifest records a pending protected authority
handover, preserving the `jeryu-split` identity and immutable v5 lineage.

Prepare a component export with `cargo run --locked -p jeryu-split-tool --bin
jeryu-split -- export-tree --component jeryu-web --source FULL_COMMIT_SHA
--resolve-lock`. This writes a Git tree and source provenance without updating
remote refs. Rust exports bind external Jeryu packages to that monorepo commit;
npm exports preserve dependency integrities and relocate workspace links.
Lock resolution rejects changed external package versions and duplicate
Jeryu package identities. `bash scripts/test-split-exports.sh` reproduces every
export twice and runs its standalone checks in automatically removed Git
clones. The source commit and external dependencies must be available first;
passing these checks does not authorize publication.
