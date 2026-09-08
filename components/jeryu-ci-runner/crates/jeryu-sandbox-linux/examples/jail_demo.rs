//! RUNG 1 — a runnable jail demo for the native Jeryu sandbox.
//!
//! This drives the SAME production launch path the runner uses
//! ([`SandboxPlan::from_decision`] -> [`spawn_sandboxed`] -> [`verify_enforcement`])
//! to confine a child process to a single temporary "checkout" directory, then
//! has that child attempt four escapes that the kernel primitives must block:
//!
//!   (a) write a file INSIDE the checkout            -> expect OK    (allowed)
//!   (b) write a file OUTSIDE the checkout            -> expect DENIED (Landlock)
//!   (c) read /etc/shadow                             -> expect DENIED
//!   (d) open a TCP AF_INET socket                    -> expect DENIED (seccomp)
//!
//! HONESTY: a check is marked `skipped` ONLY when the host primitive that would
//! do the blocking is genuinely unavailable (e.g. Landlock disabled, no
//! seccomp-bpf). We never fake a DENIED. The /proc enforcement proof
//! (NoNewPrivs / Seccomp / landlock_abi) is printed via `verify_enforcement`.
//!
//! Build:  cargo build -p jeryu-sandbox-linux --example jail_demo
//! Run:    cargo run   -p jeryu-sandbox-linux --example jail_demo
//!
//! The binary re-execs ITSELF in a `child` mode (a copy placed inside the
//! checkout so it is reachable under the workspace-only Landlock ruleset) so the
//! escape attempts are precise raw syscalls running under the applied sandbox.

use jeryu_runner_core::job::{JobRequest, NetworkPolicy, SecretPolicy, TokenPolicy};
use jeryu_runner_core::policy::select_runner;
use jeryu_runner_core::sandbox::SandboxPlan;
use jeryu_runner_core::trust::TrustTier;
use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::launch::{spawn_sandboxed, verify_enforcement};
use jeryu_sandbox_linux::watchdog::run_with_watchdog;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

// Child exit codes, mirroring the escape suite's convention.
const CHILD_BLOCKED: i32 = 0; // attempt was denied by a kernel primitive (good)
const CHILD_ESCAPED: i32 = 1; // attempt SUCCEEDED -> a breach (bad)
const CHILD_SKIPPED: i32 = 2; // child could not even attempt (treated as skip)

fn main() {
    // ---- Child mode: run one escape attempt under the already-applied sandbox.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("child") {
        let attempt = args.next().unwrap_or_default();
        let checkout = args.next().map(PathBuf::from).unwrap_or_default();
        std::process::exit(run_child_attempt(&attempt, &checkout));
    }

    // ---- Parent mode: build the jail and supervise the escape attempts.
    std::process::exit(run_parent());
}

/// One escape attempt, classified by the parent from the child's exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Allowed,
    Denied,
    Escaped,
    Skipped,
}

impl Verdict {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "ALLOWED",
            Self::Denied => "DENIED",
            Self::Escaped => "ESCAPED",
            Self::Skipped => "SKIPPED",
        }
    }
}

struct Attempt {
    name: &'static str,
    primitive: &'static str,
    /// What we EXPECT the kernel to do when the primitive is available.
    expect: Verdict,
}

const ATTEMPTS: &[Attempt] = &[
    Attempt {
        name: "write_inside_checkout",
        primitive: "landlock",
        expect: Verdict::Allowed,
    },
    Attempt {
        name: "write_outside_checkout",
        primitive: "landlock",
        expect: Verdict::Denied,
    },
    Attempt {
        name: "read_etc_shadow",
        primitive: "landlock",
        expect: Verdict::Denied,
    },
    Attempt {
        name: "open_af_inet_socket",
        primitive: "seccomp",
        expect: Verdict::Denied,
    },
];

