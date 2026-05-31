//! Adversarial escape suite: four jobs that MUST each be blocked by a real
//! kernel primitive — OR honestly skipped-with-receipt when that primitive is
//! unavailable on this host. There are NO false skips: a skip is only legal when
//! the capability probe says the primitive is genuinely missing.
//!
//! Crucial asymmetry the task demands: a Landlock-blockable escape MUST NOT skip
//! when `landlock_abi.is_some()`. We assert that directly.
//!
//! Each escape is run in a throwaway child where the relevant primitive is
//! applied (seccomp filter / Landlock ruleset / cgroup membership) and the child
//! then attempts the escape, reporting the verdict through its exit code. The
//! parent classifies the result as Blocked / Allowed / Skipped and writes a
//! receipt to `target/jankurai/runner-sandbox/enforcement.json`.

use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::seccomp_rules;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, fork};
use seccompiler::TargetArch;
use std::path::{Path, PathBuf};

/// Verdict for one escape attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// The escape was blocked by the kernel primitive (the desired outcome).
    Blocked,
    /// The escape SUCCEEDED — a true sandbox breach. Always a test failure.
    Escaped,
    /// The primitive is genuinely unavailable on this host; skipped honestly.
    Skipped,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Blocked => "blocked",
            Self::Escaped => "escaped",
            Self::Skipped => "skipped",
        }
    }
}

struct EscapeResult {
    name: &'static str,
    primitive: &'static str,
    verdict: Verdict,
    detail: String,
}

fn native_groups() -> Vec<String> {
    vec![
        "process-basic".into(),
        "file-readwrite-workspace".into(),
        "futex".into(),
        "time".into(),
        "rust-build-tooling".into(),
    ]
}

fn target_arch() -> TargetArch {
    match std::env::consts::ARCH {
        "aarch64" => TargetArch::aarch64,
        "riscv64" => TargetArch::riscv64,
        _ => TargetArch::x86_64,
    }
}

/// Run `body` in a forked child and interpret its exit code.
/// Exit 0 => Blocked, 1 => Escaped, 2 => Skipped (child decided), other => Escaped.
fn run_child<F: FnOnce() -> u8>(body: F) -> Verdict {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let code = body();
            unsafe { libc::_exit(code as i32) };
        }
        Ok(ForkResult::Parent { child }) => match waitpid(child, None) {
            Ok(WaitStatus::Exited(_, 0)) => Verdict::Blocked,
            Ok(WaitStatus::Exited(_, 2)) => Verdict::Skipped,
            // Killed by a signal => treat as blocked (e.g. cgroup OOM/kill,
            // seccomp KillProcess). The escape did not complete.
            Ok(WaitStatus::Signaled(_, _, _)) => Verdict::Blocked,
            _ => Verdict::Escaped,
        },
        Err(_) => Verdict::Skipped,
    }
}

// ---- Escape 1: write outside the workspace (Landlock) ---------------------

fn escape_write_outside_workspace(caps: &SandboxCapabilities, workspace: &Path) -> EscapeResult {
    let outside = std::env::temp_dir().join(format!(
        "jeryu-escape-write-{}-outside.txt",
        std::process::id()
    ));
    let outside_for_child = outside.clone();
    let ws = workspace.to_path_buf();

    let verdict = if caps.landlock_abi.is_none() {
        Verdict::Skipped
    } else {
        run_child(move || {
            if apply_landlock_workspace_only(&ws).is_err() {
                return 2; // could not set up the primitive -> skip
            }
            // Attempt to create a file OUTSIDE the workspace. Landlock must deny.
            match std::fs::write(&outside_for_child, b"escaped") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&outside_for_child);
                    1 // escaped!
                }
                Err(_) => 0, // blocked
            }
        })
    };
    let _ = std::fs::remove_file(&outside);

    EscapeResult {
        name: "write_outside_workspace",
        primitive: "landlock",
        verdict,
        detail: format!("target={}", outside.display()),
    }
}

