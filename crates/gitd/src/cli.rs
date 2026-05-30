//! Command line interface.

use crate::audit::append_receipt;
use crate::config::GitdConfig;
use crate::error::{GitdError, Result};
use crate::hooks::PreReceiveGuard;
use crate::lfs::LfsStore;
use crate::mirror::MirrorService;
use crate::object_fsck::ObjectFsck;
use crate::refs::RefService;
use crate::repo::{RepoId, RepoManager};
use crate::smart_http::SmartHttpServer;
use crate::ssh;
use std::path::PathBuf;

/// Run the CLI with process args.
pub fn run() -> Result<i32> {
    run_with_args(std::env::args().collect())
}

/// Run the CLI with explicit args for tests.
pub fn run_with_args(args: Vec<String>) -> Result<i32> {
    if args.len() < 2 {
        print_help();
        return Ok(2);
    }
    match args[1].as_str() {
        "init-repo" => {
            let root = value_after(&args, "--root")?;
            let owner = positional(&args, 0)?;
            let repo = positional(&args, 1)?;
            let manager = RepoManager::new(GitdConfig::new(root));
            let id = RepoId::new(owner, repo)?;
            let created = manager.create_bare(&id)?;
            append_receipt(&created.path, "repo.create", &id.to_string())?;
            println!("{}", created.path.display());
            Ok(0)
        }
        "serve-http" => {
            let root = value_after(&args, "--root")?;
            let addr = optional_value_after(&args, "--addr")
                .unwrap_or_else(|| "127.0.0.1:8080".to_string());
            let mut config = GitdConfig::new(root);
            config.http_addr = addr;
            let server = SmartHttpServer::new(RepoManager::new(config.clone()));
            server.serve(&config.http_addr)?;
            Ok(0)
        }
        "refs" => {
            let root = value_after(&args, "--root")?;
            let owner = positional(&args, 0)?;
            let repo_name = positional(&args, 1)?;
            let manager = RepoManager::new(GitdConfig::new(root));
            let repo = manager.open_parts(&owner, &repo_name)?;
            let service = RefService::new(manager);
            for r in service.list_refs(&repo)? {
                println!("{} {}", r.oid, r.name);
            }
            Ok(0)
        }
        "pre-receive" => {
            let root = value_after(&args, "--root")?;
            let owner = positional(&args, 0)?;
            let repo_name = positional(&args, 1)?;
            let actor =
                optional_value_after(&args, "--actor").unwrap_or_else(|| "unknown".to_string());
            let config = GitdConfig::new(root);
            let manager = RepoManager::new(config.clone());
            let repo = manager.open_parts(&owner, &repo_name)?;
            let guard =
                PreReceiveGuard::new(config.protected_refs, ObjectFsck::new(config.git_bin));
            let changes = guard.evaluate_stdin(&repo, &actor)?;
            append_receipt(
                &repo.path,
                "pre_receive.accept",
                &format!("actor={actor} changes={}", changes.len()),
            )?;
            Ok(0)
        }
        "ssh-exec" => {
            let root = value_after(&args, "--root")?;
            let config = GitdConfig::new(&root);
            ssh::exec_from_env(&PathBuf::from(root), &config.git_bin)
        }
        "mirror-clone" => {
            let root = value_after(&args, "--root")?;
            let remote = value_after(&args, "--remote")?;
            let owner = positional(&args, 0)?;
            let repo_name = positional(&args, 1)?;
            let config = GitdConfig::new(root);
            let manager = RepoManager::new(config);
            let service = MirrorService::new(manager);
            let repo = service.mirror_clone(&remote, &RepoId::new(owner, repo_name)?)?;
            append_receipt(&repo.path, "mirror.clone", &remote)?;
            println!("{}", repo.path.display());
            Ok(0)
        }
        "lfs-put" => {
            let root = value_after(&args, "--root")?;
            let oid = value_after(&args, "--oid")?;
            let file = value_after(&args, "--file")?;
            let owner = positional(&args, 0)?;
            let repo_name = positional(&args, 1)?;
            let manager = RepoManager::new(GitdConfig::new(root));
            let repo = manager.open_parts(&owner, &repo_name)?;
            let bytes = std::fs::read(&file)?;
            LfsStore::for_repo(&repo.path).put_bytes(&oid, &bytes)?;
            append_receipt(&repo.path, "lfs.put", &oid)?;
            Ok(0)
        }
        "lfs-get" => {
            let root = value_after(&args, "--root")?;
            let oid = value_after(&args, "--oid")?;
            let owner = positional(&args, 0)?;
            let repo_name = positional(&args, 1)?;
            let manager = RepoManager::new(GitdConfig::new(root));
            let repo = manager.open_parts(&owner, &repo_name)?;
            let bytes = LfsStore::for_repo(&repo.path).get(&oid)?;
            use std::io::Write;
            std::io::stdout().write_all(&bytes)?;
            Ok(0)
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(0)
        }
        other => Err(GitdError::InvalidInput(format!("unknown command: {other}"))),
    }
}

fn value_after(args: &[String], flag: &str) -> Result<String> {
    optional_value_after(args, flag)
        .ok_or_else(|| GitdError::InvalidInput(format!("missing required flag {flag}")))
}

fn optional_value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

fn positional(args: &[String], index: usize) -> Result<String> {
    let mut values = Vec::new();
    let mut skip = false;
    for (i, arg) in args.iter().enumerate().skip(2) {
        if skip {
            skip = false;
            continue;
        }
        if arg.starts_with("--") {
            if args.get(i + 1).is_some_and(|v| !v.starts_with("--")) {
                skip = true;
            }
            continue;
        }
        values.push(arg.clone());
    }
    values
        .get(index)
        .cloned()
        .ok_or_else(|| GitdError::InvalidInput(format!("missing positional argument {index}")))
}

fn print_help() {
    eprintln!("gitd phase1 commands:");
    eprintln!("  init-repo --root PATH OWNER REPO");
    eprintln!("  serve-http --root PATH [--addr HOST:PORT]");
    eprintln!("  refs --root PATH OWNER REPO");
    eprintln!("  pre-receive --root PATH [--actor ACTOR] OWNER REPO");
    eprintln!("  ssh-exec --root PATH");
    eprintln!("  mirror-clone --root PATH --remote URL OWNER REPO");
    eprintln!("  lfs-put --root PATH --oid SHA256 --file PATH OWNER REPO");
    eprintln!("  lfs-get --root PATH --oid SHA256 OWNER REPO");
}
