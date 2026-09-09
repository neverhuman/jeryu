//! Session workspace materialization, agent credential seeding, and runtime selection.

use super::*;

/// Materialize the session workspace into a real working tree on the unique branch
/// at `base_oid`. Clones the local bare repo (`--no-local`-style file clone, no
/// network) into `workspace`, then forces the session branch to the registered base
/// oid. Returns `Err(reason)` if any host git step fails so the caller can degrade
/// gracefully (record the run failed with a TTY line) instead of returning a 500.
pub(super) fn materialize_workspace(
    git_bin: &str,
    bare: &std::path::Path,
    workspace: &std::path::Path,
    branch: &str,
    base_oid: &str,
) -> Result<(), String> {
    // A pre-existing empty dir (the temp path was reserved up front) is fine, but a
    // populated one is not — `git clone` requires an empty or absent target.
    let _ = std::fs::remove_dir_all(workspace);
    let bare = bare.to_string_lossy().to_string();
    let workspace_arg = workspace.to_string_lossy().to_string();
    // Local file clone of the forge's bare repo — robust regardless of which branch
    // the bare repo currently points HEAD at, and it never touches the network.
    run_git(
        git_bin,
        &["clone", "--no-local", &bare, &workspace_arg],
        None,
    )?;
    // Force the session branch to the exact base oid and check it out, so the agent
    // starts on its own namespaced branch at the default-branch tip.
    run_git(
        git_bin,
        &["-C", &workspace_arg, "checkout", "-B", branch, base_oid],
        None,
    )?;
    Ok(())
}

/// Run one host git step, mapping a spawn failure or non-zero exit to a short
/// reason string for the graceful checkout-failure path.
fn run_git(git_bin: &str, args: &[&str], cwd: Option<&std::path::Path>) -> Result<(), String> {
    let mut cmd = std::process::Command::new(git_bin);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd
        .output()
        .map_err(|err| format!("git {} failed to spawn: {err}", args.join(" ")))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Resolve which PTY backend a session agent runs under, the launch command for it,
/// and (for `auto`) a docker fallback. `auto` (default) runs the native kernel
/// sandbox and, only if native returns `sandbox_unavailable` at spawn time, retries
/// the same run on docker; `docker`/`native` force one path.
///
/// Returns `(backend, primary_spec, docker_fallback)`:
/// - `docker`: backend `DockerHost`, primary = the host `docker run ... <image>
///   <in-image agent argv>` (hardened flags from the planned [`OciSpec`]); `None`
///   when `docker` is absent (drives the graceful not-available line).
/// - `native`: backend `Native`, primary = the resolved host agent binary; `None`
///   when that binary is absent.
/// - `auto`: backend `Native` with the host agent binary as primary AND a docker
///   command as the fallback (when docker is on PATH). A missing native binary
///   still degrades to the graceful not-available line — `auto` only falls back on
///   an actual kernel-sandbox failure, never on a missing agent CLI.
pub(super) fn resolve_session_backend(
    config: &SessionRuntimeConfig,
    agent_id: &str,
    agent_program: &std::path::Path,
    workspace: &std::path::Path,
    env: BTreeMap<String, String>,
    container: &OciSpec,
    run_id: &str,
) -> (PtyBackend, Option<CommandSpec>, Option<CommandSpec>) {
    let native_ok = !agent_program.as_os_str().is_empty() && agent_program.is_file();
    let native_spec = native_ok.then(|| {
        // The native binary is launched directly (no in-image entrypoint), so the
        // default launch flags the docker path bakes into `in_image_agent_command`
        // are merged in here too — appended only when the caller did not already
        // pass them, so the two backends stay flag-for-flag identical.
        let mut args: Vec<String> = container.command.iter().skip(1).cloned().collect();
        append_missing_flags(&mut args, agent_default_flags(agent_id));
        CommandSpec {
            program: agent_program.to_string_lossy().to_string(),
            args,
            env: env.clone(),
        }
    });
    let docker_spec = config
        .docker_bin
        .as_deref()
        .map(|docker| docker_command(docker, container, workspace, agent_id, env, run_id));

    match config.runtime {
        SessionRuntime::Docker => (PtyBackend::DockerHost, docker_spec, None),
        SessionRuntime::Native => (PtyBackend::Native, native_spec, None),
        SessionRuntime::Auto => (PtyBackend::Native, native_spec, docker_spec),
    }
}

/// Seed the host operator's agent CLI auth into the session workspace so the
/// container (or native) agent starts pre-authenticated. Login once on the
/// host, every New Session inherits.
///
/// The container `$HOME` is `/workspace/.agent-home` (set in the Dockerfile),
/// so we copy the auth files into `{workspace}/.agent-home/{config_dir}/{file}`.
///
/// This is best-effort: a missing host file is silently skipped, a copy failure
/// is logged but never blocks the session. Files are copied fresh (not
/// bind-mounted) on every session start so credentials are always up-to-date
/// but the container cannot modify the host's tokens.
pub(super) fn seed_agent_auth(
    workspace: &std::path::Path,
    agent_id: &str,
    auth_home: Option<&std::path::Path>,
) {
    let host_home = auth_home
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| {
            std::env::var("JERYU_AUTH_HOME")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/root".to_string()))
                })
        });
    seed_agent_auth_from_home(workspace, agent_id, &host_home);
}