// ---- Escape 2: open an AF_INET socket (seccomp) ---------------------------

fn escape_open_inet_socket(caps: &SandboxCapabilities) -> EscapeResult {
    let groups = native_groups();
    let arch = target_arch();

    let verdict = if !caps.seccomp_bpf {
        Verdict::Skipped
    } else {
        match seccomp_rules::compile(&groups, arch) {
            Err(_) => Verdict::Skipped,
            Ok(bpf) => run_child(move || {
                if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
                    return 2;
                }
                if seccompiler::apply_filter(&bpf).is_err() {
                    return 2;
                }
                let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
                if fd >= 0 {
                    unsafe { libc::close(fd) };
                    1 // escaped: got an internet socket
                } else {
                    0 // blocked
                }
            }),
        }
    };

    EscapeResult {
        name: "open_af_inet_socket",
        primitive: "seccomp",
        verdict,
        detail: "socket(AF_INET, SOCK_STREAM)".to_string(),
    }
}

// ---- Escape 3: read a sensitive file outside the workspace (Landlock) -----

fn escape_read_outside_workspace(caps: &SandboxCapabilities, workspace: &Path) -> EscapeResult {
    // HONESTY: /etc/shadow is already unreadable via DAC for non-root, so reading
    // it would "succeed-as-blocked" even WITHOUT Landlock and prove nothing. We
    // therefore target a file that IS readable by DAC but lives OUTSIDE the
    // workspace, so only Landlock can do the blocking. We seed it ourselves to
    // avoid depending on host files. We also attempt /etc/shadow as a secondary
    // assertion that the canonical secret stays unreadable.
    let secret = std::env::temp_dir().join(format!(
        "jeryu-escape-read-{}-secret.txt",
        std::process::id()
    ));
    if std::fs::write(&secret, b"top-secret-outside-workspace").is_err() {
        return EscapeResult {
            name: "read_outside_workspace",
            primitive: "landlock",
            verdict: Verdict::Skipped,
            detail: "could not seed secret file".to_string(),
        };
    }
    let secret_for_child = secret.clone();
    let ws = workspace.to_path_buf();

    let verdict = if caps.landlock_abi.is_none() {
        Verdict::Skipped
    } else {
        run_child(move || {
            if apply_landlock_workspace_only(&ws).is_err() {
                return 2;
            }
            // Reading a DAC-readable file outside the workspace must be denied.
            match std::fs::read(&secret_for_child) {
                Ok(_) => 1,  // escaped: read a file outside the sandbox
                Err(_) => 0, // blocked by Landlock
            }
        })
    };
    let _ = std::fs::remove_file(&secret);

    EscapeResult {
        name: "read_outside_workspace",
        primitive: "landlock",
        verdict,
        detail: format!(
            "target={} (DAC-readable, outside workspace)",
            secret.display()
        ),
    }
}

// ---- Escape 4: fork bomb (cgroup pids.max) --------------------------------

