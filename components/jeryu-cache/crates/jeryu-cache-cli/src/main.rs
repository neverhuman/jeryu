use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use jeryu_cache_core::{
    CacheKeyMaterial, CacheRequest, ContentAddressedStore, Digest, PolicyEngine,
};
use jeryu_cache_service::{JeryuCache, JeryuCachePaths};

#[derive(Debug, Parser)]
#[command(name = "jeryu_cache", about = "Jeryu Phase 12 cache/CAS operator CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Derive a cache key from JSON key material.
    Key {
        #[arg(long)]
        material: PathBuf,
    },
    /// Store a file in a local content-addressed store.
    Put {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        file: PathBuf,
    },
    /// Restore a verified object from a local content-addressed store.
    Get {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        digest: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Evaluate a JSON cache policy request.
    Policy {
        #[arg(long)]
        request: PathBuf,
    },
    /// Run a smoke test of key, policy, CAS, service, and receipts.
    SelfTest,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Key { material } => {
            let material: CacheKeyMaterial = serde_json::from_slice(&fs::read(material)?)?;
            let key = material.derive_key()?;
            println!("{}", serde_json::to_string_pretty(&key)?);
        }
        Command::Put { root, file } => {
            let cas = ContentAddressedStore::open(root)?;
            let obj = cas.put_file(file)?;
            println!("{}", serde_json::to_string_pretty(&obj)?);
        }
        Command::Get { root, digest, out } => {
            let cas = ContentAddressedStore::open(root)?;
            let digest = Digest::parse(digest)?;
            cas.write_to(&digest, out)?;
            println!("restored {digest}");
        }
        Command::Policy { request } => {
            let request: CacheRequest = serde_json::from_slice(&fs::read(request)?)?;
            let decision = PolicyEngine.evaluate(&request);
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Command::SelfTest => self_test()?,
    }
    Ok(())
}

fn self_test() -> anyhow::Result<()> {
    use jeryu_cache_core::{CacheAction, CacheKeyMaterial, CacheLayer, CacheScope, TrustTier};

    let root = std::env::temp_dir().join(format!("jeryu_cache-self-test-{}", std::process::id()));
    let mut service = JeryuCache::open(JeryuCachePaths {
        cas_root: root.join("cas"),
        receipt_root: root.join("receipts"),
        quarantine_root: root.join("quarantine"),
    })?;
    let d = |s: &str| Digest::from_bytes(s.as_bytes());
    let key = CacheKeyMaterial {
        cache_schema_version: 1,
        tenant_id: "tenant".into(),
        repo_id_or_explicit_shared_scope: "repo".into(),
        trust_tier: TrustTier::T1ProtectedInternal,
        rustc_version: "rustc 1.78.0".into(),
        cargo_version: "cargo 1.78.0".into(),
        toolchain_channel: "stable".into(),
        host_triple: "x86_64-unknown-linux-gnu".into(),
        target_triple: "x86_64-unknown-linux-gnu".into(),
        profile: "dev".into(),
        feature_set: vec!["default".into()],
        rustflags: vec![],
        linker_identity: "lld".into(),
        sysroot_digest: d("sysroot"),
        cargo_lock_subgraph_digest: d("lock"),
        cargo_toml_digest: d("toml"),
        workspace_metadata_digest: d("workspace"),
        crate_source_digest: d("source"),
        build_rs_digest: d("build-rs"),
        build_rs_declared_inputs_digest: d("build-inputs"),
        proc_macro_digest: d("proc"),
        native_deps_digest: d("native"),
        env_allowlist_digest: d("env"),
        runner_rootfs_digest: d("rootfs"),
        sandbox_policy_digest: d("sandbox"),
    }
    .derive_key()?;
    let request = CacheRequest {
        action: CacheAction::Write,
        layer: CacheLayer::L3RepoCompiledCas,
        actor_tier: TrustTier::T1ProtectedInternal,
        source_repo_id: "repo".into(),
        target_repo_id: "repo".into(),
        scope: CacheScope::Repo {
            tenant_id: "tenant".into(),
            repo_id: "repo".into(),
        },
        green_protected_policy: true,
        has_explainable_fingerprint: true,
        has_receipt: true,
        is_release_lane: false,
        is_agent_patch: false,
    };
    service
        .write(request.clone(), key.clone(), b"artifact")
        .context("write should pass")?;
    let mut restore = request;
    restore.action = CacheAction::Restore;
    let outcome = service.restore(restore, &key)?;
    anyhow::ensure!(outcome.hit, "self-test restore should hit");
    println!(
        "jeryu_cache self-test passed with receipt {}",
        outcome.receipt.receipt_id
    );
    Ok(())
}
