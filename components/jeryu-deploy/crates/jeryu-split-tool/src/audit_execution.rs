//! Bounded read-only auditor execution and source-bound report collection.
use super::*;
use std::process::{Command, Stdio};

pub(super) struct Executor<'a> {
    pub(super) root: &'a Path,
    pub(super) out: &'a Path,
    pub(super) auditor: &'a Path,
    pub(super) binary_hash: &'a str,
    pub(super) version: &'a str,
    pub(super) timeout_seconds: u64,
    pub(super) governing: Option<&'a str>,
}

pub(super) fn execute(executor: &Executor<'_>, row: &mut Row, index: usize) -> Result<()> {
    let path = source_path(executor.root, &row.source)?;
    execute_at(executor, row, index, &path)
}

// Called only with normal source_path admission or a freshly verified Acquired
// path. Neither reports nor arbitrary CLI path arguments supply this value.
pub(super) fn execute_at(
    executor: &Executor<'_>,
    row: &mut Row,
    index: usize,
    path: &Path,
) -> Result<()> {
    let git_root = PathBuf::from(git(path, &["rev-parse", "--show-toplevel"])?);
    let before = snapshot(&git_root)?;
    if let Some(expected) = &row.source.commit {
        ensure!(&before.0 == expected, "wrong dependency commit");
    }
    let scope_path = path.strip_prefix(&git_root)?;
    let tree = if scope_path.as_os_str().is_empty() {
        before.1.clone()
    } else {
        git(
            &git_root,
            &["rev-parse", &format!("HEAD:{}", scope_path.display())],
        )?
    };
    if let Some(expected) = &row.commit {
        ensure!(
            &before.0 == expected,
            "public acquisition commit changed before audit"
        );
    }
    if let Some(expected) = &row.tree {
        ensure!(
            &tree == expected,
            "public acquisition tree changed before audit"
        );
    }
    row.commit = Some(before.0.clone());
    row.tree = Some(tree);
    let policy_path = path.join("agent/audit-policy.toml");
    let policy = fs::read_to_string(&policy_path).context("missing owning policy")?;
    row.policy_sha256 = Some(hash(policy.as_bytes()));
    let owner = row
        .source
        .repository
        .strip_prefix("neverhuman/")
        .context("owned repository")?;
    let minimum = audit_evidence::policy(&policy, owner, row.source.minimum)?;
    let governing_error = if let Some(governing) = executor.governing {
        let relative = policy_path.strip_prefix(&git_root)?;
        match crate::split_tree::source_git_command(&git_root)
            .env("GIT_NO_LAZY_FETCH", "1")
            .args(["show", &format!("{governing}:{}", relative.display())])
            .output()
        {
            Ok(protected) if protected.status.success() => {
                row.governing_policy_sha256 = Some(hash(&protected.stdout));
                // Any policy edit requires its own reviewed admission. Neither a
                // weaker candidate nor new allowlists can supply its own gate.
                if protected.stdout != policy.as_bytes() {
                    Some("candidate and governing policies differ")
                } else {
                    None
                }
            }
            _ => Some("governing policy source unavailable"),
        }
    } else {
        Some("authenticated protected governing policy has not been supplied")
    };
    let attempt = executor.out.join(format!("{index:04}"));
    fs::DirBuilder::new().mode(0o700).create(&attempt)?;
    let report = attempt.join("report.json");
    let markdown = attempt.join("report.md");
    let log = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(attempt.join("audit.log"))?;
    ensure!(
        hash_binary(executor.auditor)? == executor.binary_hash,
        "auditor executable changed"
    );
    // Expose system executables/libraries, this source and this attempt only.
    // Host homes, credentials and neighboring checkouts are not mounted.
    let status = Command::new("/usr/bin/timeout")
        .args(["--signal=TERM", "--kill-after=10s"])
        .arg(executor.timeout_seconds.to_string())
        .arg("/usr/bin/bwrap")
        .args([
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/bin",
            "/bin",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind-try",
            "/lib64",
            "/lib64",
            "--dir",
            "/etc",
            "--ro-bind-try",
            "/etc/ld.so.cache",
            "/etc/ld.so.cache",
            "--tmpfs",
            "/tmp",
            "--unshare-pid",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
            "--ro-bind",
        ])
        // Keep parent Git metadata visible for component scopes under /tmp.
        .arg(&git_root)
        .arg(&git_root)
        .arg("--bind")
        .arg(&attempt)
        .arg(&attempt)
        // ro-bind canonicalizes proc-FD paths; bind-data consumes exact held bytes.
        .args([
            "--perms",
            "0500",
            "--ro-bind-data",
            "0",
            "/tmp/jeryu-auditor",
        ])
        .args([
            "--unshare-net",
            "--die-with-parent",
            "--new-session",
            "--chdir",
        ])
        .arg(path)
        .arg("/tmp/jeryu-auditor")
        .args([
            "audit",
            ".",
            "--full",
            "--mode",
            "standard",
            "--no-score-history",
            "--fail-on",
            "critical,high",
            "--fail-under",
        ])
        .arg(minimum.to_string())
        .arg("--policy")
        .arg(&policy_path)
        .arg("--json")
        .arg(&report)
        .arg("--md")
        .arg(&markdown)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", "/tmp")
        .env("TMPDIR", "/tmp")
        .env("JANKURAI_NO_UPDATE_CHECK", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::from(fs::File::open(executor.auditor)?))
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .status()?;
    row.command_exit = status.code();
    ensure!(
        snapshot(&git_root)? == before,
        "audited source changed during execution"
    );
    ensure!(
        hash_binary(executor.auditor)? == executor.binary_hash,
        "auditor executable changed during execution"
    );
    if matches!(status.code(), Some(124 | 137)) {
        row.status = "timed_out";
        row.reason = "audit exceeded its execution deadline".into();
        return Ok(());
    }
    let bytes = read_report(&report).context(
        "auditor did not produce a complete regular report; inspect retained private audit.log",
    )?;
    row.report_sha256 = Some(hash(&bytes));
    let summary = audit_evidence::admit(
        &bytes,
        Binding {
            commit: &before.0,
            version: executor.version,
            // The pinned producer records its own relative policy path. It
            // ignores --policy; the actual owning file is independently hashed.
            policy_path: "./agent/audit-policy.toml",
            minimum,
            max_soft: (owner == "jeryu").then_some(0),
        },
        status.code().context("auditor terminated by signal")?,
    )?;
    row.status = if summary.passed && governing_error.is_none() {
        "passed"
    } else {
        "failed_policy"
    };
    row.reason = governing_error
        .unwrap_or(if summary.passed {
            "complete local score admission"
        } else {
            "score, findings, caps, ratchet or conformance failed"
        })
        .into();
    row.summary = Some(summary);
    Ok(())
}