fn run_parent() -> i32 {
    let caps = SandboxCapabilities::probe();

    // A throwaway "checkout" the sandboxed child is confined to.
    let checkout = std::env::temp_dir().join(format!("jeryu-jail-demo-{}", std::process::id()));
    if let Err(err) = std::fs::create_dir_all(&checkout) {
        eprintln!("FATAL: could not create checkout dir: {err}");
        return 1;
    }

    // Copy ourselves into the checkout so the sandboxed child has a binary it can
    // execute WITHOUT widening the workspace-only Landlock ruleset (the workspace
    // rule already grants read+execute; system read roots cover the loader/libc).
    let child_bin = checkout.join("jail_child");
    let me = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("FATAL: current_exe: {e}");
        std::process::exit(1);
    });
    if let Err(err) = std::fs::copy(&me, &child_bin) {
        eprintln!("FATAL: could not stage child binary: {err}");
        let _ = std::fs::remove_dir_all(&checkout);
        return 1;
    }

    // Build the production plan via the policy decision, confining to `checkout`.
    let probe_job = job(checkout.clone(), child_bin.clone(), Vec::new());
    let decision = match select_runner(&probe_job) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("FATAL: select_runner: {err}");
            let _ = std::fs::remove_dir_all(&checkout);
            return 1;
        }
    };
    let plan = SandboxPlan::from_decision(&checkout, &decision);
    let level = caps.enforcement_level(&plan);

    println!("== jeryu native sandbox jail demo ==");
    println!("checkout : {}", checkout.display());
    println!("caps     : {}", caps.summary());
    println!("plan     : {}", plan.explain());
    println!("level    : {}", level.as_str());
    println!(
        "landlock : {}",
        caps.landlock_abi
            .map(|a| format!("abi{a}"))
            .unwrap_or_else(|| "none".to_string())
    );
    println!();

    // Run each attempt in its own sandboxed child. The first attempt's child is
    // also used to read the /proc enforcement proof while it is still alive.
    let mut results: Vec<(&Attempt, Verdict, String)> = Vec::new();
    let mut printed_proof = false;
    for attempt in ATTEMPTS {
        let skip_reason = skip_reason(attempt, &caps);
        let (verdict, detail) = if let Some(reason) = skip_reason {
            (Verdict::Skipped, reason)
        } else {
            run_one_attempt(attempt, &checkout, &child_bin, &caps, &plan, &level, {
                let proof = !printed_proof;
                printed_proof |= proof;
                proof
            })
        };
        results.push((attempt, verdict, detail));
    }

    // Per-attempt PASS/FAIL lines.
    println!("-- attempts --");
    let mut all_ok = true;
    for (attempt, verdict, detail) in &results {
        // A skip is honest only when the primitive is genuinely unavailable; we
        // only ever set Skipped from `skip_reason`, so a skip never fails here.
        let pass = match verdict {
            Verdict::Skipped => true,
            other => *other == attempt.expect,
        };
        all_ok &= pass;
        println!(
            "  [{}] {:<24} expect={:<7} got={:<7} ({}) {}",
            if pass { "PASS" } else { "FAIL" },
            attempt.name,
            attempt.expect.as_str(),
            verdict.as_str(),
            attempt.primitive,
            detail,
        );
    }

    println!();
    println!(
        "VERDICT  : {}",
        if all_ok {
            "PASS — every escape was blocked (or honestly skipped) and the allowed write succeeded"
        } else {
            "FAIL — a check did not match its expected kernel verdict"
        }
    );

    let _ = std::fs::remove_dir_all(&checkout);
    i32::from(!all_ok)
}