pub(super) fn seed_agent_auth_from_home(
    workspace: &std::path::Path,
    agent_id: &str,
    host_home: &std::path::Path,
) {
    /// One auth file mapping: host relative path (under `$HOME`) → container
    /// relative path (under `{workspace}/.agent-home`).
    struct AuthFile {
        host_rel: &'static str,
        container_rel: &'static str,
    }

    let codex_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".codex/auth.json",
            container_rel: ".codex/auth.json",
        },
        AuthFile {
            host_rel: ".codex/config.toml",
            container_rel: ".codex/config.toml",
        },
        AuthFile {
            host_rel: ".codex/config.yaml",
            container_rel: ".codex/config.yaml",
        },
    ];

    let claude_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".claude.json",
            container_rel: ".claude.json",
        },
        AuthFile {
            host_rel: ".claude/.credentials.json",
            container_rel: ".claude/.credentials.json",
        },
        AuthFile {
            host_rel: ".claude/settings.json",
            container_rel: ".claude/settings.json",
        },
    ];

    let agy_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".gemini/antigravity-cli/installation_id",
            container_rel: ".gemini/antigravity-cli/installation_id",
        },
        AuthFile {
            host_rel: ".gemini/antigravity-cli/settings.json",
            container_rel: ".gemini/antigravity-cli/settings.json",
        },
    ];

    let all_files: &[AuthFile] = &[
        AuthFile {
            host_rel: ".codex/auth.json",
            container_rel: ".codex/auth.json",
        },
        AuthFile {
            host_rel: ".codex/config.toml",
            container_rel: ".codex/config.toml",
        },
        AuthFile {
            host_rel: ".codex/config.yaml",
            container_rel: ".codex/config.yaml",
        },
        AuthFile {
            host_rel: ".claude.json",
            container_rel: ".claude.json",
        },
        AuthFile {
            host_rel: ".claude/.credentials.json",
            container_rel: ".claude/.credentials.json",
        },
        AuthFile {
            host_rel: ".claude/settings.json",
            container_rel: ".claude/settings.json",
        },
        AuthFile {
            host_rel: ".gemini/antigravity-cli/installation_id",
            container_rel: ".gemini/antigravity-cli/installation_id",
        },
        AuthFile {
            host_rel: ".gemini/antigravity-cli/settings.json",
            container_rel: ".gemini/antigravity-cli/settings.json",
        },
    ];

    let files: &[AuthFile] = match agent_id {
        "codex" => codex_files,
        "claude" => claude_files,
        "agy" => agy_files,
        _ => all_files,
    };

    let agent_home = workspace.join(".agent-home");

    for file in files {
        let src = host_home.join(file.host_rel);
        if !src.is_file() {
            continue;
        }
        let dst = agent_home.join(file.container_rel);
        if let Some(parent) = dst.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            eprintln!(
                "seed_agent_auth: failed to create dir {} -> {}: {}",
                src.display(),
                dst.display(),
                err
            );
            continue;
        }
        match std::fs::copy(&src, &dst) {
            Ok(bytes) => {
                let _ = std::fs::set_permissions(
                    &dst,
                    std::fs::Permissions::from_mode(seeded_auth_file_mode(file.container_rel)),
                );
                eprintln!(
                    "seed_agent_auth[{}]: seeded {} -> {} ({} bytes)",
                    agent_id,
                    src.display(),
                    dst.display(),
                    bytes
                );
            }
            Err(err) => {
                eprintln!(
                    "seed_agent_auth[{}]: failed to copy {} -> {}: {}",
                    agent_id,
                    src.display(),
                    dst.display(),
                    err
                );
            }
        }
    }

    // ── Seed agy auth: copy entire ~/.gemini tree (config + CLI state) ────
    if agent_id == "agy" || !matches!(agent_id, "codex" | "claude") {
        // Recursively copy relevant ~/.gemini subtrees for agy auth.
        fn copy_dir_recursive(
            src: &std::path::Path,
            dst: &std::path::Path,
            agent_id: &str,
            label: &str,
        ) {
            let _ = std::fs::create_dir_all(dst);
            let entries = match std::fs::read_dir(src) {
                Ok(e) => e,
                Err(_) => return,
            };
            for entry in entries.flatten() {
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                if src_path.is_dir() {
                    copy_dir_recursive(&src_path, &dst_path, agent_id, label);
                } else if src_path.is_file() {
                    match std::fs::copy(&src_path, &dst_path) {
                        Ok(bytes) => {
                            eprintln!(
                                "seed_agent_auth[{}]: seeded {} {} ({} bytes)",
                                agent_id,
                                label,
                                entry.file_name().to_string_lossy(),
                                bytes
                            );
                        }
                        Err(err) => {
                            eprintln!(
                                "seed_agent_auth[{}]: failed {} {}: {}",
                                agent_id,
                                label,
                                entry.file_name().to_string_lossy(),
                                err
                            );
                        }
                    }
                }
            }
        }

        // ~/.gemini/antigravity-cli/ (installation_id, implicit tokens, settings)
        let cli_src = host_home.join(".gemini/antigravity-cli");
        let cli_dst = agent_home.join(".gemini/antigravity-cli");
        copy_dir_recursive(&cli_src, &cli_dst, agent_id, "cli");

        // ~/.gemini/config/ (projects, mcp_config, .migrated marker)
        let cfg_src = host_home.join(".gemini/config");
        let cfg_dst = agent_home.join(".gemini/config");
        copy_dir_recursive(&cfg_src, &cfg_dst, agent_id, "config");
    }

    // ── Seed a custom resolv.conf with public DNS for sandboxed agents ──
    // Some agent CLIs (agy) use [::1]:53 as DNS fallback instead of reading
    // /etc/resolv.conf. Write a resolv.conf with Google DNS into the workspace
    // at a well-known path; the sandbox mounts it over /etc/resolv.conf.
    {
        let resolv_path = workspace.join(".resolv.conf");
        let _ = std::fs::write(
            &resolv_path,
            "nameserver 8.8.8.8\nnameserver 8.8.4.4\noptions ndots:0\n",
        );
        eprintln!(
            "seed_agent_auth[{}]: wrote custom resolv.conf at {}",
            agent_id,
            resolv_path.display()
        );
    }

    // ── Create writable dirs that agent CLIs expect under $HOME ──────────
    // agy needs write access to log/, cache/, conversations/, knowledge/ etc.
    if agent_id == "agy" {
        for subdir in &[
            ".gemini/antigravity-cli/log",
            ".gemini/antigravity-cli/cache",
            ".gemini/antigravity-cli/conversations",
            ".gemini/antigravity-cli/knowledge",
            ".gemini/antigravity-cli/builtin",
        ] {
            let _ = std::fs::create_dir_all(agent_home.join(subdir));
        }
    }

    // ── Strip host-only MCP servers from seeded Codex config ───────────
    // MCP servers like jnoccio-router bind to the host's loopback and are
    // unreachable from inside the sandbox. Leaving them causes a noisy
    // "MCP startup incomplete" warning on every session start.
    if agent_id == "codex" || !matches!(agent_id, "claude" | "agy") {
        let codex_cfg = agent_home.join(".codex/config.toml");
        if codex_cfg.is_file() {
            let _ = std::fs::set_permissions(&codex_cfg, std::fs::Permissions::from_mode(0o600));
            if let Ok(raw) = std::fs::read_to_string(&codex_cfg) {
                // Drop all [mcp_servers.*] sections (and their sub-tables).
                let mut cleaned = String::new();
                let mut skip = false;
                for line in raw.lines() {
                    if line.starts_with("[mcp_servers") {
                        skip = true;
                        continue;
                    }
                    // A new top-level section ends the skip.
                    if skip && line.starts_with('[') && !line.starts_with("[mcp_servers") {
                        skip = false;
                    }
                    if !skip {
                        cleaned.push_str(line);
                        cleaned.push('\n');
                    }
                }
                let _ = std::fs::write(&codex_cfg, &cleaned);
                let _ =
                    std::fs::set_permissions(&codex_cfg, std::fs::Permissions::from_mode(0o400));
                eprintln!(
                    "seed_agent_auth[{}]: stripped [mcp_servers.*] from config.toml",
                    agent_id
                );
            }
        }
    }

    // ── Auto-trust the workspace so codex/claude never prompts ──────────
    // Codex sees `/workspace` in docker and the real checkout path in native.
    if agent_id == "codex" || agent_id == "claude" || agent_id == "agy" {
        let codex_cfg = agent_home.join(".codex/config.toml");
        let trust_paths = codex_trust_paths(workspace);
        if codex_cfg.is_file() {
            // Temporarily make writable (it was locked to 0o400 by the copy loop).
            let _ = std::fs::set_permissions(&codex_cfg, std::fs::Permissions::from_mode(0o600));
            if append_codex_trust_entries(&codex_cfg, &trust_paths).is_ok() {
                let _ = std::fs::set_permissions(
                    &codex_cfg,
                    std::fs::Permissions::from_mode(seeded_auth_file_mode(".codex/config.toml")),
                );
                eprintln!(
                    "seed_agent_auth[{}]: ensured workspace trust in config.toml",
                    agent_id
                );
            }
        } else {
            // No host config — create a minimal one with just workspace trust.
            let _ = std::fs::create_dir_all(agent_home.join(".codex"));
            let _ = write_codex_trust_config(&codex_cfg, &trust_paths);
            eprintln!(
                "seed_agent_auth[{}]: created minimal config.toml with workspace trust",
                agent_id
            );
        }
    }

    // ── Claude: skip first-run onboarding (theme picker) ────────────────
    // Claude Code checks `hasCompletedOnboarding` in top-level `~/.claude.json`.
    // Keep the nested path too for older builds, but the top-level copy is the
    // important one for auth/session state.
    if agent_id == "claude" || !matches!(agent_id, "codex" | "agy") {
        for relative in [".claude.json", ".claude/.claude.json"] {
            if let Err(error) = ensure_claude_onboarding_state(&agent_home.join(relative), agent_id)
            {
                eprintln!("{error}");
            }
        }
    }
}

