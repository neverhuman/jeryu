//! Owner: `jeryu node` command handler
//! Proof: `cargo test -p jeryu -- commands::node`
//! Invariants:
//!   - `node add` saves config only after a successful probe (or --dry-run).
//!   - `node list` never panics on an empty or malformed config directory.
//!   - `node remove` refuses to delete configs for nodes with active managers
//!     unless --force is passed.

use anyhow::Result;
use jeryu::{node_support, node_types::NodeConfig, runner_backend_remote};

use crate::cli::NodeCommands;

pub(crate) async fn execute_node_command(subcmd: NodeCommands) -> Result<i32> {
    match subcmd {
        NodeCommands::Add {
            target,
            alias,
            ssh_port,
            identity,
            max_managers,
            storage_limit_gb,
            pool_affinity,
            gitlab_url,
            dry_run,
        } => {
            cmd_add(
                target,
                alias,
                ssh_port,
                identity,
                max_managers,
                storage_limit_gb,
                pool_affinity,
                gitlab_url,
                dry_run,
            )
            .await
        }

        NodeCommands::List { json } => cmd_list(json),

        NodeCommands::Remove { alias, force } => cmd_remove(&alias, force).await,

        NodeCommands::Doctor { alias } => cmd_doctor(&alias).await,
    }
}

// ---------------------------------------------------------------------------
// node add
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
async fn cmd_add(
    target: String,
    alias: Option<String>,
    ssh_port: u16,
    identity: Option<String>,
    max_managers: usize,
    storage_limit_gb: f64,
    pool_affinity: Vec<String>,
    gitlab_url: Option<String>,
    dry_run: bool,
) -> Result<i32> {
    let alias = alias
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| NodeConfig::derive_alias(&target));

    if dry_run {
        println!("🔍 [dry-run] Would register node:");
        println!("  alias:             {alias}");
        println!("  target:            {target}");
        println!("  ssh_port:          {ssh_port}");
        println!("  max_managers:      {max_managers}");
        println!("  storage_limit_gb:  {storage_limit_gb}");
        if !pool_affinity.is_empty() {
            println!("  pool_affinity:     {}", pool_affinity.join(", "));
        }
        if let Some(url) = &gitlab_url {
            println!("  gitlab_url:        {url}");
        }
        println!("  [dry-run] No changes made.");
        return Ok(0);
    }

    // Check for existing registration.
    if node_support::node_config_exists(&alias) {
        eprintln!(
            "❌ Node '{alias}' is already registered. Use `jeryu node remove {alias}` first, \
             or choose a different alias with --alias."
        );
        return Ok(1);
    }

    // Build a temporary config for the probe.
    let mut cfg = NodeConfig::new(alias.clone(), target.clone());
    cfg.ssh_port = ssh_port;
    cfg.identity = identity;
    cfg.max_managers = max_managers;
    cfg.storage_limit_gb = storage_limit_gb;
    cfg.pool_affinity = pool_affinity;
    cfg.gitlab_url_override = gitlab_url;

    println!("🔍 Probing node '{alias}' ({target}) ...");
    let probe = runner_backend_remote::probe_node(&cfg).await;

    // Connectivity is mandatory.
    if !probe.reachable {
        eprintln!("❌ Node '{alias}' is not reachable via SSH.");
        eprintln!("   Check that the target is correct, SSH keys are installed, and");
        eprintln!("   the node is online.");
        return Ok(1);
    }

    let os_str = probe
        .os
        .as_deref()
        .unwrap_or("unknown OS");
    let arch_str = probe
        .arch
        .as_deref()
        .unwrap_or("unknown arch");
    println!("  ✓ SSH reachable  ({os_str} / {arch_str})");

    // Docker is required.
    if !probe.docker_ready {
        eprintln!("❌ Docker is not available on node '{alias}'.");
        eprintln!("   Install Docker and ensure the SSH user can run `docker info`.");
        return Ok(1);
    }
    println!("  ✓ Docker available");

    // Disk space guidance.
    match probe.disk_free_gb {
        Some(free) if free < storage_limit_gb => {
            eprintln!(
                "⚠️  Node '{alias}' has only {free:.1} GiB free but storage_limit_gb is \
                 {storage_limit_gb:.1} GiB."
            );
            eprintln!(
                "   The auto-GC will trigger at 90% of storage_limit_gb ({:.1} GiB used).",
                storage_limit_gb * 0.9
            );
            eprintln!("   Consider lowering --storage-limit-gb or freeing disk space.");
        }
        Some(free) => {
            println!("  ✓ Disk free: {free:.1} GiB  (limit: {storage_limit_gb:.1} GiB)");
        }
        None => {
            println!("  ⚠  Could not determine free disk space");
        }
    }

    // Save config.
    node_support::save_node_config(&cfg)?;

    println!();
    println!("✅ Node '{alias}' registered.");
    println!();
    println!("   Pools will route managers to this node automatically.");

    if cfg.pool_affinity.is_empty() {
        println!("   (accepts all pools)");
    } else {
        println!("   (accepts pools: {})", cfg.pool_affinity.join(", "));
    }

    println!();
    println!("   To add another node:   jeryu node add <target>");
    println!("   To list nodes:         jeryu node list");
    println!("   To remove this node:   jeryu node remove {alias}");

    Ok(0)
}

// ---------------------------------------------------------------------------
// node list
// ---------------------------------------------------------------------------