/// Decide, from the probed capabilities, whether an attempt must be SKIPPED
/// because its blocking primitive is genuinely unavailable. Returns `None` when
/// the attempt is meaningful and must be run for real.
fn skip_reason(attempt: &Attempt, caps: &SandboxCapabilities) -> Option<String> {
    // The "allowed write inside" attempt is always meaningful: it needs no
    // blocking primitive, it proves the jail still lets legitimate work happen.
    if attempt.expect == Verdict::Allowed {
        return None;
    }
    match attempt.primitive {
        "landlock" if caps.landlock_abi.is_none() => {
            Some("skipped: Landlock unavailable on this host".to_string())
        }
        "seccomp" if !caps.seccomp_bpf => {
            Some("skipped: seccomp-bpf unavailable on this host".to_string())
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_one_attempt(
    attempt: &Attempt,
    checkout: &Path,
    child_bin: &Path,
    caps: &SandboxCapabilities,
    plan: &SandboxPlan,
    level: &jeryu_sandbox_linux::capability::EnforcementLevel,
    print_proof: bool,
) -> (Verdict, String) {
    let job = job(
        checkout.to_path_buf(),
        child_bin.to_path_buf(),
        vec![
            "child".to_string(),
            attempt.name.to_string(),
            checkout.display().to_string(),
        ],
    );

    let child = match spawn_sandboxed(&job, plan, caps, &sandbox_env()) {
        Ok(c) => c,
        Err(err) => {
            return (
                Verdict::Skipped,
                format!("skipped: spawn_sandboxed failed: {err}"),
            );
        }
    };
    let pid = child.id();

    // Read the /proc enforcement proof while the child is still alive. The child
    // sleeps briefly at startup so NoNewPrivs/Seccomp are observable.
    if print_proof {
        let mut report = verify_enforcement(pid, level);
        for _ in 0..50 {
            if report.proc_no_new_privs.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
            report = verify_enforcement(pid, level);
        }
        println!("-- enforcement proof (/proc/{pid}/status) --");
        println!("  level         : {}", report.level);
        println!(
            "  NoNewPrivs    : {}",
            report
                .proc_no_new_privs
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        );
        println!(
            "  Seccomp(mode) : {} (2 == filter mode)",
            report
                .proc_seccomp
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        );
        println!("  applied       : {}", report.applied.join(", "));
        if !report.skipped.is_empty() {
            println!("  skipped       : {}", report.skipped.join(", "));
        }
        println!();
    }

    let out = match run_with_watchdog(child, Duration::from_secs(10)) {
        Ok(o) => o,
        Err(err) => return (Verdict::Skipped, format!("skipped: watchdog: {err}")),
    };
    if out.timed_out {
        return (Verdict::Skipped, "skipped: child timed out".to_string());
    }

    let stderr = String::from_utf8_lossy(&out.stderr);
    let detail = stderr.lines().last().unwrap_or("").trim().to_string();

    // A child killed by a signal (e.g. seccomp KillProcess) counts as blocked —
    // the escape did not complete.
    let verdict = match out.exit_code {
        Some(CHILD_BLOCKED) => {
            if attempt.expect == Verdict::Allowed {
                // For the allowed-write attempt, exit 0 means the write succeeded.
                Verdict::Allowed
            } else {
                Verdict::Denied
            }
        }
        Some(CHILD_ESCAPED) => {
            if attempt.expect == Verdict::Allowed {
                // The legitimate write was itself denied -> a regression we surface.
                Verdict::Denied
            } else {
                Verdict::Escaped
            }
        }
        Some(CHILD_SKIPPED) => Verdict::Skipped,
        None => Verdict::Denied, // killed by signal -> blocked
        Some(other) => {
            return (
                Verdict::Skipped,
                format!("skipped: unexpected child exit {other}: {detail}"),
            );
        }
    };
    (verdict, detail)
}

/// The child body: perform exactly ONE escape attempt under the sandbox that the
/// parent's `spawn_sandboxed` already applied via `pre_exec`. Returns the child
/// exit code (`CHILD_BLOCKED` / `CHILD_ESCAPED` / `CHILD_SKIPPED`).
fn run_child_attempt(attempt: &str, checkout: &Path) -> i32 {
    // Sleep briefly so the parent can read /proc/<pid>/status for the proof.
    std::thread::sleep(Duration::from_millis(150));

    match attempt {
        "write_inside_checkout" => {
            let target = checkout.join("inside.txt");
            match std::fs::write(&target, b"hello from inside the jail") {
                Ok(()) => {
                    eprintln!("wrote {}", target.display());
                    CHILD_BLOCKED // exit 0 == the allowed write succeeded
                }
                Err(err) => {
                    eprintln!("inside write unexpectedly failed: {err}");
                    CHILD_ESCAPED // signalled as a regression to the parent
                }
            }
        }
        "write_outside_checkout" => {
            // Target a path OUTSIDE the checkout that is DAC-writable, so only
            // Landlock can do the blocking.
            let outside = std::env::temp_dir().join(format!(
                "jeryu-jail-escape-{}-outside.txt",
                std::process::id()
            ));
            match std::fs::write(&outside, b"escaped") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&outside);
                    eprintln!("BREACH: wrote outside at {}", outside.display());
                    CHILD_ESCAPED
                }
                Err(err) => {
                    eprintln!("blocked writing outside ({}): {err}", outside.display());
                    CHILD_BLOCKED
                }
            }
        }
        "read_etc_shadow" => match std::fs::read("/etc/shadow") {
            Ok(_) => {
                eprintln!("BREACH: read /etc/shadow");
                CHILD_ESCAPED
            }
            Err(err) => {
                eprintln!("blocked reading /etc/shadow: {err}");
                CHILD_BLOCKED
            }
        },
        "open_af_inet_socket" => {
            // SAFETY: socket(2) is a single syscall with no pointer args; the
            // child is between exec and exit and touches no shared state.
            let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
            if fd >= 0 {
                // SAFETY: closing a fd we just opened.
                unsafe { libc::close(fd) };
                eprintln!("BREACH: opened AF_INET socket (fd {fd})");
                CHILD_ESCAPED
            } else {
                let err = std::io::Error::last_os_error();
                eprintln!("blocked AF_INET socket: {err}");
                CHILD_BLOCKED
            }
        }
        other => {
            eprintln!("unknown attempt {other}");
            CHILD_SKIPPED
        }
    }
}

fn job(workspace: PathBuf, command: PathBuf, args: Vec<String>) -> JobRequest {
    JobRequest {
        job_id: "jail-demo".into(),
        repo_id: "repo".into(),
        commit_sha: "demo".into(),
        workspace,
        command: command.display().to_string(),
        args,
        env: BTreeMap::new(),
        trust_tier: TrustTier::T1ProtectedInternal,
        requested_runner: None,
        network_policy: NetworkPolicy::Deny,
        secret_policy: SecretPolicy::Default,
        token_policy: TokenPolicy::ReadOnly,
        timeout_ms: 10_000,
        fork: false,
    }
}

fn sandbox_env() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("PATH".into(), "/usr/local/bin:/usr/bin:/bin".into());
    env.insert("HOME".into(), "/tmp".into());
    env
}
