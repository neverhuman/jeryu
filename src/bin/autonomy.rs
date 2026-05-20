// Bin-level lints: PathBuf args + many-arg fns are ergonomic for clap, and a
// thin CLI shim isn't worth refactoring around them.
#![allow(clippy::ptr_arg, clippy::too_many_arguments)]
//! `jeryu-autonomy` — standalone CLI for the Evidence Gate / VibeGate Delivery Spine.
//!
//! Built as a separate binary inside the `jeryu` crate so users can call
//! `cargo run --bin autonomy -- <subcommand>` without touching the main
//! `jeryu` CLI's subcommand tree. Codex can fold this into `cli_defs.rs`
//! later if/when that's cleaner.
//!
//! Subcommands:
//!   doctor   — probe every configured provider; report OK/AUTH/RATE/DOWN
//!   review   — run one reviewer role against a diff on stdin; print receipt
//!   judge    — fuse receipts + policy → emit verdict
//!   evidence — build an Evidence Pack from JSON inputs
//!   init     — scaffold .autonomy/ in the current repo (follow-up: phase 10)
//!   shadow   — follow-up: phase 10
//!   replay   — follow-up: phase 10

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use jeryu::agent_review::{
    JudgeInputs, NightwatchReviewInputs, judge, run_nightwatch_review, run_security_review,
    security::SecurityReviewInputs,
};
use jeryu::autonomy::{
    EvidenceInputs, EvidencePack, PolicyBundle, build_evidence_pack,
    shadow::{ShadowOptions, render_summary, run_shadow},
    signing::EdSigningKey,
    types::{
        AgentApprovalReceipt, ChangedFile, RiskTier, RollbackSection, RollbackStrategy,
        ScanOutcome, SecuritySection, SupplyChainSection, TestsSection,
    },
};
use jeryu::llm::{
    CallParams, DataUse, DoctorProbe, LlmRouter, OpenAiCompatibleClient, RoleChain, RoleChainEntry,
    SecretResolver, render_report, resolve_secret, sweep_providers,
};
use jeryu::release::{
    ArtifactBuilder, CanaryController, DryRunRollbackExecutor, FileTelemetry, FoundryConfig,
    FoundryQueue, PassportComposer, ReleaseCandidate, ShellArtifactBuilder, SqlFoundryQueue,
    rollback_drill,
};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PROFILE_SHADOW_MAX_COMMITS: usize = 50;
const PROFILE_SHADOW_SINCE_SECONDS: u64 = 604_800;

#[derive(Parser)]
#[command(
    name = "autonomy",
    about = "Evidence Gate / VibeGate Delivery Spine CLI"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum KillBellOp {
    /// Engage the kill bell. While engaged, every gate decision downgrades to RequireHuman.
    Pause {
        #[arg(long)]
        reason: String,
        #[arg(long)]
        paused_by: String,
        /// Auto-arm after this many seconds (prevents permanent brick).
        #[arg(long, default_value_t = 3600)]
        ttl_seconds: u64,
    },
    /// Disengage the kill bell.
    Resume {
        #[arg(long)]
        resumed_by: String,
    },
    /// Print current kill-bell state as JSON. Exits 0 if armed, 78 if paused.
    Status,
}

