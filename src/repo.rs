use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum RepoMode {
    Direct,
    Observed,
    Enforced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum HookMode {
    Off,
    Advisory,
    Enforce,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum HookProfile {
    PrePush,
    PreCommitJankurai,
    All,
}

pub struct DirectRepoOptions {
    pub path: PathBuf,
    pub name: String,
    pub namespace: String,
    pub branch: String,
    pub protect_main: bool,
    pub hooks: HookMode,
    pub replace_origin: bool,
    pub new_repo: bool,
    pub dry_run: bool,
    pub main_relay: bool,
    pub offline_release_remote: Option<String>,
}

#[derive(Clone, Copy, Debug)]
struct JeryuConfigSpec<'a> {
    mode: RepoMode,
    hooks: HookMode,
    namespace: &'a str,
    name: &'a str,
    branch: &'a str,
    protect_main: bool,
    main_relay: bool,
    offline_release_remote: Option<&'a str>,
}

#[derive(Debug, Serialize)]
pub struct DirectRepoPlan {
    pub action: String,
    pub repo_path: String,
    pub namespace: String,
    pub name: String,
    pub branch: String,
    pub remote_name: String,
    pub remote_url: String,
    pub mode: RepoMode,
    pub hooks: HookMode,
    pub protect_main: bool,
    pub main_relay: bool,
    pub offline_release_remote: Option<String>,
    pub dry_run: bool,
    pub steps: Vec<String>,
}

pub async fn install_git_hooks() -> Result<i32> {
    let repo_root = git_repo_root()?;
    configure_git_hooks(&repo_root)?;
    println!("Configured local core.hooksPath to ops/git-hooks");
    Ok(0)
}

pub async fn init_direct_repo(opts: DirectRepoOptions) -> Result<i32> {
    setup_direct_repo(opts).await
}

pub async fn adopt_direct_repo(opts: DirectRepoOptions) -> Result<i32> {
    setup_direct_repo(opts).await
}

pub async fn set_repo_mode(mode: RepoMode) -> Result<i32> {
    let repo_root = git_repo_root()?;
    let hooks = match mode {
        RepoMode::Direct => HookMode::Off,
        RepoMode::Observed => HookMode::Advisory,
        RepoMode::Enforced => HookMode::Enforce,
    };
    write_jeryu_configs(
        &repo_root,
        JeryuConfigSpec {
            mode,
            hooks,
            namespace: "root",
            name: "unknown",
            branch: "main",
            protect_main: true,
            main_relay: false,
            offline_release_remote: None,
        },
    )?;
    configure_hook_mode(&repo_root, hooks, HookProfile::All)?;
    println!("Configured JeRyu repo mode: {mode:?}");
    Ok(0)
}

pub async fn hooks_status() -> Result<i32> {
    let repo_root = git_repo_root()?;
    let hooks_path =
        git_config_get(&repo_root, "core.hooksPath")?.unwrap_or_else(|| "(unset)".into());
    println!("core.hooksPath: {hooks_path}");
    println!(
        "jeryu hook dir: {}",
        repo_root.join(".jeryu/hooks").display()
    );
    Ok(0)
}

pub async fn hooks_enable(mode: HookMode) -> Result<i32> {
    let repo_root = git_repo_root()?;
    configure_hook_mode(&repo_root, mode, HookProfile::All)?;
    println!("Configured local JeRyu hooks: {mode:?}");
    Ok(0)
}

pub async fn hooks_disable() -> Result<i32> {
    let repo_root = git_repo_root()?;
    unset_git_config(&repo_root, "core.hooksPath")?;
    println!("Disabled local JeRyu hooks for this checkout");
    Ok(0)
}

pub async fn hooks_install(profile: HookProfile, mode: HookMode) -> Result<i32> {
    let repo_root = git_repo_root()?;
    configure_hook_mode(&repo_root, mode, profile)?;
    println!("Installed JeRyu hook profile {profile:?} in {mode:?} mode");
    Ok(0)
}

pub async fn jankurai_fast(changed_from: &str) -> Result<i32> {
    let repo_root = git_repo_root()?;
    fs::create_dir_all(repo_root.join("target/jankurai"))?;
    let status = std::process::Command::new("jankurai")
        .current_dir(&repo_root)
        .args([
            "audit",
            ".",
            "--changed-fast",
            "--changed-from",
            changed_from,
            "--mode",
            "advisory",
            "--json",
            "target/jankurai/changed-fast.json",
            "--md",
            "target/jankurai/changed-fast.md",
        ])
        .status()
        .context("running jankurai changed-fast audit")?;
    Ok(status.code().unwrap_or(1))
}

pub async fn state_proof() -> Result<i32> {
    let redlinedb_bin = redlinedb_bin_path();
    validate_redlinedb_bin(&redlinedb_bin)?;

    let version = Command::new(&redlinedb_bin)
        .arg("--version")
        .output()
        .await
        .with_context(|| {
            format!(
                "running {} --version; install or symlink RedlineDB at {}",
                redlinedb_bin.display(),
                default_redlinedb_bin_path().display()
            )
        })?;
    if !version.status.success() {
        bail!(
            "{} --version failed: {}. Install or symlink RedlineDB at {}",
            redlinedb_bin.display(),
            String::from_utf8_lossy(&version.stderr).trim(),
            default_redlinedb_bin_path().display()
        );
    }

    let proof_dir =
        std::env::temp_dir().join(format!("jeryu-redline-proof-{}", std::process::id()));
    fs::create_dir_all(&proof_dir).with_context(|| format!("creating {}", proof_dir.display()))?;
    let proof_db = proof_dir.join("state-proof.redlineDB");
    let url = format!("redline:{}?mode=rwc", proof_db.display());

    let mut test = Command::new("cargo");
    test.args([
        "test",
        "-p",
        "jeryu",
        "state::tests::redline_backend_smoke_test_when_configured",
        "--",
        "--nocapture",
    ]);
    test.env("JERYU_TEST_REDLINE_URL", &url);
    let result = crate::exec::run_status_check(&mut test, "running redline proof test").await;
    if std::env::var("JERYU_KEEP_REDLINE_PROOF").ok().as_deref() != Some("1") {
        let _ = fs::remove_dir_all(&proof_dir);
    }
    result?;
    Ok(0)
}

async fn setup_direct_repo(opts: DirectRepoOptions) -> Result<i32> {
    let mode = match opts.hooks {
        HookMode::Off => RepoMode::Direct,
        HookMode::Advisory => RepoMode::Observed,
        HookMode::Enforce => RepoMode::Enforced,
    };
    let remote_name = if opts.new_repo || opts.replace_origin {
        "origin"
    } else {
        "jeryu"
    };
    let remote_url = local_gitlab_ssh_url(&opts.namespace, &opts.name);
    let plan = DirectRepoPlan {
        action: if opts.new_repo { "init" } else { "adopt" }.into(),
        repo_path: opts.path.display().to_string(),
        namespace: opts.namespace.clone(),
        name: opts.name.clone(),
        branch: opts.branch.clone(),
        remote_name: remote_name.into(),
        remote_url: remote_url.clone(),
        mode,
        hooks: opts.hooks,
        protect_main: opts.protect_main,
        main_relay: opts.main_relay,
        offline_release_remote: opts.offline_release_remote.clone(),
        dry_run: opts.dry_run,
        steps: vec![
            "verify local JeRyu/GitLab endpoints or print recovery command".into(),
            "create or reuse the local GitLab project".into(),
            "configure git remotes without recurring --mirror pushes".into(),
            "write deterministic .jeryu policy files without secrets".into(),
            "configure optional local hooks".into(),
        ],
    };

    if opts.dry_run {
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(0);
    }

    if opts.new_repo {
        fs::create_dir_all(&opts.path)
            .with_context(|| format!("creating {}", opts.path.display()))?;
        if !opts.path.join(".git").is_dir() {
            run_git(&opts.path, &["init", "-b", &opts.branch])?;
        }
    } else if !opts.path.join(".git").is_dir() {
        bail!("{} is not a git checkout", opts.path.display());
    }

    if !local_gitlab_reachable().await {
        bail!(
            "local JeRyu/GitLab is not reachable; recover with `jeryu install server --yes` or `jeryu init`"
        );
    }
    let project = ensure_local_gitlab_project(&opts.namespace, &opts.name).await?;
    if opts.protect_main {
        dotenvy::from_path(crate::config::env_file()).ok();
        let pat = std::env::var("GITLAB_PAT").context(
            "GITLAB_PAT not found; recover with `jeryu bootstrap` or `jeryu install server --yes`",
        )?;
        let client = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:8929", Some(pat));
        client
            .protect_branch_mr_only(project.id, &opts.branch)
            .await
            .with_context(|| format!("protecting branch {}", opts.branch))?;
    }

    configure_remote(&opts.path, remote_name, &remote_url, opts.replace_origin)?;
    if remote_name == "jeryu" {
        run_git(
            &opts.path,
            &["config", "--local", "remote.pushDefault", "jeryu"],
        )?;
    }
    run_git(
        &opts.path,
        &[
            "config",
            "--local",
            &format!("branch.{}.remote", opts.branch),
            remote_name,
        ],
    )?;
    run_git(
        &opts.path,
        &[
            "config",
            "--local",
            &format!("branch.{}.merge", opts.branch),
            &format!("refs/heads/{}", opts.branch),
        ],
    )?;
    write_jeryu_configs(
        &opts.path,
        JeryuConfigSpec {
            mode,
            hooks: opts.hooks,
            namespace: &opts.namespace,
            name: &opts.name,
            branch: &opts.branch,
            protect_main: opts.protect_main,
            main_relay: opts.main_relay,
            offline_release_remote: opts.offline_release_remote.as_deref(),
        },
    )?;
    configure_hook_mode(&opts.path, opts.hooks, HookProfile::All)?;
    println!("Configured direct JeRyu repo at {}", opts.path.display());
    println!("Remote {remote_name}: {remote_url}");
    Ok(0)
}

fn local_gitlab_ssh_url(namespace: &str, name: &str) -> String {
    format!(
        "ssh://git@127.0.0.1:2224/{}/{}.git",
        namespace.trim_matches('/'),
        name.trim_end_matches(".git")
    )
}

async fn local_gitlab_reachable() -> bool {
    let client = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:8929", None);
    client.is_ready().await
}

async fn ensure_local_gitlab_project(
    namespace: &str,
    name: &str,
) -> Result<crate::gitlab_client::Project> {
    dotenvy::from_path(crate::config::env_file()).ok();
    let pat = std::env::var("GITLAB_PAT").context(
        "GITLAB_PAT not found; recover with `jeryu bootstrap` or `jeryu install server --yes`",
    )?;
    let client = crate::gitlab_client::GitlabClient::new("http://127.0.0.1:8929", Some(pat));
    let expected = format!(
        "{}/{}",
        namespace.trim_matches('/'),
        name.trim_end_matches(".git")
    );
    let projects = client.list_projects().await?;
    if let Some(project) = projects
        .into_iter()
        .find(|project| project.path_with_namespace == expected || project.name == name)
    {
        return Ok(project);
    }
    client.create_project(name).await
}

fn configure_remote(
    repo_root: &Path,
    remote_name: &str,
    remote_url: &str,
    replace_origin: bool,
) -> Result<()> {
    if remote_name == "origin" && replace_origin && git_remote_exists(repo_root, "origin") {
        run_git(repo_root, &["remote", "set-url", "origin", remote_url])?;
        return Ok(());
    }
    if git_remote_exists(repo_root, remote_name) {
        run_git(repo_root, &["remote", "set-url", remote_name, remote_url])?;
    } else {
        run_git(repo_root, &["remote", "add", remote_name, remote_url])?;
    }
    Ok(())
}

fn git_remote_exists(repo_root: &Path, name: &str) -> bool {
    std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["remote", "get-url", name])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn write_jeryu_configs(repo_root: &Path, spec: JeryuConfigSpec<'_>) -> Result<()> {
    let dir = repo_root.join(".jeryu");
    fs::create_dir_all(&dir)?;
    write_file_if_changed(
        &dir.join("repo.toml"),
        &format!(
            "schema_version = \"1\"\nmode = \"{}\"\nnamespace = \"{}\"\nname = \"{}\"\ndefault_branch = \"{}\"\nremote_url = \"{}\"\n",
            mode_label(spec.mode),
            spec.namespace,
            spec.name,
            spec.branch,
            local_gitlab_ssh_url(spec.namespace, spec.name)
        ),
    )?;
    write_file_if_changed(
        &dir.join("policy.toml"),
        &format!(
            "schema_version = \"1\"\nprotect_main = {}\nprotected_branches = [\"{}\"]\nprotected_tags = [\"v*\"]\nhooks = \"{}\"\n\n[main_relay]\nenabled = {}\nactor = \"jeryu\"\nprotected_branch = \"{}\"\nrequire_admission_receipt = true\n\n[offline_release_mirror]\nenabled = {}\nremote = \"{}\"\nrefs = [\"refs/tags/v*\", \"refs/heads/release/*\"]\n",
            spec.protect_main,
            spec.branch,
            hook_label(spec.hooks),
            spec.main_relay,
            spec.branch,
            spec.offline_release_remote.is_some(),
            spec.offline_release_remote.unwrap_or("")
        ),
    )?;
    write_file_if_changed(
        &dir.join("backup.toml"),
        "schema_version = \"1\"\nlocal_bundle = true\nbare_mirror = true\nprune_mirror = false\nssh_target = \"\"\nrequire_fresh_backup = \"warn\"\n",
    )?;
    write_file_if_changed(
        &dir.join("ci.toml"),
        "schema_version = \"1\"\ngithub_actions_required = false\nlocal_gitlab_required = true\n",
    )?;
    Ok(())
}

fn write_file_if_changed(path: &Path, content: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(());
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

fn configure_hook_mode(repo_root: &Path, mode: HookMode, profile: HookProfile) -> Result<()> {
    match mode {
        HookMode::Off => {
            unset_git_config(repo_root, "core.hooksPath")?;
        }
        HookMode::Advisory | HookMode::Enforce => {
            let hooks_dir = repo_root.join(".jeryu/hooks");
            fs::create_dir_all(&hooks_dir)?;
            if matches!(profile, HookProfile::PrePush | HookProfile::All) {
                write_executable_hook(&hooks_dir.join("pre-push"), &pre_push_hook(mode))?;
            }
            if matches!(profile, HookProfile::PreCommitJankurai | HookProfile::All) {
                write_executable_hook(
                    &hooks_dir.join("pre-commit"),
                    &jankurai_pre_commit_hook(mode),
                )?;
            }
            run_git(
                repo_root,
                &["config", "--local", "core.hooksPath", ".jeryu/hooks"],
            )?;
        }
    }
    Ok(())
}

fn pre_push_hook(mode: HookMode) -> String {
    let blocking = matches!(mode, HookMode::Enforce);
    format!(
        "#!/bin/sh\nset -u\nREPO_ROOT=\"$(git rev-parse --show-toplevel)\"\ncd \"$REPO_ROOT\"\nbash scripts/ci-parity.sh --fast --no-audit\nstatus=$?\nif [ \"$status\" -ne 0 ]; then\n  echo \"jeryu advisory pre-push failed\" >&2\n  {}\nfi\nexit 0\n",
        if blocking { "exit $status" } else { "exit 0" }
    )
}

fn jankurai_pre_commit_hook(mode: HookMode) -> String {
    let blocking = matches!(mode, HookMode::Enforce);
    format!(
        "#!/bin/sh\nset -u\nmkdir -p target/jankurai\nbase=${{JERYU_JANKURAI_CHANGED_FROM:-origin/main}}\njankurai audit . --changed-fast --changed-from \"$base\" --mode advisory --json target/jankurai/pre-commit-changed-fast.json --md target/jankurai/pre-commit-changed-fast.md\nstatus=$?\nif [ \"$status\" -ne 0 ]; then\n  echo \"jankurai changed-fast guard reported findings\" >&2\n  {}\nfi\nexit 0\n",
        if blocking { "exit $status" } else { "exit 0" }
    )
}

fn write_executable_hook(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .with_context(|| format!("running git {:?}", args))?;
    if !output.status.success() {
        bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn git_config_get(repo_root: &Path, key: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["config", "--local", "--get", key])
        .output()
        .with_context(|| format!("reading git config {key}"))?;
    if output.status.success() {
        Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Ok(None)
    }
}

fn unset_git_config(repo_root: &Path, key: &str) -> Result<()> {
    let _ = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["config", "--local", "--unset-all", key])
        .output();
    Ok(())
}

fn mode_label(mode: RepoMode) -> &'static str {
    match mode {
        RepoMode::Direct => "direct",
        RepoMode::Observed => "observed",
        RepoMode::Enforced => "enforced",
    }
}

fn hook_label(mode: HookMode) -> &'static str {
    match mode {
        HookMode::Off => "off",
        HookMode::Advisory => "advisory",
        HookMode::Enforce => "enforce",
    }
}

