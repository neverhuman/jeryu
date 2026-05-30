use cratevault::cache::{CacheKind, CacheScope};
use cratevault::fingerprint::FingerprintInput;
use cratevault::harness::run_adversarial_suite;
use cratevault::ids::{Actor, RepoId, TenantId};
use cratevault::policy::{CacheContext, CachePolicy, TrustTier};
use cratevault::service::CrateVault;
use cratevault::{Result, StoreStatus};
use std::env;
use std::path::PathBuf;

fn main() {
    if let Err(err) = run() {
        eprintln!("cratevault: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("self-test") => {
            let root = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".cratevault-dev"));
            let results = run_adversarial_suite(root)?;
            let mut failed = 0usize;
            for result in &results {
                let marker = if result.passed { "ok" } else { "FAILED" };
                println!("{marker}: {} — {}", result.name, result.detail);
                if !result.passed {
                    failed += 1;
                }
            }
            if failed == 0 {
                println!("phase6 adversarial suite: ok ({} scenarios)", results.len());
                Ok(())
            } else {
                Err(cratevault::VaultError::CachePoisoned(format!(
                    "{failed} adversarial scenarios failed"
                )))
            }
        }
        Some("put-demo") => {
            let root = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".cratevault-dev"));
            let payload = args.get(3).map(String::as_str).unwrap_or("demo artifact");
            let tenant = TenantId::new("tenant")?;
            let repo = RepoId::new("repo")?;
            let actor = Actor::new("cli")?;
            let context = CacheContext::new(
                tenant.clone(),
                repo.clone(),
                TrustTier::ProtectedInternal,
                false,
            );
            let scope = CacheScope::Repo {
                tenant_id: tenant.clone(),
                repo_id: repo.clone(),
            };
            let fingerprint =
                FingerprintInput::for_repo(tenant, repo, TrustTier::ProtectedInternal);
            let vault = CrateVault::open(root, CachePolicy::new())?;
            let outcome = vault.put_cache(
                &context,
                actor,
                CacheKind::CompiledArtifact,
                scope,
                &fingerprint,
                payload.as_bytes(),
                true,
            )?;
            match outcome.status {
                StoreStatus::Trusted => println!("stored trusted cache: {}", outcome.receipt.id),
                StoreStatus::Quarantined => {
                    println!("stored quarantined cache: {}", outcome.receipt.id)
                }
                StoreStatus::Denied => println!("cache write denied: {}", outcome.receipt.id),
            }
            Ok(())
        }
        _ => {
            println!("cratevault phase6");
            println!("usage:");
            println!("  cratevault self-test [root]");
            println!("  cratevault put-demo [root] [payload]");
            Ok(())
        }
    }
}