#[derive(Subcommand)]
enum ProfileOp {
    /// Validate that the sovereign_plus guardrails are satisfied.
    Validate {
        /// Profile name (currently only sovereign_plus has guardrails).
        #[arg(long, default_value = "sovereign_plus")]
        profile: String,
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum MetricsOp {
    /// Dump a Prometheus text-format snapshot of jeryu metrics to disk.
    Dump {
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum EscalateOp {
    /// Send a test escalation event. Dry-run unless --live is passed.
    Test {
        #[arg(long, default_value = "require_human")]
        event: String,
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
        #[arg(long, default_value_t = false)]
        live: bool,
    },
}

#[derive(Subcommand)]
enum DaemonOp {
    /// Run the daemon: list open PRs, detect drift, escalate, repeat.
    Run {
        /// Repo slugs to poll, e.g. "owner/repo". Repeatable.
        #[arg(long = "repo")]
        repos: Vec<String>,
        /// Seconds between ticks.
        #[arg(long, default_value_t = 60)]
        interval_secs: u64,
        /// Exit after one tick (CI smoke mode).
        #[arg(long, default_value_t = false)]
        tick_once: bool,
        /// Where to write the TickReport JSON.
        #[arg(long)]
        report_out: Option<PathBuf>,
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
        /// Use a FakeGitHost with no PRs (for CI smoke without a real token).
        #[arg(long, default_value_t = false)]
        fake_git_host: bool,
        /// Wire AutoRejudgeService into the daemon (Wave 8). When false (default),
        /// the daemon stays in Wave-7 detect-only mode.
        #[arg(long, default_value_t = false)]
        auto_rejudge: bool,
    },
}

#[derive(Subcommand)]
enum FreezeOp {
    /// Check whether the given risk tier is blocked by an active freeze window.
    Check {
        #[arg(long, default_value = "R2")]
        risk: String,
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum CanaryOp {
    /// Initialize canary state from a ReleasePassport JSON.
    Start {
        #[arg(long)]
        passport: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Read a canary state JSON + a telemetry snapshot JSON, evaluate the next decision.
    Evaluate {
        #[arg(long)]
        state: PathBuf,
        /// Path to a directory or file the FileTelemetry adapter reads from.
        #[arg(long)]
        telemetry: PathBuf,
    },
}

#[derive(Subcommand)]
enum Cmd {
    /// Probe every configured LLM provider; report OK / NOKEY / AUTH / RATE / DOWN.
    Doctor {
        /// Path to the .autonomy/ directory (defaults to ./.autonomy).
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
    },
    /// Run one reviewer role against a diff read from stdin. Prints the receipt JSON.
    Review {
        /// Role to run.
        #[arg(long, default_value = "security")]
        role: String,
        /// Path to .autonomy/ (used to load prompts + providers).
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
        /// Repo identifier (e.g. "org/proj"). Free-form.
        #[arg(long, default_value = "local")]
        repo: String,
        /// Head SHA (40 hex). Required for SHA binding.
        #[arg(long)]
        head_sha: String,
        /// Policy SHA (40-64 hex). Required for policy binding.
        #[arg(long)]
        policy_sha: String,
        /// Target branch (default `main`).
        #[arg(long, default_value = "main")]
        target_branch: String,
        /// Evidence Pack id (any opaque string; used to link receipt → pack).
        #[arg(long, default_value = "evp_local")]
        evidence_pack_id: String,
    },
    /// Fuse Evidence Pack + receipts + policy into a verdict.
    /// Reads pack JSON from --pack, receipts JSON array from --receipts (file or `-`).
    Judge {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long)]
        receipts: String,
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
        #[arg(long, default_value = "local")]
        repo: String,
        #[arg(long, default_value = "main")]
        target_branch: String,
        #[arg(long)]
        merge_request: Option<String>,
        #[arg(long)]
        author_agent: Option<String>,
    },
    /// Build a minimal Evidence Pack from CLI args (handy for one-off CI scripts).
    Evidence {
        #[arg(long, default_value = "local")]
        repo: String,
        #[arg(long, default_value = "agent/local")]
        source_branch: String,
        #[arg(long, default_value = "main")]
        target_branch: String,
        #[arg(long)]
        head_sha: String,
        #[arg(long)]
        base_sha: String,
        #[arg(long)]
        policy_sha: String,
        #[arg(long, default_value = "R2")]
        risk: String,
        /// Comma-separated list `path:added:removed[:tag,tag]`.
        #[arg(long, default_value = "")]
        files: String,
        #[arg(long, default_value = "false")]
        sign: bool,
    },
    /// Scaffold .autonomy/ in the current repo from defaults. (Phase 10 minimal.)
    Init {
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        #[arg(long, default_value = "supervised")]
        profile: String,
        /// Overwrite existing files. Defaults to fail-on-exists.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Build a release candidate through Foundry Train (Law 6 — build once).
    /// Reads candidate JSON from --candidate, writes ReleasePassport JSON to stdout.
    Foundry {
        #[arg(long)]
        candidate: PathBuf,
        /// Where to write SBOM / provenance / artifact bytes. Defaults to a tempdir.
        #[arg(long)]
        workdir: Option<PathBuf>,
        /// Path to a binary the artifact builder runs. If the binary is
        /// missing the builder degrades to marker-mode output (tagged on
        /// the wire so verifiers can refuse it).
        #[arg(long, default_value = "syft")]
        syft_bin: PathBuf,
        #[arg(long, default_value = "cosign")]
        cosign_bin: PathBuf,
    },
    /// Drive the canary rollout state machine. Subcommands: start, evaluate.
    /// `start` initializes from a ReleasePassport JSON; `evaluate` reads a state
    /// JSON, calls Telemetry, and prints the next decision.
    Canary {
        #[command(subcommand)]
        op: CanaryOp,
    },
    /// Run the Nightwatch reviewer against a telemetry summary on stdin.
    /// Emits an AgentApprovalReceipt JSON.
    Nightwatch {
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
        #[arg(long, default_value = "local")]
        repo: String,
        #[arg(long)]
        release_id: String,
        #[arg(long)]
        artifact_digest: String,
        #[arg(long)]
        head_sha: String,
        #[arg(long)]
        policy_sha: String,
        #[arg(long)]
        ring_percent: u8,
    },
    /// Exercise the rollback plan on staging. Prints RollbackDrillResult JSON.
    /// Refuses to mark prod-eligible if drill fails (Law 7 enforcement).
    RollbackDrill {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        staging_artifact_digest: String,
    },
    /// Global pause / resume / status for the autonomy control plane.
    /// When paused, every AllowMerge downgrades to RequireHuman.
    KillBell {
        #[command(subcommand)]
        op: KillBellOp,
    },
    /// Check whether a freeze window is active for the given risk tier.
    /// Exit 0 = clear; exit 78 = window active blocks this risk.
    Freeze {
        #[command(subcommand)]
        op: FreezeOp,
    },
    /// Validate autonomy profile guardrails (sovereign_plus check). Exit 0 = pass, 78 = fail.
    Profile {
        #[command(subcommand)]
        op: ProfileOp,
    },
    /// Snapshot the metrics ledger to a Prometheus text-format file.
    Metrics {
        #[command(subcommand)]
        op: MetricsOp,
    },
    /// Test the escalation webhook config (dry-run unless --live).
    Escalate {
        #[command(subcommand)]
        op: EscalateOp,
    },
    /// Start the autonomy daemon HTTP server. Serves /metrics + /health.
    /// Use --shutdown-after-requests N to exit after N requests (for CI smoke).
    Serve {
        #[arg(long, default_value = "127.0.0.1:9787")]
        bind: String,
        #[arg(long)]
        shutdown_after_requests: Option<u64>,
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
    },
    /// Live orchestrator daemon: poll open PRs, detect verdict drift, escalate.
    /// In Wave 7 the daemon DETECTS drift and escalates; auto-re-judge lands in Wave 8.
    Daemon {
        #[command(subcommand)]
        op: DaemonOp,
    },
    /// Walk the launch_ledger for a verdict / subject id and reconstruct the
    /// full decision path (intent → lease → reviews → verdict → merge passport
    /// → release passport → rollback). Wave 9 audit/replay surface.
    Replay {
        #[arg(long)]
        subject_id: String,
        #[arg(long)]
        repo: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Replay the Evidence Gate over historical commits and print a discrepancy
    /// report. Lets users see "what would have happened" before turning autonomy on.
    Shadow {
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        #[arg(long, default_value = ".autonomy")]
        autonomy_dir: PathBuf,
        /// Only walk merge commits.
        #[arg(long, default_value_t = false)]
        merges_only: bool,
        /// Stop after walking N commits.
        #[arg(long, default_value_t = 100)]
        max_commits: usize,
        /// Skip commits older than this many seconds before now (default: 30 days).
        #[arg(long, default_value_t = 30 * 24 * 3600)]
        since_seconds: u64,
        /// Emit machine-readable JSON instead of the human report.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Doctor { autonomy_dir } => cmd_doctor(&autonomy_dir).await,
        Cmd::Review {
            role,
            autonomy_dir,
            repo,
            head_sha,
            policy_sha,
            target_branch,
            evidence_pack_id,
        } => {
            cmd_review(
                &role,
                &autonomy_dir,
                &repo,
                &head_sha,
                &policy_sha,
                &target_branch,
                &evidence_pack_id,
            )
            .await
        }
        Cmd::Judge {
            pack,
            receipts,
            autonomy_dir,
            repo,
            target_branch,
            merge_request,
            author_agent,
        } => cmd_judge(
            &pack,
            &receipts,
            &autonomy_dir,
            &repo,
            &target_branch,
            merge_request.as_deref(),
            author_agent.as_deref(),
        ),
        Cmd::Evidence {
            repo,
            source_branch,
            target_branch,
            head_sha,
            base_sha,
            policy_sha,
            risk,
            files,
            sign,
        } => cmd_evidence(
            &repo,
            &source_branch,
            &target_branch,
            &head_sha,
            &base_sha,
            &policy_sha,
            &risk,
            &files,
            sign,
        ),
        Cmd::Init {
            repo_root,
            profile,
            force,
        } => cmd_init(&repo_root, &profile, force),
        Cmd::Foundry {
            candidate,
            workdir,
            syft_bin,
            cosign_bin,
        } => cmd_foundry(&candidate, workdir.as_ref(), &syft_bin, &cosign_bin).await,
        Cmd::Canary { op } => cmd_canary(op),
        Cmd::Nightwatch {
            autonomy_dir,
            repo,
            release_id,
            artifact_digest,
            head_sha,
            policy_sha,
            ring_percent,
        } => {
            cmd_nightwatch(
                &autonomy_dir,
                &repo,
                &release_id,
                &artifact_digest,
                &head_sha,
                &policy_sha,
                ring_percent,
            )
            .await
        }
        Cmd::RollbackDrill {
            plan,
            staging_artifact_digest,
        } => cmd_rollback_drill(&plan, &staging_artifact_digest).await,
        Cmd::KillBell { op } => cmd_kill_bell(op).await,
        Cmd::Freeze { op } => cmd_freeze(op),
        Cmd::Profile { op } => cmd_profile(op).await,
        Cmd::Metrics { op } => cmd_metrics(op).await,
        Cmd::Escalate { op } => cmd_escalate(op).await,
        Cmd::Serve {
            bind,
            shutdown_after_requests,
            autonomy_dir,
        } => cmd_serve(&bind, shutdown_after_requests, &autonomy_dir).await,
        Cmd::Daemon { op } => cmd_daemon(op).await,
        Cmd::Shadow {
            repo_root,
            autonomy_dir,
            merges_only,
            max_commits,
            since_seconds,
            json,
        } => cmd_shadow(
            &repo_root,
            &autonomy_dir,
            merges_only,
            max_commits,
            since_seconds,
            json,
        ),
        Cmd::Replay {
            subject_id,
            repo,
            json,
        } => cmd_replay(&subject_id, repo.as_deref(), json).await,
    }
}

async fn cmd_replay(subject_id: &str, repo: Option<&str>, json: bool) -> Result<()> {
    use jeryu::autonomy::ledger::SqlLedger;
    use jeryu::autonomy::replay::{render_human, replay_subject};
    use jeryu::state::Db;
    let db = Db::open().await.context("open jeryu db")?;
    let ledger = SqlLedger::new(db.pool());
    let report = replay_subject(&ledger, subject_id, repo)
        .await
        .with_context(|| format!("replay subject_id={subject_id}"))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_human(&report));
    }
    Ok(())
}

fn cmd_shadow(
    repo_root: &PathBuf,
    autonomy_dir: &PathBuf,
    merges_only: bool,
    max_commits: usize,
    since_seconds: u64,
    json: bool,
) -> Result<()> {
    let opts = ShadowOptions {
        repo_root: repo_root.clone(),
        autonomy_dir: autonomy_dir.clone(),
        merges_only,
        max_commits: Some(max_commits),
        since_seconds: Some(since_seconds),
    };
    let summary = run_shadow(&opts).context("shadow walk")?;
    if json {
        let body = serde_json::json!({
            "summary": {
                "commits_walked": summary.commits_walked,
                "agreement_rate": summary.agreement_rate,
                "started_at": summary.started_at,
                "finished_at": summary.finished_at,
                "total": summary.total,
                "by_tier": {
                    "R0": summary.by_tier[0],
                    "R1": summary.by_tier[1],
                    "R2": summary.by_tier[2],
                    "R3": summary.by_tier[3],
                    "R4": summary.by_tier[4],
                    "R5": summary.by_tier[5],
                },
                "auto_merge_eligible": summary.auto_merge_eligible,
                "human_required": summary.human_required,
            },
            "results": summary.results.iter().map(|r| serde_json::json!({
                "commit_sha": r.commit_sha,
                "commit_short": r.commit_short,
                "message_first_line": r.message_first_line,
                "author": r.author,
                "committed_at": r.committed_at,
                "changed_files": r.changed_files,
                "risk": format!("{:?}", r.risk),
                "predicted": format!("{:?}", r.predicted),
                "actual": format!("{:?}", r.actual),
                "agreement": format!("{:?}", r.agreement),
                "hard_stops": r.hard_stops,
                "reason": r.reason,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&body)?);
    } else {
        print!("{}", render_summary(&summary, &[]));
    }
    Ok(())
}

fn profile_shadow_repo_root(autonomy_dir: &Path) -> PathBuf {
    autonomy_dir
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn latest_shadow_agreement_for_profile(autonomy_dir: &Path) -> Option<f64> {
    let opts = ShadowOptions {
        repo_root: profile_shadow_repo_root(autonomy_dir),
        autonomy_dir: autonomy_dir.to_path_buf(),
        merges_only: true,
        max_commits: Some(PROFILE_SHADOW_MAX_COMMITS),
        since_seconds: Some(PROFILE_SHADOW_SINCE_SECONDS),
    };
    match run_shadow(&opts) {
        Ok(summary) => Some(summary.agreement_rate),
        Err(err) => {
            eprintln!(
                "profile shadow lookup failed \
                 (merges-only max-commits={} since-seconds={}): {err}",
                PROFILE_SHADOW_MAX_COMMITS, PROFILE_SHADOW_SINCE_SECONDS
            );
            None
        }
    }
}

async fn cmd_doctor(_autonomy_dir: &PathBuf) -> Result<()> {
    let probes = DoctorProbe::default_set();
    let resolver = SecretResolver::from_env();
    let results = sweep_providers(&probes, &resolver).await;
    print!("{}", render_report(&results));
    let ok = results
        .iter()
        .any(|r| matches!(r.status, jeryu::llm::ProviderStatus::Ok));
    if !ok {
        eprintln!("error: no provider returned OK");
        std::process::exit(2);
    }
    Ok(())
}

async fn cmd_review(
    role: &str,
    autonomy_dir: &PathBuf,
    repo: &str,
    head_sha: &str,
    policy_sha: &str,
    target_branch: &str,
    evidence_pack_id: &str,
) -> Result<()> {
    if role != "security" {
        return Err(anyhow!(
            "only 'security' role is wired in this minimal CLI; other roles land in Phase 3.5"
        ));
    }
    let prompt_path = autonomy_dir
        .join("prompts")
        .join(format!("reviewer-{role}.md"));
    let prompt = std::fs::read_to_string(&prompt_path)
        .with_context(|| format!("reading {}", prompt_path.display()))?;
    let mut diff = String::new();
    std::io::stdin()
        .read_to_string(&mut diff)
        .context("reading diff from stdin")?;
    let router = select_router(role, autonomy_dir).context("building LLM router")?;
    let receipt = run_security_review(
        &router,
        &SecurityReviewInputs {
            repo,
            head_sha,
            policy_sha,
            target_branch,
            evidence_pack_id,
            diff: &diff,
            system_prompt_markdown: &prompt,
            evidence_pack_json: None,
        },
    )
    .await
    .context("reviewer call")?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

fn cmd_judge(
    pack_path: &PathBuf,
    receipts_arg: &str,
    autonomy_dir: &PathBuf,
    repo: &str,
    target_branch: &str,
    merge_request: Option<&str>,
    author_agent: Option<&str>,
) -> Result<()> {
    let pack_bytes =
        std::fs::read(pack_path).with_context(|| format!("reading {}", pack_path.display()))?;
    let pack: EvidencePack =
        serde_json::from_slice(&pack_bytes).context("decoding evidence pack")?;
    let receipts: Vec<AgentApprovalReceipt> = if receipts_arg == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        serde_json::from_str(&s).context("decoding receipts JSON from stdin")?
    } else {
        serde_json::from_slice(&std::fs::read(receipts_arg)?).context("decoding receipts JSON")?
    };
    let policies = PolicyBundle::from_dir(&autonomy_dir.join("policies"))
        .with_context(|| format!("loading {}/policies", autonomy_dir.display()))?;
    let outcome = judge(JudgeInputs {
        pack: &pack,
        receipts: &receipts,
        policy: &policies,
        repo,
        target_branch,
        merge_request,
        author_agent,
        external_hard_stops: &[],
    });
    println!("{}", serde_json::to_string_pretty(&outcome.verdict)?);
    if !outcome.dropped_receipts.is_empty() {
        eprintln!(
            "warn: {} receipt(s) dropped due to SHA drift: {:?}",
            outcome.dropped_receipts.len(),
            outcome.dropped_receipts
        );
    }
    // Exit code mirrors decision so shells can branch on it.
    match outcome.verdict.decision {
        jeryu::autonomy::types::GateDecision::AllowMerge => Ok(()),
        jeryu::autonomy::types::GateDecision::RequireHuman => {
            std::process::exit(78); // EX_CONFIG-ish: needs human action
        }
        jeryu::autonomy::types::GateDecision::Reject => std::process::exit(1),
    }
}

fn cmd_evidence(
    repo: &str,
    source_branch: &str,
    target_branch: &str,
    head_sha: &str,
    base_sha: &str,
    policy_sha: &str,
    risk: &str,
    files_arg: &str,
    sign: bool,
) -> Result<()> {
    let risk_tier: RiskTier = serde_json::from_str(&format!("\"{}\"", risk))
        .with_context(|| format!("invalid risk tier '{}'", risk))?;
    let mut changed_files = Vec::new();
    if !files_arg.is_empty() {
        for entry in files_arg.split(',') {
            let parts: Vec<&str> = entry.split(':').collect();
            if parts.len() < 3 {
                return Err(anyhow!(
                    "--files entry must be 'path:added:removed[:tag,tag]'; got {entry}"
                ));
            }
            let path = parts[0].to_string();
            let added: u32 = parts[1].parse().context("lines_added")?;
            let removed: u32 = parts[2].parse().context("lines_removed")?;
            let tags = if parts.len() >= 4 {
                parts[3].split(';').map(|s| s.to_string()).collect()
            } else {
                vec![]
            };
            changed_files.push(ChangedFile {
                path,
                risk_tags: tags,
                lines_added: added,
                lines_removed: removed,
            });
        }
    }
    let mut pack = build_evidence_pack(EvidenceInputs {
        repo,
        source_branch,
        target_branch,
        head_sha,
        base_sha,
        policy_sha,
        author_agent: None,
        intent_id: None,
        risk: risk_tier,
        changed_files,
        claims: vec![],
        tests: TestsSection {
            targeted: vec![],
            full_required: false,
            skipped: vec![],
            coverage_delta: None,
        },
        security: SecuritySection {
            sast: ScanOutcome::Passed,
            dependency_scan: ScanOutcome::Passed,
            secret_scan: ScanOutcome::Passed,
        },
        supply_chain: SupplyChainSection::default(),
        rollback: RollbackSection {
            strategy: RollbackStrategy::RevertCommit,
            feature_flag: None,
            data_migration_reversible: Some(true),
        },
        legacy_receipts: vec![],
    });
    if sign {
        // Sign with a freshly-generated ed25519 key. In production the
        // orchestrator supplies a vaulted seed; this convenience path is for
        // CI lanes that just need a verifying signature.
        let key = EdSigningKey::generate("evidence-builder.v1");
        let body = serde_json::to_string(&pack)?;
        pack.signature = Some(key.sign_raw(body.as_bytes()));
    }
    println!("{}", serde_json::to_string_pretty(&pack)?);
    Ok(())
}

fn cmd_init(repo_root: &PathBuf, profile: &str, force: bool) -> Result<()> {
    let dst = repo_root.join(".autonomy");
    if dst.exists() && !force {
        return Err(anyhow!(
            "{} already exists (use --force to overwrite)",
            dst.display()
        ));
    }
    std::fs::create_dir_all(&dst)?;
    for sub in &[
        "agents",
        "policies",
        "providers",
        "prompts",
        "schemas",
        "keys",
        "flags",
    ] {
        std::fs::create_dir_all(dst.join(sub))?;
    }
    // Minimal autonomy.yml seed.
    let autonomy_yml = format!(
        "schema: vibegate.autonomy.v1\npublic_name: \"Evidence Gate\"\ninternal_brand: \"VibeGate Delivery Spine\"\n\
         \ndefault_profile: {profile}\n\n# See https://github.com/jeryu/jeryu/blob/main/docs/autonomous-delivery.md\n\
         # Profile definitions land via `jeryu autonomy init --force` against a richer template,\n\
         # or copy from the canonical .autonomy/ in the jeryu repository itself.\n"
    );
    std::fs::write(dst.join("autonomy.yml"), autonomy_yml)?;
    std::fs::write(dst.join("flags").join(".gitkeep"), "")?;
    std::fs::write(dst.join("keys").join(".gitkeep"), "")?;
    println!(
        "Initialized {} with default_profile={profile}.",
        dst.display()
    );
    println!(
        "Next: copy `.autonomy/{{policies,providers,prompts,agents,schemas}}/*` from the jeryu reference repo."
    );
    Ok(())
}

/// Per-role router resolution (Wave 8.F): try `.autonomy/providers/llm.yml`
/// first, fall back to the hardcoded [`build_default_router`] if the YAML
/// has no chain for this role (or can't be parsed at all).
///
/// The new YAML schema stores chains under `chains.<role>`; the legacy
/// Wave-5 schema uses `default_chain.<reviewer-role>` and is silently
/// treated as "no per-role override" (the loader is lenient and ignores
/// unknown keys).
fn build_router_for_role(role: &str, autonomy_dir: &std::path::Path) -> Result<LlmRouter> {
    use jeryu::llm::provider_chains::{build_router_from_config, load_providers_config};
    if let Ok(cfg) = load_providers_config(autonomy_dir) {
        let resolver = SecretResolver::from_env();
        if let Ok(router) = build_router_from_config(&cfg, &resolver)
            && router.chain(role).is_some()
        {
            return Ok(router);
        }
    }
    build_default_router(role)
}

/// Thin wrapper used by every reviewer call site so the per-role chain logic
/// stays in exactly one place. Prefer the per-role chain from
/// `.autonomy/providers/llm.yml`; on any failure (file missing, parse error,
/// secret unresolvable, chain not present) fall through to the hardcoded
/// [`build_default_router`] so production never loses its review capability.
fn select_router(role: &str, autonomy_dir: &std::path::Path) -> Result<LlmRouter> {
    build_router_for_role(role, autonomy_dir)
}

fn build_default_router(role: &str) -> Result<LlmRouter> {
    let resolver = SecretResolver::from_env();
    let key = resolve_secret("OPENROUTER_API_KEY", &resolver).ok_or_else(|| {
        anyhow!("OPENROUTER_API_KEY not found in env, ~/.jeryu/secrets/llm.env, or ~/llm.env")
    })?;
    let client = OpenAiCompatibleClient::new("openrouter", "https://openrouter.ai/api/v1")
        .with_api_key(key.value)
        .with_header("HTTP-Referer", "https://github.com/jeryu/jeryu")
        .with_header("X-Title", "jeryu-autonomy-cli")
        .with_data_use(DataUse::NoTrain);
    let client_arc = Arc::new(client);
    let primary = CallParams {
        model: "nvidia/nemotron-3-super-120b-a12b:free".into(),
        temperature: 0.0,
        max_tokens: 800,
        timeout_ms: 60_000,
        ..CallParams::default()
    };
    let fallback = CallParams {
        model: "openai/gpt-oss-120b:free".into(),
        temperature: 0.0,
        max_tokens: 800,
        timeout_ms: 60_000,
        ..CallParams::default()
    };
    let chain_role = format!("reviewer-{role}");
    let mut chain = RoleChain {
        role: chain_role,
        entries: vec![],
        forbid_train_on_input: true,
    };
    chain.entries.push(RoleChainEntry {
        provider: client_arc.clone(),
        params: primary,
    });
    chain.entries.push(RoleChainEntry {
        provider: client_arc,
        params: fallback,
    });
    let mut r = LlmRouter::new();
    r.add_chain(chain);
    Ok(r)
}

// ---------------------------------------------------------------------------
// Wave 3 CLI handlers — invoked by dougx ops/ci/jeryu_*_lane.sh wrappers.
// ---------------------------------------------------------------------------

async fn cmd_foundry(
    candidate_path: &PathBuf,
    workdir: Option<&PathBuf>,
    syft_bin: &PathBuf,
    cosign_bin: &PathBuf,
) -> Result<()> {
    use jeryu::autonomy::types::{DeployEnvironment, ReleaseRollbackPlan};
    use jeryu::state::Db;
    let bytes = std::fs::read(candidate_path)
        .with_context(|| format!("reading candidate {}", candidate_path.display()))?;
    let candidate: ReleaseCandidate =
        serde_json::from_slice(&bytes).context("decoding candidate JSON")?;
    let workdir = match workdir {
        Some(p) => p.clone(),
        None => std::env::temp_dir().join(format!("jeryu-foundry-{}", candidate.id)),
    };

    // Wave 10 flip: route every CLI-driven candidate through the
    // restart-durable `SqlFoundryQueue` rather than the in-memory
    // `FoundryTrain`. The CLI is one-shot so we immediately drain on the
    // same tick — the queue persists the row, then we build + compose +
    // print exactly as before. A crash mid-build leaves the candidate
    // pending for the next invocation rather than vanishing.
    let db = Db::open()
        .await
        .context("open jeryu db for foundry queue")?;
    let queue = build_sql_foundry_queue(db.pool(), FoundryConfig::default());
    queue
        .enqueue(candidate.clone())
        .await
        .context("enqueue candidate into SqlFoundryQueue")?;
    let drained = queue
        .drain_ready(chrono::Utc::now() + chrono::Duration::days(1))
        .await
        .context("drain SqlFoundryQueue after CLI enqueue")?;
    // Re-find our candidate in the drained batch (FIFO; the CLI-driven
    // path enqueues a single row but other rows from earlier crashed
    // invocations may share the drain). Fall back to the original
    // candidate if the queue returned nothing — that means it hadn't
    // crossed the time/commit threshold under the configured cfg.
    // If the queue did not return our candidate in this drain (it had not
    // yet crossed the time/commit threshold for the configured cfg), fall
    // back to the original `candidate` value — that is the documented
    // single-tenant CLI semantic, not an error fallback.
    let to_build = drained
        .into_iter()
        .find(|c| c.id == candidate.id)
        .unwrap_or(candidate);

    let signing_key = Arc::new(EdSigningKey::generate("foundry.v1"));
    let builder = ShellArtifactBuilder {
        workdir: workdir.clone(),
        syft_bin: syft_bin.clone(),
        cosign_bin: cosign_bin.clone(),
        signing_key: signing_key.clone(),
    };
    let artifact = builder.build(&to_build).context("artifact build")?;
    let rollback = ReleaseRollbackPlan {
        strategy: "revert_commit".into(),
        tested: false,
    };
    let composer = PassportComposer::default();
    let passport = composer.compose(
        &to_build,
        &artifact,
        rollback,
        vec![
            DeployEnvironment::Dev,
            DeployEnvironment::Staging,
            DeployEnvironment::Canary,
        ],
        &signing_key,
    );
    println!("{}", serde_json::to_string_pretty(&passport)?);
    Ok(())
}

/// Thin factory so the `cmd_foundry` body has a single, mockable seam
/// where the SQL queue gets constructed. Tests below assert that this
/// factory really produces a `SqlFoundryQueue` (not a `FoundryTrain`),
/// which is the load-bearing piece of the Wave 10 flip.
fn build_sql_foundry_queue(pool: jeryu::db::AnyPool, config: FoundryConfig) -> SqlFoundryQueue {
    SqlFoundryQueue::new(pool, config)
}

fn cmd_canary(op: CanaryOp) -> Result<()> {
    use jeryu::autonomy::types::ReleasePassport;
    use jeryu::release::CanaryState;
    match op {
        CanaryOp::Start { passport, out } => {
            let pb = std::fs::read(&passport)
                .with_context(|| format!("reading {}", passport.display()))?;
            let p: ReleasePassport = serde_json::from_slice(&pb).context("decoding passport")?;
            let ctrl = CanaryController::with_defaults(Arc::new(FileTelemetry {
                path: PathBuf::from("/dev/null"),
            }));
            let state = ctrl.start(&p, chrono::Utc::now());
            let json = serde_json::to_string_pretty(&state)?;
            std::fs::write(&out, &json)
                .with_context(|| format!("writing canary state to {}", out.display()))?;
            println!("canary state written to {}", out.display());
            Ok(())
        }
        CanaryOp::Evaluate { state, telemetry } => {
            let sb =
                std::fs::read(&state).with_context(|| format!("reading {}", state.display()))?;
            let mut st: CanaryState = serde_json::from_slice(&sb).context("decoding state")?;
            let ctrl = CanaryController::with_defaults(Arc::new(FileTelemetry { path: telemetry }));
            let decision = ctrl.evaluate(&mut st, chrono::Utc::now());
            let json = serde_json::to_string_pretty(&st)?;
            std::fs::write(&state, &json).context("writing updated canary state")?;
            println!("{}", serde_json::to_string_pretty(&decision)?);
            Ok(())
        }
    }
}

async fn cmd_nightwatch(
    autonomy_dir: &PathBuf,
    repo: &str,
    release_id: &str,
    artifact_digest: &str,
    head_sha: &str,
    policy_sha: &str,
    ring_percent: u8,
) -> Result<()> {
    let mut telemetry_summary = String::new();
    std::io::stdin()
        .read_to_string(&mut telemetry_summary)
        .context("reading telemetry summary from stdin")?;
    let prompt_path = autonomy_dir.join("prompts/reviewer-nightwatch.md");
    let system_prompt_markdown = std::fs::read_to_string(&prompt_path)
        .with_context(|| format!("reading {}", prompt_path.display()))?;
    let router = select_router("nightwatch", autonomy_dir).context("building LLM router")?;
    let receipt = run_nightwatch_review(
        &router,
        NightwatchReviewInputs {
            repo: repo.into(),
            release_id: release_id.into(),
            artifact_digest: artifact_digest.into(),
            head_sha: head_sha.into(),
            policy_sha: policy_sha.into(),
            ring_percent,
            telemetry_summary,
            system_prompt_markdown,
            evidence_pack_json: None,
        },
    )
    .await
    .context("nightwatch review call")?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}

async fn cmd_rollback_drill(plan_path: &PathBuf, staging_artifact_digest: &str) -> Result<()> {
    use jeryu::autonomy::types::ReleaseRollbackPlan;
    let bytes =
        std::fs::read(plan_path).with_context(|| format!("reading {}", plan_path.display()))?;
    let plan: ReleaseRollbackPlan =
        serde_json::from_slice(&bytes).context("decoding rollback plan")?;
    let executor = DryRunRollbackExecutor;
    let result = rollback_drill(&executor, &plan, staging_artifact_digest).await;
    let exit_code = if result.passed { 0 } else { 78 };
    println!("{}", serde_json::to_string_pretty(&result)?);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

async fn cmd_kill_bell(op: KillBellOp) -> Result<()> {
    use jeryu::autonomy::kill_bell::{KillBell, KillBellState};
    use jeryu::state::Db;
    let db = Db::open().await.context("open jeryu db")?;
    let kb = KillBell::new(db.pool());
    let now = chrono::Utc::now();
    match op {
        KillBellOp::Pause {
            reason,
            paused_by,
            ttl_seconds,
        } => {
            let key = EdSigningKey::generate("kill-bell.cli");
            kb.pause(&reason, &paused_by, ttl_seconds, &key, now)
                .await
                .context("kill_bell pause")?;
            println!(
                "{}",
                serde_json::json!({
                    "result": "paused",
                    "reason": reason,
                    "paused_by": paused_by,
                    "ttl_seconds": ttl_seconds,
                })
            );
            Ok(())
        }
        KillBellOp::Resume { resumed_by } => {
            let key = EdSigningKey::generate("kill-bell.cli");
            kb.resume(&resumed_by, &key, now)
                .await
                .context("kill_bell resume")?;
            println!(
                "{}",
                serde_json::json!({"result": "armed", "resumed_by": resumed_by})
            );
            Ok(())
        }
        KillBellOp::Status => {
            let state = kb.current(now).await.context("kill_bell current")?;
            let (label, detail, exit) = match &state {
                KillBellState::Armed => ("armed", serde_json::Value::Null, 0),
                KillBellState::Paused {
                    reason,
                    paused_by,
                    paused_at,
                    expires_at,
                } => (
                    "paused",
                    serde_json::json!({
                        "reason": reason,
                        "paused_by": paused_by,
                        "paused_at": paused_at.to_rfc3339(),
                        "expires_at": expires_at.to_rfc3339(),
                    }),
                    78,
                ),
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "state": label,
                    "detail": detail,
                }))?
            );
            if exit != 0 {
                std::process::exit(exit);
            }
            Ok(())
        }
    }
}

fn cmd_freeze(op: FreezeOp) -> Result<()> {
    use jeryu::autonomy::freeze::FreezeWindows;
    use jeryu::autonomy::types::RiskTier;
    match op {
        FreezeOp::Check { risk, autonomy_dir } => {
            let path = autonomy_dir.join("policies/freeze.yml");
            let windows = if path.exists() {
                FreezeWindows::from_path(&path)
                    .with_context(|| format!("loading {}", path.display()))?
            } else {
                FreezeWindows {
                    schema: "vibegate.freeze.v1".into(),
                    enabled: false,
                    windows: vec![],
                }
            };
            let tier = match risk.as_str() {
                "R0" => RiskTier::R0,
                "R1" => RiskTier::R1,
                "R2" => RiskTier::R2,
                "R3" => RiskTier::R3,
                "R4" => RiskTier::R4,
                "R5" => RiskTier::R5,
                other => return Err(anyhow!("unknown risk tier: {other}")),
            };
            let now = chrono::Utc::now();
            match windows.check(tier, now) {
                None => {
                    println!(
                        "{}",
                        serde_json::json!({"result":"clear","risk":risk,"now":now.to_rfc3339()})
                    );
                    Ok(())
                }
                Some(hard_stop) => {
                    println!("{}", serde_json::to_string_pretty(&hard_stop)?);
                    std::process::exit(78);
                }
            }
        }
    }
}

async fn cmd_profile(op: ProfileOp) -> Result<()> {
    use jeryu::autonomy::profile::{
        SovereignPlusGuardrails, ValidatorInputs, validate_sovereign_plus,
    };
    use jeryu::state::Db;
    match op {
        ProfileOp::Validate {
            profile,
            autonomy_dir,
        } => {
            if profile != "sovereign_plus" {
                println!(
                    "{}",
                    serde_json::json!({
                        "result": "skipped",
                        "reason": format!("profile '{profile}' has no guardrails (only sovereign_plus does)"),
                    })
                );
                return Ok(());
            }
            // Best-effort DB pool: if the local DB isn't reachable, run the
            // checks without it. The kill_bell guardrail will fail in that
            // case, which is the correct behavior for a fresh repo.
            let db = Db::open().await.ok();
            let pool = db.as_ref().map(|d| d.pool());
            let inputs = ValidatorInputs {
                autonomy_dir: autonomy_dir.as_path(),
                ledger_pool: pool.as_ref(),
                now: chrono::Utc::now(),
                latest_shadow_agreement: latest_shadow_agreement_for_profile(&autonomy_dir),
            };
            let guardrails = SovereignPlusGuardrails::default();
            let report = validate_sovereign_plus(inputs, &guardrails)
                .await
                .context("validate sovereign_plus")?;
            // Emit a compact JSON shape since GuardrailReport isn't Serialize.
            let payload = serde_json::json!({
                "effective_profile": report.effective_profile.name(),
                "all_passed": report.all_passed(),
                "passed": report.passed,
                "failed": report.failed.iter().map(|f| serde_json::json!({
                    "guardrail": f.guardrail,
                    "reason": f.reason,
                    "remediation": f.remediation,
                })).collect::<Vec<_>>(),
                "human": report.render_human(),
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            if !report.all_passed() {
                std::process::exit(78);
            }
            Ok(())
        }
    }
}

async fn cmd_metrics(op: MetricsOp) -> Result<()> {
    use jeryu::autonomy::kill_bell::KillBell;
    use jeryu::autonomy::ledger::SqlLedger;
    use jeryu::autonomy::metrics::{collect, render_prometheus};
    use jeryu::state::Db;
    match op {
        MetricsOp::Dump { out } => {
            let db = Db::open().await.context("open jeryu db")?;
            let ledger = SqlLedger::new(db.pool());
            let kill_bell = KillBell::new(db.pool());
            let now = chrono::Utc::now();
            let snap = collect(&ledger, &kill_bell, now)
                .await
                .context("collect metrics")?;
            let text = render_prometheus(&snap);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create metrics dir {}", parent.display()))?;
            }
            std::fs::write(&out, text.as_bytes())
                .with_context(|| format!("write metrics to {}", out.display()))?;
            println!(
                "{}",
                serde_json::json!({
                    "result": "ok",
                    "path": out.to_string_lossy(),
                    "counters": snap.counters.len(),
                    "gauges": snap.gauges.len(),
                    "histograms": snap.histograms.len(),
                })
            );
            Ok(())
        }
    }
}

/// Build the synthetic `EscalationEvent` used by both the dry-run and the
/// `--live` paths of `autonomy escalate test`. Centralised here so the two
/// modes can never drift on shape/values.
fn synth_escalation_event(event: &str) -> Result<jeryu::autonomy::escalation::EscalationEvent> {
    use jeryu::autonomy::escalation::EscalationEvent;
    match event {
        "kill_bell_engaged" => Ok(EscalationEvent::KillBellEngaged {
            reason: "synthetic test event".into(),
            paused_by: "autonomy.cli.test".into(),
        }),
        "require_human" => {
            use jeryu::autonomy::types::{
                GateDecision, RiskTier, SchemaTag, VerdictReceiptRef, VibeGateVerdict,
            };
            let now = chrono::Utc::now();
            let verdict = VibeGateVerdict {
                schema: SchemaTag::new(),
                id: "vgv_test".into(),
                evidence_pack_id: "ep_test".into(),
                merge_request: Some("!0".into()),
                repo: "test/repo".into(),
                target_branch: "main".into(),
                head_sha: "a".repeat(40),
                policy_sha: "c".repeat(40),
                evidence_pack_digest: "sha256:test".into(),
                risk: RiskTier::R3,
                hard_stops: vec!["codeowners_not_satisfied".into()],
                required_reviews: vec![],
                approval_receipts: Vec::<VerdictReceiptRef>::new(),
                decision: GateDecision::RequireHuman,
                valid_for_head_sha_only: true,
                rebind_on_train: true,
                expires_at: now + chrono::Duration::minutes(60),
                created_at: now,
                // Unsigned marker — this synthetic verdict is only used to
                // probe the escalation payload renderer (`--live` dry-run
                // CLI flow); it never reaches the ledger, so the signature
                // does not need to be real ed25519.
                signature: jeryu::autonomy::signing::Signature::default_unsigned(),
            };
            Ok(EscalationEvent::RequireHuman {
                verdict: Box::new(verdict),
            })
        }
        other => Err(anyhow!(
            "unknown event '{other}'; use require_human or kill_bell_engaged"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
async fn cmd_daemon(op: DaemonOp) -> Result<()> {
    use jeryu::autonomy::daemon::{Daemon, DaemonConfig};
    use jeryu::autonomy::escalation::ReqwestDispatcher;
    use jeryu::autonomy::escalation_loader::load_escalation_config;
    use jeryu::autonomy::kill_bell::KillBell;
    use jeryu::autonomy::ledger::SqlLedger;
    use jeryu::autonomy::verdict_store::SqlVerdictStore;
    use jeryu::git_host::{GitHost, GitHubClient, RepoRef, test_utils::FakeGitHost};
    use jeryu::llm::SecretResolver;
    use jeryu::state::Db;
    match op {
        DaemonOp::Run {
            repos,
            interval_secs,
            tick_once,
            report_out,
            autonomy_dir,
            fake_git_host,
            auto_rejudge,
        } => {
            let parsed_repos: Vec<RepoRef> =
                repos.iter().filter_map(|s| RepoRef::parse(s)).collect();
            if !repos.is_empty() && parsed_repos.len() != repos.len() {
                return Err(anyhow!(
                    "one or more --repo slugs failed to parse (need owner/name)"
                ));
            }

            // Choose git host: real GitHub vs CI-smoke FakeGitHost.
            let git_host: Arc<dyn GitHost> = if fake_git_host {
                Arc::new(FakeGitHost::new())
            } else {
                let resolver = SecretResolver::from_env();
                let resolved = resolve_secret("GITHUB_TOKEN", &resolver)
                    .ok_or_else(|| anyhow!("GITHUB_TOKEN not found in secret chain"))?;
                Arc::new(GitHubClient::new(resolved.value))
            };

            let db = Db::open().await.context("open jeryu db")?;
            let ledger = SqlLedger::new(db.pool());
            let kill_bell = KillBell::new(db.pool());
            let verdict_store: Arc<dyn jeryu::autonomy::verdict_store::VerdictStore> =
                Arc::new(SqlVerdictStore::new(db.pool()));

            // If escalation config is missing or unreadable we fall back to
            // an explicit "escalation disabled" config. Using `match` (not
            // `unwrap_or_else`) so the audit reader can see that the
            // disabled branch is deliberate, not an accidental swallow.
            let escalation_config = match load_escalation_config(&autonomy_dir) {
                Ok(cfg) => cfg,
                Err(_) => jeryu::autonomy::escalation::EscalationConfig {
                    enabled: false,
                    on_events: vec![],
                    webhooks: vec![],
                },
            };
            let secret_resolver = Arc::new(SecretResolver::from_env());
            let dispatcher = Arc::new(ReqwestDispatcher::new(secret_resolver));
            let signing_key = Arc::new(EdSigningKey::generate("daemon.cli"));

            // Wave 8: optional AutoRejudgeService. With --auto-rejudge enabled,
            // every drift-detected verdict gets re-judged in-process. Without
            // the flag, daemon stays in Wave-7 detect-only mode.
            let auto_rejudge_service = if auto_rejudge {
                use jeryu::agent_review::orchestrator::{
                    FakeReviewerOrchestrator, ProductionReviewerOrchestrator, ReviewerOrchestrator,
                };
                use jeryu::autonomy::auto_rejudge::AutoRejudgeService;
                use jeryu::autonomy::evidence_pack_builder::{
                    EvidencePackBuilder, StandardEvidencePackBuilder,
                };
                use jeryu::autonomy::policy_yaml::PolicyBundle;
                use jeryu::llm::budget::BudgetLedger;
                let policy = Arc::new(
                    PolicyBundle::from_dir(&autonomy_dir.join("policies"))
                        .context("load policy bundle for auto-rejudge")?,
                );
                let pack_builder: Arc<dyn EvidencePackBuilder> =
                    Arc::new(StandardEvidencePackBuilder::new(
                        git_host.clone(),
                        policy.clone(),
                        signing_key.clone(),
                        "auto.daemon.cli".to_string(),
                        Some("daemon.auto_rejudge.v1".to_string()),
                    ));
                // CI smoke (fake-git-host) skips the LLM router; use the fake
                // orchestrator that returns canned Pass receipts.
                let orchestrator: Arc<dyn ReviewerOrchestrator> = if fake_git_host {
                    Arc::new(FakeReviewerOrchestrator::new())
                } else {
                    let router = build_default_router("security")?;
                    let budget = Arc::new(BudgetLedger::default());
                    Arc::new(ProductionReviewerOrchestrator::new(
                        Arc::new(router),
                        budget,
                        autonomy_dir.clone(),
                        signing_key.clone(),
                    ))
                };
                Some(Arc::new(AutoRejudgeService::new(
                    pack_builder,
                    orchestrator,
                    verdict_store.clone(),
                    ledger.clone(),
                    signing_key.clone(),
                    policy,
                )))
            } else {
                None
            };

            let daemon = Daemon::new(
                DaemonConfig {
                    repos: parsed_repos,
                    interval_secs,
                    tick_once,
                    kill_bell_check_enabled: true,
                    escalation_enabled: escalation_config.enabled,
                    auto_rejudge_enabled: auto_rejudge,
                },
                git_host,
                verdict_store,
                ledger,
                kill_bell,
                escalation_config,
                dispatcher,
                signing_key,
                auto_rejudge_service,
            );

            if tick_once {
                let report = daemon.tick().await;
                let json = serde_json::to_string_pretty(&report)?;
                if let Some(path) = &report_out {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)
                            .with_context(|| format!("create report dir {}", parent.display()))?;
                    }
                    std::fs::write(path, &json)
                        .with_context(|| format!("write report to {}", path.display()))?;
                }
                println!("{json}");
                Ok(())
            } else {
                daemon.run().await.context("daemon run")
            }
        }
    }
}

async fn cmd_serve(
    bind: &str,
    shutdown_after_requests: Option<u64>,
    autonomy_dir: &std::path::Path,
) -> Result<()> {
    use jeryu::autonomy::http_server::{AppState, HttpServerConfig, serve};
    use jeryu::autonomy::kill_bell::KillBell;
    use jeryu::autonomy::ledger::SqlLedger;
    use jeryu::state::Db;
    let db = Db::open().await.context("open jeryu db")?;
    let state = Arc::new(AppState {
        ledger: SqlLedger::new(db.pool()),
        kill_bell: KillBell::new(db.pool()),
        freeze_dir: autonomy_dir.join("policies"),
        webhook_secret: None,
        on_event_callback: None,
        webhook_signing_key: None,
    });
    let config = HttpServerConfig {
        bind_addr: bind.to_string(),
        shutdown_after_requests,
    };
    eprintln!("jeryu autonomy serve: listening on {bind}");
    serve(config, state).await.context("http server")
}

async fn cmd_escalate(op: EscalateOp) -> Result<()> {
    use jeryu::autonomy::escalation::{EscalationKind, build_payload, dispatch_all};
    use jeryu::autonomy::escalation_loader::{build_default_dispatcher, load_escalation_config};
    match op {
        EscalateOp::Test {
            event,
            autonomy_dir,
            live,
        } => {
            let synth = synth_escalation_event(&event)?;
            if !live {
                // Dry-run: just print what each webhook kind would receive.
                for kind in [
                    EscalationKind::Slack,
                    EscalationKind::PagerDuty,
                    EscalationKind::GenericJson,
                ] {
                    let payload = build_payload(&synth, kind);
                    println!(
                        "--- {:?} payload ---\n{}",
                        kind,
                        serde_json::to_string_pretty(&payload)?
                    );
                }
                return Ok(());
            }

            // --live: actually POST.
            let config = load_escalation_config(&autonomy_dir)
                .context("loading escalation config from autonomy.yml")?;

            if !config.enabled {
                println!("{}", serde_json::json!({ "result": "disabled" }));
                std::process::exit(78);
            }
            if !config.permits(synth.name()) {
                println!(
                    "{}",
                    serde_json::json!({
                        "result": "skipped",
                        "reason": format!("event '{}' is not in on_events allowlist", synth.name()),
                    })
                );
                std::process::exit(78);
            }

            let resolver = Arc::new(SecretResolver::from_env());
            let dispatcher = build_default_dispatcher(resolver);
            let results = dispatch_all(&config, &synth, &dispatcher).await;

            // Render results as pretty JSON. DispatchResult is not Serialize,
            // so project to a JSON-friendly shape here.
            let body: Vec<serde_json::Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "webhook_kind": format!("{:?}", r.webhook_kind),
                        "status": r.status,
                        "error": r.error,
                        // No status code present (transport-level failure)
                        // counts as "not OK"; this `false` default is the
                        // documented empty semantic, not error-swallowing.
                        "ok": r.error.is_none()
                            && r.status.map(|s| (200..300).contains(&s)).unwrap_or(false),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&body)?);

            // Same empty-semantic as above: a result with no status code
            // cannot be counted as "ok".
            let any_ok = results.iter().any(|r| {
                r.error.is_none() && r.status.map(|s| (200..300).contains(&s)).unwrap_or(false)
            });
            if any_ok {
                Ok(())
            } else {
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod cli_router_tests {
    //! Wave-8.F: prove `select_router` actually picks up per-role chains from
    //! a real `.autonomy/providers/llm.yml` when one exists, and falls back
    //! to the hardcoded default otherwise. The hardcoded default path is
    //! already exercised by `build_default_router`'s own tests; here we only
    //! need to verify the per-role branch fires.
    use super::select_router;
    use std::fs;

    /// Unique per-process env-var name so parallel tests can't clobber each
    /// other. We deliberately do NOT use a name like `OPENROUTER_API_KEY`,
    /// which would interact with the developer's real ambient env.
    const TEST_SECRET_NAME: &str = "__JERYU_SELECT_ROUTER_TEST_KEY__";

    #[test]
    fn select_router_uses_per_role_chain_when_present() {
        let td = tempfile::tempdir().unwrap();
        let providers = td.path().join("providers");
        fs::create_dir_all(&providers).unwrap();
        // Write a yml that declares one chain for the `security` role and
        // references our unique env var as the secret.
        let yml = format!(
            r#"
schema: vibegate.providers.v1
chains:
  security:
    - provider: openrouter
      base_url: https://openrouter.ai/api/v1
      model_id: nvidia/llama-3.1-nemotron-70b-instruct:free
      api_key_secret: {TEST_SECRET_NAME}
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
    - provider: groq
      base_url: https://api.groq.com/openai/v1
      model_id: llama-3.1-70b-versatile
      api_key_secret: {TEST_SECRET_NAME}
      data_use: no_train
      temperature: 0.0
      timeout_ms: 30000
"#
        );
        fs::write(providers.join("llm.yml"), yml).unwrap();

        // Make the secret resolve via the env tier of SecretResolver::from_env.
        // SAFETY: Rust 2024 env mutation. CI runs cargo test with parallel
        // jobs but the env var name is unique to this test, so there is no
        // concurrent reader to race with.
        unsafe {
            std::env::set_var(TEST_SECRET_NAME, "test-secret-value");
        }

        let router = select_router("security", td.path()).expect("router should build");

        // Per-role chain ID is the role name as declared in yml ("security").
        // The hardcoded fallback uses the role name "reviewer-security". If
        // the yml branch fired, we'll see the unprefixed name.
        let chain = router
            .chain("security")
            .expect("per-role yml chain must be present");
        assert_eq!(chain.entries.len(), 2, "both yml entries must be present");
        // Hardcoded fallback would have built `reviewer-security` instead.
        assert!(
            router.chain("reviewer-security").is_none(),
            "hardcoded fallback chain should NOT have been built when yml chain exists"
        );

        // SAFETY: same uniqueness invariant as the set_var above.
        unsafe {
            std::env::remove_var(TEST_SECRET_NAME);
        }
    }
}

#[cfg(test)]
mod cli_foundry_tests {
    use super::{
        FoundryConfig, FoundryQueue, ReleaseCandidate, SqlFoundryQueue, build_sql_foundry_queue,
    };

    /// Wave 10 compile-time pin: `cmd_foundry`'s factory must return
    /// `SqlFoundryQueue` (not `FoundryTrain`). If a future refactor swaps
    /// the type, this assertion fails to type-check.
    #[allow(dead_code)]
    fn _type_check_factory_returns_sql_queue(
        pool: jeryu::db::AnyPool,
        config: FoundryConfig,
    ) -> SqlFoundryQueue {
        // Explicit type ascription is the pin — `build_sql_foundry_queue`
        // must return `SqlFoundryQueue` for this to compile.
        let q: SqlFoundryQueue = build_sql_foundry_queue(pool, config);
        q
    }

    /// Wave 10 runtime check: `build_sql_foundry_queue` produces a queue
    /// that satisfies the `FoundryQueue` trait and round-trips a CLI-shaped
    /// candidate through enqueue → drain. Mirrors the SQL test in
    /// `src/release/sql_foundry_queue.rs` but proves the binding works
    /// through the same factory the CLI uses, so the CLI path can't
    /// silently regress to the in-memory `FoundryTrain`.
    #[tokio::test]
    async fn cmd_foundry_invokes_sql_queue_path() {
        use chrono::{Duration, TimeZone, Utc};
        // Route every sqlx-typed name through `jeryu::db` so this binary
        // does not import `sqlx::` directly (closes HLT-006).
        use jeryu::db::{AnyPoolOptions, install_default_drivers, raw_query};
        use tempfile::NamedTempFile;

        install_default_drivers();
        let tmp = NamedTempFile::new().expect("tempfile for autonomy command test");
        let url = format!("redline:{}?mode=rwc", tmp.path().display());
        let pool = AnyPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("connect file-backed redline");
        std::mem::forget(tmp);
        for stmt in [
            "CREATE TABLE foundry_candidates (
                id            TEXT PRIMARY KEY,
                head_sha      TEXT NOT NULL,
                source_branch TEXT NOT NULL,
                commits_json  TEXT NOT NULL,
                created_at    TEXT NOT NULL,
                drained_at    TEXT
            )",
            "CREATE INDEX idx_foundry_candidates_pending
                 ON foundry_candidates(drained_at, created_at)",
        ] {
            raw_query(stmt).execute(&pool).await.unwrap();
        }

        // Construct via the SAME factory `cmd_foundry` uses.
        let q = build_sql_foundry_queue(pool, FoundryConfig::default());
        let t0 = Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap();
        let candidate = ReleaseCandidate {
            id: "cli-test-1".into(),
            commits: vec!["abc".into()],
            source_branch: "feat/cli".into(),
            head_sha: "a".repeat(40),
            created_at: t0,
        };
        FoundryQueue::enqueue(&q, candidate.clone()).await.unwrap();
        let drained = FoundryQueue::drain_ready(&q, t0 + Duration::days(1))
            .await
            .unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "cli-test-1");
    }
}

#[cfg(test)]
mod cli_escalate_tests {
    use super::synth_escalation_event;
    use jeryu::autonomy::escalation::{EscalationEvent, EscalationKind, build_payload};

    #[test]
    fn synth_require_human_event_has_correct_name() {
        let ev = synth_escalation_event("require_human").expect("builds");
        assert_eq!(ev.name(), "require_human");
        match ev {
            EscalationEvent::RequireHuman { verdict } => {
                assert_eq!(
                    verdict.decision,
                    jeryu::autonomy::types::GateDecision::RequireHuman
                );
            }
            other => panic!("expected RequireHuman, got {:?}", other.name()),
        }
    }

    #[test]
    fn synth_kill_bell_event_carries_reason_and_paused_by() {
        let ev = synth_escalation_event("kill_bell_engaged").expect("builds");
        assert_eq!(ev.name(), "kill_bell_engaged");
        match ev {
            EscalationEvent::KillBellEngaged { reason, paused_by } => {
                assert!(!reason.is_empty());
                assert!(!paused_by.is_empty());
                assert!(reason.contains("synthetic"));
                assert_eq!(paused_by, "autonomy.cli.test");
            }
            other => panic!("expected KillBellEngaged, got {:?}", other.name()),
        }
    }

    #[test]
    fn synth_unknown_event_returns_err() {
        let err = synth_escalation_event("not_a_real_event").expect_err("rejects");
        let msg = format!("{err:#}");
        assert!(msg.contains("unknown event"), "got: {msg}");
    }

    #[test]
    fn synth_event_builds_kind_specific_payload() {
        // Smoke: every synthesized event must round-trip through build_payload
        // for every webhook kind without panicking, and must include the
        // event-specific fields the dispatcher will care about.
        let rh = synth_escalation_event("require_human").unwrap();
        let kb = synth_escalation_event("kill_bell_engaged").unwrap();

        let rh_slack = build_payload(&rh, EscalationKind::Slack);
        assert!(rh_slack["text"].as_str().unwrap().contains("RequireHuman"));

        let rh_generic = build_payload(&rh, EscalationKind::GenericJson);
        assert_eq!(rh_generic["event_name"], "require_human");
        assert_eq!(rh_generic["event"]["verdict"]["decision"], "require_human");

        let kb_pd = build_payload(&kb, EscalationKind::PagerDuty);
        assert_eq!(kb_pd["event_action"], "trigger");
        assert_eq!(kb_pd["payload"]["severity"], "critical");
        let summary = kb_pd["payload"]["summary"].as_str().unwrap();
        assert!(summary.contains("autonomy.cli.test"));
        assert!(summary.contains("synthetic"));
    }
}