fn cmd_list(json: bool) -> Result<i32> {
    let nodes = node_support::list_node_configs()?;

    if json {
        let entries: Vec<serde_json::Value> = nodes
            .iter()
            .map(|n| {
                serde_json::json!({
                    "alias": n.alias,
                    "target": n.target,
                    "ssh_port": n.ssh_port,
                    "max_managers": n.max_managers,
                    "storage_limit_gb": n.storage_limit_gb,
                    "pool_affinity": n.pool_affinity,
                    "gitlab_url_override": n.gitlab_url_override,
                    "enabled": n.enabled,
                    "created_at_utc": n.created_at_utc,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&entries)?);
        return Ok(0);
    }

    if nodes.is_empty() {
        println!("No remote nodes registered.");
        println!();
        println!("Add one with:  jeryu node add <user@host>");
        return Ok(0);
    }

    println!(
        "{:<14} {:<24} {:<6} {:<8} {:<10} {:<8} {}",
        "ALIAS", "TARGET", "PORT", "MAX/MGR", "STORAGE", "ENABLED", "POOL AFFINITY"
    );
    for n in &nodes {
        let affinity = if n.pool_affinity.is_empty() {
            "(all)".to_string()
        } else {
            n.pool_affinity.join(",")
        };
        println!(
            "{:<14} {:<24} {:<6} {:<8} {:<10} {:<8} {}",
            n.alias,
            n.target,
            n.ssh_port,
            n.max_managers,
            format!("{:.0}GiB", n.storage_limit_gb),
            if n.enabled { "yes" } else { "no" },
            affinity,
        );
    }

    Ok(0)
}

// ---------------------------------------------------------------------------
// node remove
// ---------------------------------------------------------------------------

async fn cmd_remove(alias: &str, force: bool) -> Result<i32> {
    if !node_support::node_config_exists(alias) {
        eprintln!("❌ Node '{alias}' is not registered.");
        return Ok(1);
    }

    if !force {
        // Check if there are active managers on this node.
        // We load the DB to count active managers; if DB unavailable we warn.
        match check_active_managers_on_node(alias).await {
            Ok(count) if count > 0 => {
                eprintln!(
                    "❌ Node '{alias}' has {count} active manager(s) in the DB."
                );
                eprintln!(
                    "   Drain the pool first, or use --force to remove anyway."
                );
                return Ok(1);
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!(
                    "⚠  Could not check DB for active managers ({e}). Use --force to skip this check."
                );
                return Ok(1);
            }
        }
    }

    node_support::delete_node_config(alias)?;
    println!("✅ Node '{alias}' removed.");

    if force {
        println!(
            "⚠  Managers still running on the node were not stopped. \
             Stop them manually with: ssh <target> docker ps --filter label=jeryu.managed=true"
        );
    }

    Ok(0)
}

/// Delegates to `node_support::count_active_managers_on_node` — DB concern
/// is owned by the typed Db adapter; this call-site is domain dispatch only.
async fn check_active_managers_on_node(alias: &str) -> Result<usize> {
    node_support::count_active_managers_on_node(alias).await
}

// ---------------------------------------------------------------------------
// node doctor
// ---------------------------------------------------------------------------

async fn cmd_doctor(alias: &str) -> Result<i32> {
    let cfg = match node_support::load_node_config(alias) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ Could not load node config for '{alias}': {e}");
            return Ok(1);
        }
    };

    println!("🩺 Running diagnostics for node '{alias}' ({}) ...", cfg.target);
    println!();

    let probe = runner_backend_remote::probe_node(&cfg).await;

    // SSH connectivity
    if probe.reachable {
        let os = probe.os.as_deref().unwrap_or("unknown");
        let arch = probe.arch.as_deref().unwrap_or("unknown");
        println!("  ✓ SSH reachable   OS: {os} / {arch}");
    } else {
        println!("  ✗ SSH UNREACHABLE");
        println!();
        println!("  Troubleshooting:");
        println!("    1. Can you SSH manually?  ssh -p {} {}", cfg.ssh_port, cfg.target);
        if let Some(id) = &cfg.identity {
            println!("    2. Identity file: {id}");
        }
        println!("    3. Is the node online? Is port {} open?", cfg.ssh_port);
        return Ok(1);
    }

    // Docker
    if probe.docker_ready {
        println!("  ✓ Docker available");
    } else {
        println!("  ✗ Docker NOT available");
        println!("    Install Docker or grant the SSH user permission to run docker commands.");
    }

    // Disk
    match probe.disk_free_gb {
        Some(free) => {
            let status = if free >= cfg.storage_limit_gb {
                "✓"
            } else if free >= cfg.storage_limit_gb * 0.5 {
                "⚠"
            } else {
                "✗"
            };
            println!(
                "  {status} Disk free: {free:.1} GiB  (limit: {:.1} GiB  GC threshold: {:.1} GiB)",
                cfg.storage_limit_gb,
                cfg.storage_limit_gb * 0.9,
            );
        }
        None => println!("  ? Could not determine free disk space"),
    }

    // Pool affinity
    if cfg.pool_affinity.is_empty() {
        println!("  ✓ Pool affinity: (accepts all pools)");
    } else {
        println!("  ✓ Pool affinity: {}", cfg.pool_affinity.join(", "));
    }

    // GitLab URL override
    if let Some(url) = &cfg.gitlab_url_override {
        println!("  ✓ GitLab URL override: {url}");
    } else {
        println!("  ℹ GitLab URL override: (using control-plane default)");
    }

    // Max managers
    println!("  ✓ Max managers: {}", cfg.max_managers);

    // Active DB managers
    match check_active_managers_on_node(alias).await {
        Ok(n) => println!("  ✓ Active managers in DB: {n} / {}", cfg.max_managers),
        Err(_) => println!("  ? Could not query DB for active managers"),
    }

    println!();
    let all_ok = probe.reachable && probe.docker_ready;
    if all_ok {
        println!("✅ Node '{alias}' is healthy.");
    } else {
        println!("❌ Node '{alias}' has issues (see above).");
        return Ok(1);
    }

    Ok(0)
}