fn seeded_auth_file_mode(container_rel: &str) -> u32 {
    match container_rel {
        ".claude.json" | ".claude/settings.json" => 0o600,
        _ => 0o400,
    }
}

fn codex_trust_paths(workspace: &std::path::Path) -> Vec<String> {
    let mut paths = vec!["/workspace".to_string()];
    let native = workspace.to_string_lossy().to_string();
    if native != "/workspace" {
        paths.push(native);
    }
    paths
}

fn write_codex_trust_config(path: &std::path::Path, trust_paths: &[String]) -> std::io::Result<()> {
    let mut text = String::new();
    for trust_path in trust_paths {
        text.push_str(&codex_trust_entry(trust_path));
    }
    std::fs::write(path, text)?;
    std::fs::set_permissions(
        path,
        std::fs::Permissions::from_mode(seeded_auth_file_mode(".codex/config.toml")),
    )
}

fn append_codex_trust_entries(
    path: &std::path::Path,
    trust_paths: &[String],
) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut additions = String::new();
    for trust_path in trust_paths {
        let header = codex_trust_header(trust_path);
        if !existing.contains(&header) {
            additions.push_str(&codex_trust_entry(trust_path));
        }
    }
    if additions.is_empty() {
        return Ok(());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
    file.write_all(additions.as_bytes())
}

