# Jeryu

Jeryu is a self-hosted forge for Git repositories, issues, pull requests,
reviews and protected merges, with a browser interface and optional CI runners.

**Status: PENDING qualification.** This monorepo is a source candidate.
Complete CI, anonymous public-origin installation and the central release
are still unqualified. Read [current status](docs/migration/STATUS.md) and
the [dependency and audit dashboard](docs/dependencies.md).

## Quick start

Install the prerequisites below, then use the source installation contract:

```bash
git clone https://github.com/neverhuman/jeryu.git
cd jeryu
./scripts/build.sh
./scripts/install.sh --from-source
export PATH="${JERYU_INSTALL_DIR:-$HOME/.local/bin}:$PATH"
jeryu serve
```

The first build needs network (rustup, crates.io, npm). It does not need
Docker or Jankurai. Open `http://127.0.0.1:8787`. The default bind is the
same port as a local forge; if you see `Address already in use`, pass
`--bind 127.0.0.1:8788`. The build embeds production browser assets and
uses bundled SQLite; no database service is required. Source installation
verifies source and binary digests and rejects missing, modified or stale
artifacts. Complete anonymous qualification of these commands remains pending.

The installer defaults to `~/.local/bin` and prints that path. Add it to
`PATH` or the `jeryu` command will not be found. Override the destination
with `--install-dir PATH` or `JERYU_INSTALL_DIR`. Binary installation
remains closed until central signed releases qualify.

### Prerequisites and platforms

The source installation currently targets Linux x86_64. Install Git, a C
compiler, pkg-config, OpenSSL development headers, rustup, and
Node.js with npm. On Ubuntu, the native packages are
`build-essential pkg-config libssl-dev git`.

The root [Rust toolchain](rust-toolchain.toml) pins the compiler. Node.js
22.19+ on the 22.x line or Node.js 24+ is supported by the web toolchain
(Node.js 23 is not). CI uses Node.js 26.1.0. Other operating systems and
architectures are not qualified release targets. Ordinary application
builds do not require installing Jankurai or optional runner agents.

## First login and durable data

The first start creates only `jeryu-admin`. Its one-time password is stored
in `bootstrap-credentials.json` inside the data directory with owner-only
access. The server prints that path once (never the password). Default data
dir is `~/.local/share/jeryu`. Log in, change the password and remove that
credential receipt.
If first start is interrupted after writing the receipt, the next start reuses
it before creating the account. An incomplete or unsafe receipt stops bootstrap
and remains available for operator recovery.
An explicit `JERYU_BOOTSTRAP_ADMIN_PASSWORD` retains the operator provisioning
and reset flow; unset it after provisioning.

Data selection is `--data-dir PATH`, then `JERYU_DATA_DIR`, then
`$XDG_DATA_HOME/jeryu`, or `~/.local/share/jeryu` when XDG storage is unset.
SQLite and Git repositories persist there across restarts. Run
`jeryu serve` from any directory; embedded assets are used by default.
`--spa-dir PATH` explicitly serves a development browser bundle.

`--store sqlite` makes the default explicit and takes precedence over
`JERYU_STORE`. Compatibility values `redline` and `redlinedb` currently use
the same SQLite engine and print a fallback notice. They do not load RedlineDB.
Unknown values fail before runtime state is created.

CLI HTTP operations select `--api-url`, then `JERYU_API_URL`, then
`http://127.0.0.1:8787`. Create a personal access token in the authenticated
browser interface and supply `JERYU_TOKEN_FILE` or `JERYU_TOKEN`.
Connection and authorization failures return errors. Historical commands whose
server transports remain unavailable also return errors.

## Development and contribution

Develop in this repository's root Rust and npm workspaces. All 65 Rust
packages share the root Cargo lockfile; the browser and UX checks share the
root npm lockfile.

```bash
./scripts/build.sh
bash scripts/ci.sh source
bash scripts/ci.sh web
```

These commands have different scopes. See [testing](docs/testing.md) for
the complete matrix and capability requirements. A passing subset does not
qualify a release. [CONTRIBUTING.md](CONTRIBUTING.md) explains contribution
routing, regression tests and independent review; [AGENTS.md](AGENTS.md)
contains repository rules.

## Component map

Changes belong under `components/` in this monorepo. Component repositories
retain their identities and histories as downstream mirror targets; independent
export qualification and protected publication remain pending.

| Component | Source responsibilities |
| --- | --- |
| [Core](components/jeryu-core) | Domain, Git storage, read models and forge primitives |
| [Cache](components/jeryu-cache) | Build artifact cache and cache defenses |
| [Runner](components/jeryu-ci-runner) | Optional CI execution, scheduling and isolation |
| [Intelligence](components/jeryu-intelligence) | Reviews, Codegraph and agent integrations |
| [Work](components/jeryu-jira) | Work items; technical identity remains `jeryu-jira` |
| [Web](components/jeryu-web) | Embedded browser application and UX checks |
| [Tool](components/jeryu-tool) | Governed auditor identity and tool control |
| [Tool Finder](components/jeryu-tool-finder) | Tool discovery |
| [Deploy](components/jeryu-deploy) | Server, CLI, installation and monorepo tooling |
| [Release Ops](components/jeryu-release-ops) | Release, evidence and contract tooling |

Read the [architecture](docs/architecture.md) for ownership and dependency
boundaries, and the [split publication contract](docs/migration/SPLIT-PUBLICATION.md)
for deterministic exports.

## Component exports

The monorepo contains its complete component source. Build it directly with
`scripts/build.sh`; component repositories receive downstream exports.
[`family.lock.toml`](family.lock.toml) preserves historical immutable export
tags. It does not select build inputs or authorize replacement of current source.
The compatibility `scripts/fetch-family.sh --plan` command verifies embedded
source through the owning Rust command and never reconstructs components.

The auditor identity is owned by
[`tool-manifest.toml`](components/jeryu-tool/tool-manifest.toml).
`bash scripts/ci.sh auditor` acquires its public source, verifies the full
build and executable contract, and writes a source-bound installation receipt.
Matching a downloaded binary hash alone does not qualify the producer.

Maintained-head and release-pinned audits are tracked on the
[dependency dashboard](docs/dependencies.md). Live cards remain pending until
the separate maintainer service authenticates execution and publishes all
report formats together. Historical numeric badge files do not qualify this head.

## Dependencies, audits and releases

The [dashboard](docs/dependencies.md) separates maintained heads from the
versions used by Jeryu. Verified live SVG results and an evidence publisher
remain pending; a missing report is not a passing score. The required score
floor is at least 85, preserving stronger 91-point proof gates and ratchets.

RedlineDB compatibility is optional: `bash scripts/ci.sh redline` runs its
separately locked [contract harness](components/jeryu-release-ops/tests/redline/README.md).
It does not switch the server backend or block SQLite release eligibility.
Runner installation is also optional; native sandbox and product-image
qualification remain separate obligations.

See [release status](docs/release.md), [backup and recovery](docs/recovery.md),
[support](SUPPORT.md) and [security reporting](SECURITY.md). Source publication, release authority
handover and installed-service activation each require their own evidence.

Jeryu is licensed under [Apache-2.0](LICENSE). Bundled JetBrains Mono fonts
retain [SIL Open Font License 1.1 and notices](components/jeryu-web/apps/web/public/THIRD_PARTY_NOTICES.txt).
[Web dependency notices](docs/notices/web-bundles.md) cover current and
preserved historical bundles.