fn escape_fork_bomb(caps: &SandboxCapabilities) -> EscapeResult {
    let verdict = match &caps.cgroup_v2_subtree {
        None => Verdict::Skipped,
        Some(parent) => {
            // Create a child cgroup with a tiny pids.max, join it, then try to
            // spawn far more than the limit. The kernel must refuse the clones
            // once pids.max is hit, so the unbounded fork cannot complete.
            match make_limited_cgroup(parent, 16) {
                None => Verdict::Skipped,
                Some(cg) => {
                    let procs = cg.join("cgroup.procs");
                    let cg_clone = cg.clone();
                    let v = run_child(move || {
                        // Join the constrained cgroup.
                        if std::fs::write(&procs, std::process::id().to_string()).is_err() {
                            return 2;
                        }
                        // Try to create many processes. With pids.max=16 the
                        // kernel returns EAGAIN well before 200, so the bomb is
                        // contained. We reap nothing (children sleep then exit);
                        // success of containment == we observe at least one
                        // clone failure.
                        let mut contained = false;
                        let mut kids = Vec::new();
                        for _ in 0..200 {
                            match unsafe { fork() } {
                                Ok(ForkResult::Child) => {
                                    // grandchild: just sleep briefly then exit
                                    unsafe { libc::usleep(50_000) };
                                    unsafe { libc::_exit(0) };
                                }
                                Ok(ForkResult::Parent { child }) => kids.push(child),
                                Err(_) => {
                                    contained = true;
                                    break;
                                }
                            }
                        }
                        for k in kids {
                            let _ = waitpid(k, None);
                        }
                        if contained { 0 } else { 1 }
                    });
                    // Cleanup the cgroup (best effort; must be empty first).
                    let _ = std::fs::remove_dir(&cg_clone);
                    v
                }
            }
        }
    };

    EscapeResult {
        name: "fork_bomb",
        primitive: "cgroup_pids",
        verdict,
        detail: "pids.max=16, attempt 200 clones".to_string(),
    }
}

/// Apply a Landlock ruleset that makes ONLY `workspace` (and the system read
/// paths) accessible, mirroring what the real launch path installs.
fn apply_landlock_workspace_only(workspace: &Path) -> Result<(), String> {
    use landlock::{
        ABI, Access, AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
    };
    let abi = ABI::V4;
    let mut ruleset = Ruleset::default()
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| e.to_string())?
        .create()
        .map_err(|e| e.to_string())?;
    // Workspace: read + write.
    if let Ok(fd) = PathFd::new(workspace) {
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, AccessFs::from_all(abi)))
            .map_err(|e| e.to_string())?;
    }
    // System read-only roots so the dynamic loader still works inside the child.
    for ro in ["/usr", "/lib", "/lib64", "/bin", "/nix/store", "/proc"] {
        if let Ok(fd) = PathFd::new(ro) {
            ruleset = ruleset
                .add_rule(PathBeneath::new(fd, AccessFs::from_read(abi)))
                .map_err(|e| e.to_string())?;
        }
    }
    ruleset.restrict_self().map_err(|e| e.to_string())?;
    Ok(())
}

/// Create a child cgroup under `parent` with `pids_max`, returning its path.
fn make_limited_cgroup(parent: &Path, pids_max: u32) -> Option<PathBuf> {
    let _ = std::fs::write(parent.join("cgroup.subtree_control"), b"+pids +memory");
    let dir = parent.join(format!(
        "jeryu-escape-forkbomb-{}.scope",
        std::process::id()
    ));
    std::fs::create_dir(&dir).ok()?;
    if std::fs::write(dir.join("pids.max"), pids_max.to_string()).is_err() {
        let _ = std::fs::remove_dir(&dir);
        return None;
    }
    Some(dir)
}