fn codex_trust_header(path: &str) -> String {
    format!("[projects.\"{}\"]", toml_basic_string_fragment(path))
}

fn codex_trust_entry(path: &str) -> String {
    format!(
        "\n{}\ntrust_level = \"trusted\"\n",
        codex_trust_header(path)
    )
}

fn toml_basic_string_fragment(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn ensure_claude_onboarding_state(path: &std::path::Path, agent_id: &str) -> std::io::Result<()> {
    let context = |operation: &str, error: std::io::Error| {
        std::io::Error::new(
            error.kind(),
            format!(
                "seed_agent_auth[{agent_id}]: failed to {operation} Claude onboarding state {}: {error}",
                path.display()
            ),
        )
    };
    let mut object = match std::fs::read_to_string(path) {
        Ok(raw) => {
            let value: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
                context(
                    "parse",
                    std::io::Error::new(std::io::ErrorKind::InvalidData, error),
                )
            })?;
            match value {
                serde_json::Value::Object(object) => object,
                _ => {
                    return Err(context(
                        "validate",
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "expected a JSON object",
                        ),
                    ));
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::Map::new(),
        Err(error) => return Err(context("read", error)),
    };
    // Validate existing state before creating directories or mutating the file.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| context("create parent for", error))?;
    }

    object.insert(
        "hasCompletedOnboarding".to_string(),
        serde_json::json!(true),
    );
    object
        .entry("numStartups".to_string())
        .or_insert_with(|| serde_json::json!(1));
    object
        .entry("autoUpdates".to_string())
        .or_insert_with(|| serde_json::json!(false));
    object
        .entry("theme".to_string())
        .or_insert_with(|| serde_json::json!("dark"));
    object
        .entry("lastOnboardingVersion".to_string())
        .or_insert_with(|| serde_json::json!("2.1.170"));
    object
        .entry("hasSeenAutoDefaultNudge".to_string())
        .or_insert_with(|| serde_json::json!(true));
    object
        .entry("hasSeenAutoDefaultNotice".to_string())
        .or_insert_with(|| serde_json::json!(true));

    let bytes = serde_json::to_vec_pretty(&object)
        .map_err(std::io::Error::other)
        .map_err(|error| context("serialize", error))?;
    std::fs::write(path, bytes).map_err(|error| context("write", error))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| context("set permissions for", error))?;
    eprintln!(
        "seed_agent_auth[{}]: ensured Claude onboarding state at {}",
        agent_id,
        path.display()
    );
    Ok(())
}

