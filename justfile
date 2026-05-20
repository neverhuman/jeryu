default:
    @just --list

agent-index:
    cargo run -p jeryu -- repo render-agent-index

agent-audit:
    cargo run -p jeryu -- repo audit-agent-surface --json

agent-refresh:
    cargo run -p jeryu -- repo render-agent-index
    cargo run -p jeryu -- repo audit-agent-surface --json

fast:
    # fast-lane
    mkdir -p target/jankurai/cache
    CARGO_INCREMENTAL=0 cargo check --workspace --message-format=json
    CARGO_INCREMENTAL=0 cargo nextest run -p jeryu --lib

proof:
    mkdir -p target/jankurai
    jankurai proof . --changed-fast --out target/jankurai/fast-score.json

audit-fast base="origin/main":
    mkdir -p target/jankurai
    jankurai audit . --changed-fast --changed-from {{base}} --json target/jankurai/audit-fast.json --md target/jankurai/audit-fast.md --timings-json target/jankurai/audit-timings.json --mode advisory

jankurai-install JANKURAI_TAG="v1.5.1":
    # Jankurai MUST be installed from URL with an explicit version tag.
    # Local-path installs are not supported (they produce version drift).
    cargo install --git https://github.com/neverhuman/jankurai.git --tag {{JANKURAI_TAG}} jankurai --locked

bench:
    cargo bench --workspace --no-fail-fast

check-fast:
    CARGO_INCREMENTAL=1 cargo check -p jeryu --tests --locked

test-fast:
    CARGO_INCREMENTAL=1 cargo nextest run -p jeryu --lib --no-fail-fast

medium:
    CARGO_INCREMENTAL=0 cargo check --workspace --message-format=json
    CARGO_INCREMENTAL=0 cargo nextest run -p jeryu --lib
    CARGO_INCREMENTAL=0 cargo test -p jeryu --tests -- --test-threads=1
    cargo run -p cargo-witness -- build
    cargo run -p cargo-vrc -- map --output-dir .

state-proof:
    cargo run -p jeryu -- repo redline-state-proof

deep:
    cargo nextest run -p jeryu
    cargo run -p cargo-witness -- diagnose

security:
    bash tools/security-lane.sh .

dependency-check:
    ./tools/check-dependencies.sh

release:
    cargo build --release -p jeryu
    cargo run -p cargo-aer -- scan --output aer-findings.json
    cargo run -p cargo-vrc -- map --output-dir .

tui-screenshots:
    scripts/capture-tui-screenshots.sh

tui-screenshot-smoke:
    cargo run --release -p tui-capture -- --cols 48 --rows 6 --out target/tui-capture/smoke.png --dump-text target/tui-capture/smoke.txt -- bash -lc "printf '┌────────────────────────┐\n│ Unicode border test    │\n│ Blocks: █ ▓ ▒ ░        │\n└────────────────────────┘\n'; sleep 2"
score:
	jankurai audit . --full --mode advisory --json agent/repo-score.json --md agent/repo-score.md --score-history agent/score-history.jsonl --score-history-csv agent/score-history.csv
doctor:
	jankurai doctor --fail-on high
	jankurai security run . --out target/jankurai/security/evidence.json
rust-map:
	jankurai rust map .
rust-witness:
	jankurai rust witness build .
rust-diagnose:
	jankurai rust diagnose .
check: fast score security rust-map rust-witness rust-diagnose
# jankurai scaffold Justfile

# ============================================================
# Evidence Gate / VibeGate Delivery Spine — local recipes
# ============================================================
# These are pre-PR / developer-machine only. CI never calls them.

# Run the full local CI parity script — mirrors what remote CI runs.
# If this exits 0, you can push to PR with full confidence.
ci-parity:
	bash scripts/ci-parity.sh

# Same as ci-parity but skip slow checks (integration tests, jankurai audit).
ci-parity-fast:
	bash scripts/ci-parity.sh --fast --no-audit

# Gate the current branch with ci-parity, then push and open a PR.
publish-pr base="main" remote="origin":
	branch="$(git branch --show-current)"
	bash scripts/publish-pr.sh --remote "{{remote}}" --branch "$branch" --base "{{base}}" -- gh pr create --base "{{base}}" --head "$branch" --fill

# Run the autonomy-only unit tests (no network).
autonomy-fast:
	cargo test -p jeryu --lib autonomy:: llm:: agent_review:: approval:: -- --test-threads=4

# Run the mock end-to-end pipeline test (no network).
autonomy-e2e:
	cargo test --test autonomy_e2e

# Run ALL live LLM tests against keys in env / ~/.jeryu/secrets/llm.env / ~/llm.env.
# Refuses to run if $CI=true. Pre-PR only.
live:
	./scripts/local-live.sh all

# Single-shot live sub-targets.
live-smoke:
	./scripts/local-live.sh smoke

live-doctor:
	./scripts/local-live.sh doctor

live-e2e:
	./scripts/local-live.sh e2e

live-github:
	./scripts/local-live.sh github

# Full pre-PR check: compile -> unit -> mock e2e -> live.
# Run this before opening any PR that touches the Evidence Gate spine.
pre-pr:
	./scripts/pre-pr.sh

# Quick CLI demo: probe every configured LLM provider.
autonomy-doctor:
	cargo run --quiet --bin autonomy -- doctor

# Quick CLI demo: review a diff piped on stdin.
# Example:
#   git diff origin/main | just autonomy-review-stdin
autonomy-review-stdin head_sha="0000000000000000000000000000000000000000" policy_sha="cccccccccccccccccccccccccccccccccccccccc":
	cargo run --quiet --bin autonomy -- review \
		--head-sha {{head_sha}} --policy-sha {{policy_sha}} \
		--target-branch main --evidence-pack-id evp_cli
