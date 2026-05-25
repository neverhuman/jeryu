use crate::cli::SecretsCommands;
use anyhow::Result;
use jeryu::{secrets, state};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
struct VaultDoctorReport {
    status: VaultStatusView,
    issues: Vec<String>,
    ok: bool,
}

#[derive(Debug, Serialize)]
struct VaultStatusView {
    addr: String,
    reachable: bool,
    initialized: bool,
    sealed: bool,
    healthy: bool,
    token_present: bool,
    mount: String,
    prefix: String,
    bootstrap_file: String,
    env_file: String,
}

impl From<(&secrets::VaultStatusReport, bool)> for VaultStatusView {
    fn from((report, reachable): (&secrets::VaultStatusReport, bool)) -> Self {
        Self {
            addr: report.addr.clone(),
            reachable,
            initialized: report.initialized,
            sealed: report.sealed,
            healthy: report.healthy,
            token_present: report.token_present,
            mount: report.mount.clone(),
            prefix: report.prefix.clone(),
            bootstrap_file: report.bootstrap_file.clone(),
            env_file: report.env_file.clone(),
        }
    }
}

pub(crate) async fn execute_secrets_commands(subcmd: SecretsCommands) -> Result<()> {
    let db = state::Db::open().await?;
    match subcmd {
        SecretsCommands::Provision { json } => {
            let report = secrets::run_secrets_provision(Some(&db)).await?;
            let status = VaultStatusView::from((&report, report.is_reachable()));
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_vault_status("jeryu secrets provision", &status);
            }
        }
        SecretsCommands::Status { json } => {
            let observed = secrets::vault_status_observed(Some(&db)).await?;
            let status = VaultStatusView::from((&observed.report, observed.reachable));
            if json {
                println!("{}", serde_json::to_string_pretty(&status)?);
            } else {
                print_vault_status("jeryu secrets status", &status);
                print_latest_secret_set(&db).await?;
            }
        }
        SecretsCommands::Doctor { json } => {
            let observed = secrets::vault_status_observed(Some(&db)).await?;
            let status = VaultStatusView::from((&observed.report, observed.reachable));
            let issues = vault_doctor_issues(&status);
            let doctor = VaultDoctorReport {
                status,
                ok: issues.is_empty(),
                issues,
            };
            if json {
                println!("{}", serde_json::to_string_pretty(&doctor)?);
            } else {
                print_vault_status("jeryu secrets doctor", &doctor.status);
                if doctor.ok {
                    println!("  Issues:      none");
                } else {
                    println!("  Issues:");
                    for issue in &doctor.issues {
                        println!("    - {}", issue);
                    }
                }
            }
            if !doctor.ok {
                anyhow::bail!("Vault doctor found {} issue(s)", doctor.issues.len());
            }
        }
        SecretsCommands::Rotate {
            repo,
            repo_root,
            version,
            target,
        } => {
            let target = target.parse::<secrets::SecretTarget>()?;
            let (repo_root_path, deploy_env, runtime_env) =
                secrets::release_paths(repo_root.as_deref());
            let repo_name = resolve_repo_name(&repo, repo_root_path.as_path());
            let outcome = secrets::rotate_release_secrets(
                &db,
                &repo_root_path,
                &repo_name,
                &version,
                target,
                &deploy_env,
                &runtime_env,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        }
        SecretsCommands::Finalize {
            repo,
            repo_root,
            version,
            target,
        } => {
            let target = target.parse::<secrets::SecretTarget>()?;
            let (repo_root_path, deploy_env, runtime_env) =
                secrets::release_paths(repo_root.as_deref());
            let repo_name = resolve_repo_name(&repo, repo_root_path.as_path());
            let path = secrets::finalize_release_secrets(
                &db,
                &repo_root_path,
                &repo_name,
                &version,
                target,
                &deploy_env,
                &runtime_env,
            )
            .await?;
            println!("Finalized runtime env: {}", path.display());
        }
        SecretsCommands::Report {
            repo,
            repo_root,
            version,
        } => {
            let (repo_root_path, _, _) = secrets::release_paths(repo_root.as_deref());
            let repo_name = resolve_repo_name(&repo, repo_root_path.as_path());
            let path =
                secrets::build_release_secret_report(&db, &repo_root_path, &repo_name, &version)
                    .await?;
            println!("Release report: {}", path.display());
        }
        SecretsCommands::Recover {
            repo,
            repo_root,
            version,
        } => {
            let (repo_root_path, _, _) = secrets::release_paths(repo_root.as_deref());
            let repo_name = resolve_repo_name(&repo, repo_root_path.as_path());
            secrets::recover_release_secrets(&db, &repo_root_path, &repo_name, &version).await?;
        }
    }
    Ok(())
}

fn resolve_repo_name(repo: &str, repo_root: &Path) -> String {
    let inferred = crate::cli::infer_repo_name();
    if repo != inferred {
        return repo.to_string();
    }
    repo_root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| repo.to_string())
}

fn print_vault_status(title: &str, report: &VaultStatusView) {
    println!("━━━ {title} ━━━");
    println!("  Vault:       {}", report.addr);
    println!("  Reachable:   {}", report.reachable);
    println!("  Initialized: {}", report.initialized);
    println!("  Sealed:      {}", report.sealed);
    println!("  Healthy:     {}", report.healthy);
    println!("  Token:       {}", report.token_present);
    println!("  Mount:       {}", report.mount);
    println!("  Prefix:      {}", report.prefix);
    println!("  Bootstrap:   {}", report.bootstrap_file);
    println!("  Env file:    {}", report.env_file);
}

async fn print_latest_secret_set(db: &state::Db) -> Result<()> {
    if let Some(secret_set) = db
        .latest_release_secret_set(&crate::cli::infer_repo_name())
        .await?
    {
        println!("\n  Latest release secret set:");
        println!("    Version:   {}", secret_set.version);
        println!("    Target:    {}", secret_set.target);
        println!("    Status:    {}", secret_set.status);
        println!("    Runtime:   {}", secret_set.rendered_runtime_env_path);
        if let Some(report_path) = secret_set.report_path {
            println!("    Report:    {}", report_path);
        }
    }
    Ok(())
}

fn vault_doctor_issues(report: &VaultStatusView) -> Vec<String> {
    let mut issues = Vec::new();
    if !report.reachable {
        issues.push(format!(
            "Vault health endpoint is unreachable at {}",
            report.addr
        ));
    }
    if !report.initialized {
        issues.push("Vault is not initialized".to_string());
    }
    if report.sealed {
        issues.push("Vault is sealed".to_string());
    }
    if !report.token_present {
        issues.push("Vault ops token is missing".to_string());
    }
    if !Path::new(&report.bootstrap_file).exists() {
        issues.push(format!(
            "bootstrap material is missing: {}",
            report.bootstrap_file
        ));
    }
    if !Path::new(&report.env_file).exists() {
        issues.push(format!("Vault env file is missing: {}", report.env_file));
    }
    issues
}