#[cfg(test)]
#[path = "onboarding_tests.rs"]
mod onboarding_tests;

/// Build the host `docker run ...` launch command for a session agent. The flags
/// come straight from the planned, hardened [`OciSpec`] (read-only root, all caps
/// dropped, `--network none`, the workspace bind-mounted at `/workspace`), with the
/// in-image agent CLI as the container's argv and `-i` + a stable `--name` injected
/// for the live PTY. The workspace mount is rewritten to the materialized session
/// checkout so the agent sees real code at `/workspace`.
fn docker_command(
    docker: &str,
    container: &OciSpec,
    workspace: &std::path::Path,
    agent_id: &str,
    env: BTreeMap<String, String>,
    run_id: &str,
) -> CommandSpec {
    let mut spec = container.clone();
    spec.workspace = workspace.to_string_lossy().to_string();
    spec.command = in_image_agent_command(agent_id);
    // The forge env (JERYU_BRANCH etc.) is already carried as `-e` flags by the
    // planned container; the host docker process itself needs no extra env.
    let args = spec.live_pty_args(run_id);
    CommandSpec {
        program: docker.to_string(),
        args,
        env,
    }
}

/// The default launch flags a session agent always runs with when started by the
/// web tool. Interactive sessions run inside the hardened, network-deny sandbox, so
/// the agents are launched in their non-interactive "trust the sandbox" modes:
/// `agy`/`claude` skip the per-action permission prompt and `codex` runs in
/// full-auto (`--yolo`). An id with no entry runs bare.
fn agent_default_flags(agent_id: &str) -> &'static [&'static str] {
    match agent_id {
        "agy" | "claude" => &["--dangerously-skip-permissions"],
        "codex" => &["--yolo"],
        _ => &[],
    }
}