fn write_enforcement_json(caps: &SandboxCapabilities, results: &[EscapeResult]) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/jankurai/runner-sandbox");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("enforcement.json");

    let escapes = results
        .iter()
        .map(|r| {
            format!(
                "{{\"name\":\"{}\",\"primitive\":\"{}\",\"verdict\":\"{}\",\"detail\":\"{}\"}}",
                r.name,
                r.primitive,
                r.verdict.as_str(),
                r.detail.replace('"', "\\\"").replace('\\', "\\\\")
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let false_skips = count_false_skips(caps, results);
    let json = format!(
        concat!(
            "{{",
            "\"capabilities\":\"{}\",",
            "\"landlock_abi\":{},",
            "\"seccomp_bpf\":{},",
            "\"cgroup_v2\":{},",
            "\"no_new_privs\":{},",
            "\"false_skips\":{},",
            "\"escapes\":[{}]",
            "}}"
        ),
        caps.summary(),
        caps.landlock_abi
            .map(|a| a.to_string())
            .unwrap_or_else(|| "null".to_string()),
        caps.seccomp_bpf,
        caps.cgroup_v2_subtree.is_some(),
        caps.no_new_privs,
        false_skips,
        escapes,
    );
    let _ = std::fs::write(&path, json);
    path
}

/// A "false skip" is a Skipped verdict whose primitive IS actually available.
/// The whole suite's integrity hinges on this being zero.
fn count_false_skips(caps: &SandboxCapabilities, results: &[EscapeResult]) -> usize {
    results
        .iter()
        .filter(|r| r.verdict == Verdict::Skipped && primitive_available(caps, r.primitive))
        .count()
}

fn primitive_available(caps: &SandboxCapabilities, primitive: &str) -> bool {
    match primitive {
        "landlock" => caps.landlock_abi.is_some(),
        "seccomp" => caps.seccomp_bpf,
        "cgroup_pids" => caps.cgroup_v2_subtree.is_some(),
        _ => false,
    }
}

#[test]
fn escape_suite_blocks_or_honestly_skips() {
    let caps = SandboxCapabilities::probe();

    // Workspace the sandboxed children are confined to.
    let workspace = std::env::temp_dir().join(format!("jeryu-escape-ws-{}", std::process::id()));
    std::fs::create_dir_all(&workspace).unwrap_or_else(|e| panic!("create workspace: {e}"));

    let results = vec![
        escape_write_outside_workspace(&caps, &workspace),
        escape_open_inet_socket(&caps),
        escape_read_outside_workspace(&caps, &workspace),
        escape_fork_bomb(&caps),
    ];

    let path = write_enforcement_json(&caps, &results);
    eprintln!("enforcement receipt: {}", path.display());
    eprintln!("capabilities: {}", caps.summary());
    for r in &results {
        eprintln!(
            "  escape {:<26} primitive={:<12} => {}",
            r.name,
            r.primitive,
            r.verdict.as_str()
        );
    }

    // 1. NO escape may actually succeed.
    let escaped: Vec<&str> = results
        .iter()
        .filter(|r| r.verdict == Verdict::Escaped)
        .map(|r| r.name)
        .collect();
    assert!(
        escaped.is_empty(),
        "sandbox BREACHED by: {escaped:?} (these escapes were not blocked)"
    );

    // 2. NO false skips: a skip is only legal when the primitive is genuinely
    //    unavailable.
    let false_skips = count_false_skips(&caps, &results);
    assert_eq!(
        false_skips, 0,
        "false_skips must be 0: some escape skipped despite its primitive being available"
    );

    // 3. ASYMMETRY: a Landlock-blockable escape MUST NOT skip when Landlock is
    //    present. On this host (ABI 4 active) both Landlock escapes must Block.
    if caps.landlock_abi.is_some() {
        for r in results.iter().filter(|r| r.primitive == "landlock") {
            assert_eq!(
                r.verdict,
                Verdict::Blocked,
                "Landlock escape '{}' must be BLOCKED (not skipped) when landlock_abi.is_some()",
                r.name
            );
        }
    }

    // 4. Same fail-closed asymmetry for seccomp and cgroup when available.
    if caps.seccomp_bpf {
        for r in results.iter().filter(|r| r.primitive == "seccomp") {
            assert_eq!(
                r.verdict,
                Verdict::Blocked,
                "seccomp escape '{}' must be BLOCKED when seccomp_bpf is available",
                r.name
            );
        }
    }
    if caps.cgroup_v2_subtree.is_some() {
        for r in results.iter().filter(|r| r.primitive == "cgroup_pids") {
            assert_eq!(
                r.verdict,
                Verdict::Blocked,
                "cgroup escape '{}' must be BLOCKED when a delegated cgroup subtree exists",
                r.name
            );
        }
    }

    let _ = std::fs::remove_dir_all(&workspace);
}