pub(crate) fn configure_git_hooks(repo_root: &Path) -> Result<()> {
    let hooks_dir = repo_root.join("ops/git-hooks");
    let pre_push = hooks_dir.join("pre-push");
    if !pre_push.is_file() {
        bail!("repo-managed hook is missing: {}", pre_push.display());
    }

    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["config", "--local", "core.hooksPath", "ops/git-hooks"])
        .output()
        .with_context(|| "configuring repo-managed git hooks".to_string())?;

    if !output.status.success() {
        bail!(
            "failed to configure core.hooksPath: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

fn redlinedb_bin_path() -> PathBuf {
    std::env::var_os("REDLINEDB_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(default_redlinedb_bin_path)
}

fn default_redlinedb_bin_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/ubuntu"))
        .join(".local/bin/redlinedb")
}

fn validate_redlinedb_bin(redlinedb_bin: &Path) -> Result<()> {
    if !redlinedb_bin.is_file() {
        bail!(
            "required RedlineDB binary is missing: {}. Install or symlink RedlineDB at {}",
            redlinedb_bin.display(),
            default_redlinedb_bin_path().display()
        );
    }

    if !is_executable(redlinedb_bin) {
        bail!(
            "required RedlineDB binary is not executable: {}. Install or symlink RedlineDB at {}",
            redlinedb_bin.display(),
            default_redlinedb_bin_path().display()
        );
    }

    Ok(())
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    fs::metadata(path)
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

pub async fn capture_tui_screenshots(output_dir: Option<PathBuf>) -> Result<i32> {
    let root = repo_root()?;
    let output_dir = match output_dir {
        Some(path) => path,
        None => root.join("paper/assets"),
    };
    let debug_dir = root.join("target/tui-capture");
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("creating {}", output_dir.display()))?;
    fs::create_dir_all(&debug_dir).with_context(|| format!("creating {}", debug_dir.display()))?;

    let cols = std::env::var("JERYU_TUI_CAPTURE_COLS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(160);
    let rows = std::env::var("JERYU_TUI_CAPTURE_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(48);
    let font_path = match std::env::var("JERYU_TUI_CAPTURE_FONT") {
        Ok(value) => PathBuf::from(value),
        Err(_) => PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
    };
    let font_size = std::env::var("JERYU_TUI_CAPTURE_FONT_SIZE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(19.0);
    let cell_w = std::env::var("JERYU_TUI_CAPTURE_CELL_W")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);
    let cell_h = std::env::var("JERYU_TUI_CAPTURE_CELL_H")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(23);
    let bg = match std::env::var("JERYU_TUI_CAPTURE_BG") {
        Ok(value) => value,
        Err(_) => "#17212b".to_string(),
    };
    let fg = match std::env::var("JERYU_TUI_CAPTURE_FG") {
        Ok(value) => value,
        Err(_) => "#f4fbff".to_string(),
    };
    let brighten = std::env::var("JERYU_TUI_CAPTURE_BRIGHTEN")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1.35_f32);
    let max_wait_ms = std::env::var("JERYU_TUI_CAPTURE_MAX_WAIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8000_u64);
    let min_wait_ms = std::env::var("JERYU_TUI_CAPTURE_MIN_WAIT_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1200_u64);
    let quiet_ms = std::env::var("JERYU_TUI_CAPTURE_QUIET_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300_u64);

    let mut build = Command::new("cargo");
    build
        .args(["build", "--release", "-p", "jeryu", "-p", "tui-capture"])
        .current_dir(&root);
    crate::exec::run_status_check(&mut build, "building tui-capture assets").await?;

    let shots = [
        ("mission", output_dir.join("jeryu-tui-mission.png")),
        ("jobs", output_dir.join("jeryu-tui-jobs-flow.png")),
        ("agents", output_dir.join("jeryu-tui-agents.png")),
        ("tests", output_dir.join("jeryu-tui-tests-vti.png")),
        ("evidence", output_dir.join("jeryu-tui-evidence.png")),
        ("release", output_dir.join("jeryu-tui-release.png")),
    ];

    for (tab, output) in shots {
        let ready_file = tempfile::NamedTempFile::new().context("creating ready file")?;
        let mut cmd = Command::new(root.join("target/release/tui-capture"));
        cmd.arg("--cols").arg(cols.to_string());
        cmd.arg("--rows").arg(rows.to_string());
        cmd.arg("--out").arg(&output);
        cmd.arg("--font").arg(&font_path);
        cmd.arg("--font-size").arg(font_size.to_string());
        cmd.arg("--cell-w").arg(cell_w.to_string());
        cmd.arg("--cell-h").arg(cell_h.to_string());
        cmd.arg("--bg").arg(&bg);
        cmd.arg("--fg").arg(&fg);
        cmd.arg("--brighten").arg(brighten.to_string());
        cmd.arg("--min-wait-ms").arg(min_wait_ms.to_string());
        cmd.arg("--max-wait-ms").arg(max_wait_ms.to_string());
        cmd.arg("--quiet-ms").arg(quiet_ms.to_string());
        cmd.arg("--ready-file").arg(ready_file.path());
        cmd.arg("--dump-text")
            .arg(debug_dir.join(format!("{tab}.txt")));
        cmd.arg("--");
        cmd.arg(root.join("target/release/jeryu"));
        cmd.arg("tui");
        cmd.arg("--screenshot");
        cmd.arg("--tab").arg(tab);
        cmd.arg("--screenshot-hold-ms").arg("10000");
        crate::exec::run_status_check(&mut cmd, &format!("tui capture failed for {tab}")).await?;
        if !ready_file.path().exists() {
            bail!("TUI did not signal readiness for {tab}");
        }
    }
    Ok(0)
}

fn repo_root() -> Result<PathBuf> {
    let mut dir = std::env::current_dir().context("resolving current directory")?;
    loop {
        if dir.join("Cargo.toml").is_file() {
            return Ok(dir);
        }
        if !dir.pop() {
            bail!("unable to locate repo root");
        }
    }
}

fn git_repo_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("resolving git repository root")?;

    if !output.status.success() {
        bail!(
            "failed to resolve git repository root: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn git(repo_root: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()
            .expect("git command");
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout={}\nstderr={}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn configure_git_hooks_sets_repo_local_hooks_path() {
        let repo = tempdir().expect("temp repo");
        git(repo.path(), &["init"]);

        let hooks_dir = repo.path().join("ops/git-hooks");
        fs::create_dir_all(&hooks_dir).expect("create hooks dir");
        let source_hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("ops/git-hooks/pre-push");
        let target_hook = hooks_dir.join("pre-push");
        fs::copy(&source_hook, &target_hook).expect("copy hook");
        let mut perms = fs::metadata(&target_hook)
            .expect("hook metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_hook, perms).expect("set hook perms");

        configure_git_hooks(repo.path()).expect("configure hooks");

        let output = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["config", "--local", "--get", "core.hooksPath"])
            .output()
            .expect("git config read");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "ops/git-hooks"
        );
        assert!(repo.path().join("ops/git-hooks/pre-push").is_file());
    }

    #[test]
    fn direct_mode_unsets_local_hooks_path() {
        let repo = tempdir().expect("temp repo");
        git(repo.path(), &["init"]);
        git(
            repo.path(),
            &["config", "--local", "core.hooksPath", "ops/git-hooks"],
        );

        configure_hook_mode(repo.path(), HookMode::Off, HookProfile::All).unwrap();

        let output = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["config", "--local", "--get", "core.hooksPath"])
            .output()
            .expect("git config read");
        assert!(!output.status.success());
    }

    #[test]
    fn observed_and_enforced_modes_install_expected_hooks() {
        let repo = tempdir().expect("temp repo");
        git(repo.path(), &["init"]);

        configure_hook_mode(
            repo.path(),
            HookMode::Advisory,
            HookProfile::PreCommitJankurai,
        )
        .unwrap();
        let pre_commit = fs::read_to_string(repo.path().join(".jeryu/hooks/pre-commit")).unwrap();
        assert!(pre_commit.contains("jankurai audit . --changed-fast"));
        assert!(pre_commit.contains("exit 0"));

        configure_hook_mode(repo.path(), HookMode::Enforce, HookProfile::PrePush).unwrap();
        let pre_push = fs::read_to_string(repo.path().join(".jeryu/hooks/pre-push")).unwrap();
        assert!(pre_push.contains("ci-parity.sh"));
        assert!(pre_push.contains("--fast --no-audit"));
        assert!(pre_push.contains("exit $status"));

        let output = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["config", "--local", "--get", "core.hooksPath"])
            .output()
            .expect("git config read");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            ".jeryu/hooks"
        );
    }

    #[test]
    fn existing_repo_preserves_origin_and_adds_jeryu_remote() {
        let repo = tempdir().expect("temp repo");
        git(repo.path(), &["init"]);
        git(
            repo.path(),
            &[
                "remote",
                "add",
                "origin",
                "git@example.invalid:team/demo.git",
            ],
        );

        configure_remote(
            repo.path(),
            "jeryu",
            "ssh://git@127.0.0.1:2224/team/demo.git",
            false,
        )
        .unwrap();

        let origin = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["remote", "get-url", "origin"])
            .output()
            .unwrap();
        let jeryu = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["remote", "get-url", "jeryu"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&origin.stdout).trim(),
            "git@example.invalid:team/demo.git"
        );
        assert_eq!(
            String::from_utf8_lossy(&jeryu.stdout).trim(),
            "ssh://git@127.0.0.1:2224/team/demo.git"
        );
    }

    #[test]
    fn new_repo_uses_origin_as_local_jeryu_remote() {
        let repo = tempdir().expect("temp repo");
        git(repo.path(), &["init"]);

        configure_remote(
            repo.path(),
            "origin",
            "ssh://git@127.0.0.1:2224/team/demo.git",
            true,
        )
        .unwrap();

        let origin = std::process::Command::new("git")
            .current_dir(repo.path())
            .args(["remote", "get-url", "origin"])
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&origin.stdout).trim(),
            "ssh://git@127.0.0.1:2224/team/demo.git"
        );
    }

    #[test]
    fn jeryu_toml_rendering_is_deterministic_and_secret_free() {
        let repo = tempdir().expect("temp repo");
        write_jeryu_configs(
            repo.path(),
            JeryuConfigSpec {
                mode: RepoMode::Observed,
                hooks: HookMode::Advisory,
                namespace: "team",
                name: "demo",
                branch: "main",
                protect_main: true,
                main_relay: true,
                offline_release_remote: Some("https://github.com/neverhuman/warp"),
            },
        )
        .unwrap();
        let first = fs::read_to_string(repo.path().join(".jeryu/policy.toml")).unwrap();
        write_jeryu_configs(
            repo.path(),
            JeryuConfigSpec {
                mode: RepoMode::Observed,
                hooks: HookMode::Advisory,
                namespace: "team",
                name: "demo",
                branch: "main",
                protect_main: true,
                main_relay: true,
                offline_release_remote: Some("https://github.com/neverhuman/warp"),
            },
        )
        .unwrap();
        let second = fs::read_to_string(repo.path().join(".jeryu/policy.toml")).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("[main_relay]"));
        assert!(first.contains("actor = \"jeryu\""));
        assert!(first.contains("[offline_release_mirror]"));
        let combined = ["repo.toml", "policy.toml", "backup.toml", "ci.toml"]
            .iter()
            .map(|name| fs::read_to_string(repo.path().join(".jeryu").join(name)).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!combined.to_ascii_lowercase().contains("token"));
        assert!(!combined.to_ascii_lowercase().contains("password"));
        assert!(!combined.to_ascii_lowercase().contains("identityfile"));
    }
}