/// Append each of `flags` to `args` only when it is not already present, so the
/// merge is idempotent against a caller who already passed the flag.
fn append_missing_flags(args: &mut Vec<String>, flags: &[&str]) {
    for flag in flags {
        if !args.iter().any(|existing| existing == flag) {
            args.push((*flag).to_string());
        }
    }
}

/// Map an `agent_id` to the agent CLI on the image's PATH plus its default launch
/// flags. The hardened sandbox image bundles the coding-agent CLIs under stable
/// names; an unknown id falls back to the standard `agent` entrypoint (its absence
/// inside the image surfaces as the container exiting, which the live stream shows).
fn in_image_agent_command(agent_id: &str) -> Vec<String> {
    let binary = match agent_id {
        "codex" => "codex",
        "claude" => "claude",
        "jekko" => "jekko",
        "agy" => "agy",
        _ => "agent",
    };
    let mut command = vec![binary.to_string()];
    append_missing_flags(&mut command, agent_default_flags(agent_id));
    command
}

/// Which container/native runtime a launched session uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionRuntime {
    Auto,
    Docker,
    Native,
}

/// The resolved session-execution config: which PTY backend to prefer and which
/// docker binary the seam points at. Production resolves this from the
/// `JERYU_AGENT_RUNTIME` / `JERYU_DOCKER_BIN` env once at [`WebState`] construction;
/// a hermetic test injects it directly so it never mutates process-global env (the
/// crate forbids `unsafe`, so `std::env::set_var` is not available to tests).
#[derive(Debug, Clone)]
pub(crate) struct SessionRuntimeConfig {
    /// Preferred backend: `auto` (native then docker fallback), `docker`, `native`.
    pub(crate) runtime: SessionRuntime,
    /// The docker binary the seam invokes (`JERYU_DOCKER_BIN` or `docker` on PATH);
    /// `None` when no docker is resolvable, which drives the graceful path.
    pub(crate) docker_bin: Option<String>,
    /// Whether New Session starts the production two-pane companion shell.
    /// Production is always enabled; hermetic tests may disable the long-lived
    /// process and exercise the enabled path in one cleanup-aware proof.
    pub(crate) spawn_companion_shell: bool,
}

impl SessionRuntimeConfig {
    /// Resolve the session runtime config from the environment.
    ///
    /// `JERYU_AGENT_RUNTIME` selects the backend (`auto` default; `docker`/`native`
    /// force one; an unknown value is treated as `auto` so a typo never wedges New
    /// Session). `JERYU_DOCKER_BIN` overrides the docker binary the seam invokes;
    /// otherwise a `docker` on `PATH` is used when present.
    pub(crate) fn from_env() -> Self {
        let runtime = match std::env::var("JERYU_AGENT_RUNTIME")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("docker") => SessionRuntime::Docker,
            Some("native") => SessionRuntime::Native,
            _ => SessionRuntime::Auto,
        };
        let docker_bin = std::env::var("JERYU_DOCKER_BIN")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .filter(|bin| std::path::Path::new(bin).is_file())
            .or_else(docker_on_path);
        Self {
            runtime,
            docker_bin,
            spawn_companion_shell: true,
        }
    }
}

/// A `docker` on `PATH` as a launchable path, or `None` when absent.
///
/// Under test this always returns `None`: the deterministic session tests must
/// never reach the host's real docker through `from_env`, so a docker-backed test
/// injects [`SessionRuntimeConfig`] with an explicit fake `docker_bin` instead.
fn docker_on_path() -> Option<String> {
    #[cfg(test)]
    {
        None
    }
    #[cfg(not(test))]
    {
        let path = std::env::var("PATH").ok()?;
        std::env::split_paths(&path)
            .map(|dir| dir.join("docker"))
            .find(|candidate| candidate.is_file())
            .map(|candidate| candidate.to_string_lossy().to_string())
    }
}
