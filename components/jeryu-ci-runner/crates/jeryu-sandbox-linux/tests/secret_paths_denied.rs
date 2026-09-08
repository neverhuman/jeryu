//! The workcell promise, asserted negatively: a Landlock jail scoped to ONLY the
//! agent's workspace must DENY reads of the host's secrets and of other repos,
//! while still permitting reads INSIDE the workspace.
//!
//! This is the north-star's core guarantee — "an agent gets only the repo it
//! claims and nothing else on the system" — so we prove it directly against the
//! exact paths it protects:
//!   * `~/.ssh/id_rsa`-shaped key material,
//!   * `~/.jeryu/jeryu.env`-shaped CI secrets,
//!   * a sibling repo the cell did NOT claim.
//!
//! The decoys are staged in a tempdir and made world-readable (chmod 0644) so a
//! DENIAL is provably Landlock's doing, not a DAC accident — targeting the real
//! `~/.ssh` (unreadable by DAC anyway) would prove nothing (the tautology trap
//! documented in `escape_suite.rs`). Following the escape-suite discipline, the
//! whole test honestly skips when this host has no Landlock; when Landlock IS
//! present, every denial MUST be real (`Blocked`, never `Skipped`).

use jeryu_sandbox_linux::capability::SandboxCapabilities;
use jeryu_sandbox_linux::{EscapeVerdict, run_in_forked_child};
use std::path::Path;

/// Apply a Landlock ruleset granting full access to ONLY `workspace` plus the
/// read-only system roots the loader needs. Copied verbatim from
/// `escape_suite.rs:235` — integration-test helpers are private per test crate
/// and cannot be imported, and this is exactly the workspace-only allowlist the
/// real launch path installs.
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
    if let Ok(fd) = PathFd::new(workspace) {
        ruleset = ruleset
            .add_rule(PathBeneath::new(fd, AccessFs::from_all(abi)))
            .map_err(|e| e.to_string())?;
    }
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

/// Stage a decoy and make it world-readable, so any denial is provably Landlock
/// (not DAC). Returns nothing — panics on a seed failure (a broken test setup).
fn stage_readable(path: &Path, body: &[u8]) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("mkdir {}: {e}", parent.display()));
    }
    std::fs::write(path, body).unwrap_or_else(|e| panic!("seed {}: {e}", path.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(path, perms).expect("chmod decoy 0644");
    }
}

#[test]
fn secrets_and_other_repos_outside_the_workspace_are_denied_inside_the_jail() {
    let caps = SandboxCapabilities::probe();
    eprintln!("SECRET-PATHS caps: {}", caps.summary());

    // Decoys OUTSIDE the workspace: host secrets + a repo the cell never claimed.
    let outside = tempfile::tempdir().unwrap_or_else(|e| panic!("decoy tempdir: {e}"));
    let ssh_key = outside.path().join("home/.ssh/id_rsa");
    let jeryu_env = outside.path().join("home/.jeryu/jeryu.env");
    let other_repo = outside.path().join("repos/unclaimed-repo/src/secret.rs");
    // Decoy CONTENT is irrelevant to the test (we only prove the read is denied),
    // so it is deliberately NOT secret-shaped — no PEM header / token= pattern —
    // to avoid tripping repo secret scanners on this fixture.
    stage_readable(
        &ssh_key,
        b"placeholder ssh key fixture (read must be denied)\n",
    );
    stage_readable(
        &jeryu_env,
        b"JERYU_DECOY_SETTING=placeholder-not-a-secret\n",
    );
    stage_readable(&other_repo, b"pub const DECOY: &str = \"placeholder\";\n");

    // The workspace the cell IS confined to (a separate tempdir).
    let workspace = tempfile::tempdir().unwrap_or_else(|e| panic!("ws tempdir: {e}"));
    let in_ws = workspace.path().join("claimed.rs");
    std::fs::write(&in_ws, b"// the one file the agent may read\n").expect("seed in-ws file");

    // -- Every off-limits path must be BLOCKED inside the jail ----------------
    for (label, decoy) in [
        ("~/.ssh key", &ssh_key),
        ("~/.jeryu CI secret", &jeryu_env),
        ("unclaimed sibling repo", &other_repo),
    ] {
        let ws = workspace.path().to_path_buf();
        let target = decoy.clone();
        let verdict = if caps.landlock_abi.is_none() {
            eprintln!("SKIP: no Landlock; cannot prove {label} is denied");
            EscapeVerdict::Skipped
        } else {
            run_in_forked_child(move || {
                if apply_landlock_workspace_only(&ws).is_err() {
                    return 2; // primitive setup failed -> honest skip
                }
                match std::fs::read(&target) {
                    Ok(_) => 1,  // ESCAPED: read a secret outside the workspace
                    Err(_) => 0, // blocked by Landlock (desired)
                }
            })
        };
        eprintln!("  {label} ({}) => {}", decoy.display(), verdict.as_str());
        assert_ne!(
            verdict,
            EscapeVerdict::Escaped,
            "BREACH: the jailed agent read {label} outside its workspace: {}",
            decoy.display()
        );
        // Asymmetry: with Landlock present, the denial MUST be real, not a skip.
        if caps.landlock_abi.is_some() {
            assert_eq!(
                verdict,
                EscapeVerdict::Blocked,
                "Landlock present: {label} read MUST be Blocked, not Skipped"
            );
        }
    }

    // -- A read INSIDE the workspace must still SUCCEED -----------------------
    // (the jail is a workspace-scoped allowlist, not a blanket deny). The byte
    // map is INVERTED here: the child returns 0 (=> Blocked verdict) when the
    // in-workspace read SUCCEEDS, which is the outcome we want to confirm.
    if caps.landlock_abi.is_some() {
        let ws = workspace.path().to_path_buf();
        let allowed = in_ws.clone();
        let verdict = run_in_forked_child(move || {
            if apply_landlock_workspace_only(&ws).is_err() {
                return 2;
            }
            match std::fs::read(&allowed) {
                Ok(_) => 0,  // in-workspace read succeeded (expected)
                Err(_) => 1, // wrongly denied a file the agent owns
            }
        });
        assert_eq!(
            verdict,
            EscapeVerdict::Blocked,
            "a read INSIDE the workspace must succeed under the workspace-only jail"
        );
    }
}
