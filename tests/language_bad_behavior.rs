use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn assert_no_nonblocking_shell_terminators(path: &str) -> Result<()> {
    let contents = fs::read_to_string(path)?;
    assert!(
        !contents.contains("|| true"),
        "{path} still contains a non-blocking shell terminator"
    );
    Ok(())
}

fn assert_no_gitlab_runner_tags(path: &str) -> Result<()> {
    let contents = fs::read_to_string(path)?;
    for (index, line) in contents.lines().enumerate() {
        if line.starts_with("  tags:") {
            bail!(
                "{}:{} defines GitLab runner tags; standard CI must stay untagged",
                path,
                index + 1
            );
        }
    }
    Ok(())
}

fn collect_files_named(root: &Path, filename: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                ".git" | "target" | "node_modules" | ".jeryu" | ".claude"
            ) {
                continue;
            }
            collect_files_named(&path, filename, files)?;
        } else if name == filename {
            files.push(path);
        }
    }
    Ok(())
}

fn dependency_name(dep_name: &str, dep_value: &toml::Value) -> String {
    dep_value
        .as_table()
        .and_then(|table| table.get("package"))
        .and_then(toml::Value::as_str)
        .unwrap_or(dep_name)
        .to_ascii_lowercase()
}

fn dependency_features(dep_value: &toml::Value) -> impl Iterator<Item = String> + '_ {
    dep_value
        .as_table()
        .and_then(|table| table.get("features"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
        .map(str::to_ascii_lowercase)
}

fn assert_manifest_keeps_sqlite_at_backend_boundary(path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading Cargo manifest {}", path.display()))?;
    let manifest: toml::Value = contents
        .parse()
        .with_context(|| format!("parsing Cargo manifest {}", path.display()))?;

    let dependency_tables = [
        manifest.get("dependencies"),
        manifest.get("dev-dependencies"),
        manifest.get("build-dependencies"),
        manifest
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
    ];

    for table in dependency_tables
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_table)
    {
        for (dep_name, dep_value) in table {
            let package = dependency_name(dep_name, dep_value);
            let dep_name = dep_name.to_ascii_lowercase();
            if dep_name.contains("sqlite")
                || package.contains("sqlite")
                || dep_name.contains("postgres")
                || package.contains("postgres")
            {
                bail!(
                    "{} depends on forbidden state-store package `{}` outside the approved SQLx backend boundary",
                    path.display(),
                    package
                );
            }
            if dep_name == "sqlx" || package == "sqlx" {
                for feature in dependency_features(dep_value) {
                    if feature.contains("sqlite") || feature.contains("postgres") {
                        bail!(
                            "{} enables forbidden SQLx feature `{}` outside the approved backend config",
                            path.display(),
                            feature
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

fn collect_guarded_db_sources(files: &mut Vec<PathBuf>) -> Result<()> {
    for root in ["db", "src", "tests"] {
        collect_guarded_db_sources_under(Path::new(root), files)?;
    }
    Ok(())
}

fn collect_guarded_db_sources_under(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("reading {}", root.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if matches!(name.as_ref(), "target" | ".git" | "node_modules") {
                continue;
            }
            collect_guarded_db_sources_under(&path, files)?;
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("rs" | "sql")
        ) && name != "language_bad_behavior.rs"
        {
            files.push(path);
        }
    }
    Ok(())
}

fn assert_no_sqlite_db_fixture(path: &Path) -> Result<()> {
    let sqlite_allowed = matches!(
        path.to_string_lossy().as_ref(),
        "db/config.rs" | "db/state.rs" | "src/db/mod.rs"
    );
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading guarded DB source {}", path.display()))?;
    for forbidden in [
        "sqlite::memory:",
        "sqlite:",
        "sqlx::sqlite",
        "SqlitePool",
        "SqliteConnection",
    ] {
        if contents.contains(forbidden) {
            if sqlite_allowed {
                continue;
            }
            bail!(
                "{} contains `{}`; SQLite is allowed only in the db backend boundary",
                path.display(),
                forbidden
            );
        }
    }
    Ok(())
}

#[test]
fn db_boundary_rejects_ad_hoc_sqlite_fallbacks() -> Result<()> {
    let mut manifests = Vec::new();
    collect_files_named(Path::new("."), "Cargo.toml", &mut manifests)?;
    for manifest in manifests {
        assert_manifest_keeps_sqlite_at_backend_boundary(&manifest)?;
    }

    let mut db_sources = Vec::new();
    collect_guarded_db_sources(&mut db_sources)?;
    for source in db_sources {
        assert_no_sqlite_db_fixture(&source)?;
    }

    write_lane_log(
        "target/jankurai/db-boundary.log",
        "DB boundary verified: SQLite is confined to approved backend surfaces\n",
    )
}

#[test]
fn language_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    assert_no_nonblocking_shell_terminators(".github/workflows/rust.yml")?;

    let log_path = Path::new("target/jankurai/language-bad-behavior.log");
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        log_path,
        "ci and git behavior lane verified: no non-blocking workflow shell terminators\n",
    )?;
    Ok(())
}

fn write_lane_log(path: &str, message: &str) -> Result<()> {
    let log_path = Path::new(path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(log_path, message)?;
    Ok(())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn git_output(repo: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .with_context(|| format!("running git {:?}", args))
}

fn git_stdout(repo: &Path, args: &[&str]) -> Result<String> {
    let output = git_output(repo, args)?;
    if !output.status.success() {
        bail!(
            "git {:?} failed\nstdout: {}\nstderr: {}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn clone_repo_for_lane_test() -> Result<(tempfile::TempDir, PathBuf, String)> {
    let sandbox = tempfile::tempdir().context("creating lane sandbox")?;
    let repo = sandbox.path().join("repo");
    let root = repo_root();
    let output = Command::new("git")
        .arg("clone")
        .arg("--no-local")
        .arg(root.as_os_str())
        .arg(repo.as_os_str())
        .output()
        .context("cloning repository for lane test")?;
    if !output.status.success() {
        bail!(
            "git clone failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::copy(
        root.join("ops/ci/jankurai-lane.sh"),
        repo.join("ops/ci/jankurai-lane.sh"),
    )
    .with_context(|| format!("copying patched lane script into {}", repo.display()))?;
    set_executable(&repo.join("ops/ci/jankurai-lane.sh"))?;
    fs::copy(root.join("ops/ci/lib.sh"), repo.join("ops/ci/lib.sh"))
        .with_context(|| format!("copying shared lane helpers into {}", repo.display()))?;
    let base = git_stdout(&repo, &["rev-parse", "HEAD"])?;
    Ok((sandbox, repo, base))
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn make_jankurai_stub(bin_dir: &Path, log_path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(bin_dir)?;
    let stub = bin_dir.join("jankurai");
    fs::write(
        &stub,
        format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nprintf '%s\\n' \"$*\" >> '{}'\ncase \"${{1:-}}\" in\n  --version)\n    printf 'jankurai 1.5.1\\n'\n    ;;\n esac\n",
            log_path.display()
        ),
    )?;
    set_executable(&stub)?;
    Ok(stub)
}

fn modify_binary_asset(repo: &Path, relative: &str, marker: &[u8]) -> Result<()> {
    let path = repo.join(relative);
    let mut bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    bytes.extend_from_slice(marker);
    fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn commit_change(repo: &Path, message: &str) -> Result<()> {
    let add = git_output(
        repo,
        &["add", "assets/tui-demo.gif", "assets/tui-workflow.png"],
    )?;
    if !add.status.success() {
        bail!(
            "git add failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&add.stdout),
            String::from_utf8_lossy(&add.stderr)
        );
    }
    let commit = git_output(repo, &["commit", "-m", message])?;
    if !commit.status.success() {
        bail!(
            "git commit failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&commit.stdout),
            String::from_utf8_lossy(&commit.stderr)
        );
    }
    Ok(())
}

fn run_proof_lane(repo: &Path, base: &str, stub_dir: &Path, log_path: &Path) -> Result<Output> {
    let mut path_value = stub_dir.as_os_str().to_os_string();
    if let Some(existing) = std::env::var_os("PATH") {
        path_value.push(":");
        path_value.push(existing);
    }
    let output = Command::new("bash")
        .arg(repo.join("ops/ci/jankurai-lane.sh"))
        .arg("proof")
        .current_dir(repo)
        .env("PATH", path_value)
        .env("JANKURAI_CHANGED_FROM", base)
        .env("JANKURAI_STUB_LOG", log_path)
        .output()
        .context("running proof lane")?;
    Ok(output)
}

fn extract_workflow_commands(path: &Path) -> Result<BTreeSet<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let workflow: serde_yaml::Value = serde_yaml::from_str(&text)
        .with_context(|| format!("parsing workflow {}", path.display()))?;
    let mut commands = BTreeSet::new();
    let jobs_key = serde_yaml::Value::String("jobs".to_string());
    let jobs = workflow
        .as_mapping()
        .and_then(|mapping| mapping.get(&jobs_key))
        .and_then(serde_yaml::Value::as_mapping)
        .with_context(|| format!("workflow {} is missing jobs", path.display()))?;
    for job in jobs.values() {
        let steps_key = serde_yaml::Value::String("steps".to_string());
        let steps = job
            .as_mapping()
            .and_then(|mapping| mapping.get(&steps_key))
            .and_then(serde_yaml::Value::as_sequence)
            .with_context(|| format!("workflow {} has a job without steps", path.display()))?;
        for step in steps {
            let run_key = serde_yaml::Value::String("run".to_string());
            let run = match step
                .as_mapping()
                .and_then(|mapping| mapping.get(&run_key))
                .and_then(serde_yaml::Value::as_str)
            {
                Some(run) => run.trim(),
                None => continue,
            };
            if run.contains('\n') {
                continue;
            }
            if let Some(command) = normalize_workflow_command(run) {
                commands.insert(command);
            }
        }
    }
    Ok(commands)
}

fn normalize_workflow_command(run: &str) -> Option<String> {
    let mut tokens = Vec::new();
    for raw in run.split_whitespace() {
        if raw.contains('=') && tokens.is_empty() && raw.chars().next()?.is_ascii_alphabetic() {
            continue;
        }
        if raw.starts_with('"') || raw.starts_with('\'') || raw.starts_with('$') {
            break;
        }
        tokens.push(raw.trim_matches('"').trim_matches('\'').to_string());
    }

    let first = tokens.first()?.clone();
    if !matches!(first.as_str(), "bash" | "actionlint") {
        return None;
    }

    if first == "bash" {
        if let Some(path) = tokens.get_mut(1) {
            *path = normalize_script_path(path);
        }
        let path = tokens.get(1)?.as_str();
        if path == "ops/ci/update-tui-readme-media.sh" {
            return None;
        }
        if !(path.starts_with("ops/ci/") || path.starts_with("scripts/install-")) {
            return None;
        }
    }

    Some(tokens.join(" "))
}

fn normalize_script_path(path: &str) -> String {
    let mut normalized = path.trim_start_matches("./");
    while let Some(stripped) = normalized.strip_prefix("../") {
        normalized = stripped;
    }
    normalized.to_string()
}

#[test]
fn ci_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    assert_no_gitlab_runner_tags(".gitlab-ci.yml")?;
    assert!(jeryu::ci_failure::is_source_fetch_auth_failure(
        "Getting source from Git repository\nremote: HTTP Basic: Access denied\nfatal: Authentication failed"
    ));
    write_lane_log(
        "target/jankurai/ci-bad-behavior.log",
        "ci bad behavior lane verified: workflow shell terminators are blocking; standard GitLab CI has no runner tags; source-fetch auth failures classify as infrastructure\n",
    )
}

#[test]
fn git_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/git-bad-behavior.log",
        "git bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}

#[test]
fn release_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/release-bad-behavior.log",
        "release bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}

#[test]
fn proof_lane_skips_binary_readme_assets_and_writes_receipts() -> Result<()> {
    let (sandbox, repo, base) = clone_repo_for_lane_test()?;
    let config_email = git_output(&repo, &["config", "user.email", "ci@example.invalid"])?;
    assert!(
        config_email.status.success(),
        "git config user.email failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&config_email.stdout),
        String::from_utf8_lossy(&config_email.stderr)
    );
    let config_name = git_output(&repo, &["config", "user.name", "CI Proof Lane"])?;
    assert!(
        config_name.status.success(),
        "git config user.name failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&config_name.stdout),
        String::from_utf8_lossy(&config_name.stderr)
    );

    modify_binary_asset(&repo, "assets/tui-demo.gif", b"proof-lane-regression-gif")?;
    modify_binary_asset(
        &repo,
        "assets/tui-workflow.png",
        b"proof-lane-regression-png",
    )?;
    commit_change(&repo, "test: binary assets for proof lane")?;

    let log_path = sandbox.path().join("jankurai-stub.log");
    let stub_dir = sandbox.path().join("bin");
    make_jankurai_stub(&stub_dir, &log_path)?;

    let output = run_proof_lane(&repo, &base, &stub_dir, &log_path)?;
    assert!(
        output.status.success(),
        "proof lane failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("proofbind skip generated binary artifact: assets/tui-demo.gif"),
        "proof lane did not report the GIF skip:\n{stderr}"
    );
    assert!(
        stderr.contains("proofbind skip generated binary artifact: assets/tui-workflow.png"),
        "proof lane did not report the PNG skip:\n{stderr}"
    );
    assert!(
        stderr.contains("proofbind changed set has no text inputs after binary filtering"),
        "proof lane did not write the empty-proofbind advisory:\n{stderr}"
    );

    let proofbind_dir = repo.join("target/jankurai/proofbind");
    let witness: serde_json::Value = serde_json::from_str(&fs::read_to_string(
        proofbind_dir.join("surface-witness.json"),
    )?)?;
    let obligations: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(proofbind_dir.join("obligations.json"))?)?;
    let proofbind_md = fs::read_to_string(proofbind_dir.join("proofbind.md"))?;

    assert_eq!(
        witness["changed_paths"].as_array().map(|paths| paths.len()),
        Some(0),
        "binary-only proof lane should not surface changed text paths"
    );
    assert_eq!(witness["summary"]["verdict"], "pass");
    assert_eq!(
        obligations["obligations"]
            .as_array()
            .map(|items| items.len()),
        Some(0)
    );
    assert_eq!(obligations["summary"]["verdict"], "pass");
    assert!(
        proofbind_md.contains("No UTF-8 changed files remained after binary artifact filtering."),
        "proofbind markdown did not record the binary-only skip:\n{proofbind_md}"
    );

    let stub_log = fs::read_to_string(&log_path)?;
    assert!(
        !stub_log.contains("proofbind verify"),
        "proofbind verify should not run when only binary assets change:\n{stub_log}"
    );
    assert!(
        stub_log.contains("proofmark rust"),
        "proofmark should still run after empty proofbind output:\n{stub_log}"
    );
    assert!(
        stub_log.contains("rust witness build"),
        "rust witness build should still run after empty proofbind output:\n{stub_log}"
    );

    Ok(())
}

#[test]
fn ci_parity_covers_the_workflow_command_surface() -> Result<()> {
    let root = repo_root();
    let parity_script = fs::read_to_string(root.join("scripts/ci-parity.sh"))?;
    let mut workflow_commands = BTreeSet::new();

    for workflow in [
        root.join(".github/workflows/jankurai.yml"),
        root.join(".github/workflows/rust.yml"),
    ] {
        workflow_commands.extend(extract_workflow_commands(&workflow)?);
    }

    let missing: Vec<_> = workflow_commands
        .into_iter()
        .filter(|command| !parity_script.contains(command))
        .collect();

    assert!(
        missing.is_empty(),
        "scripts/ci-parity.sh is missing workflow commands: {missing:#?}"
    );

    write_lane_log(
        "target/jankurai/ci-parity-drift.log",
        "ci parity workflow command surface matches the local gate\n",
    )
}
